use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use std::fs;
use tokio::time::{sleep, Duration};

#[derive(Parser)]
#[command(name = "apollo-hub", about = "APOLLO Hub — Fleet Coordination Layer")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Hub coordination service
    Start {
        #[arg(short, long, default_value = "0.0.0.0:9090")]
        listen: String,
        #[arg(short, long, default_value = ".apollo/hub_nodes.json")]
        storage: PathBuf,
    },
    /// Register a node with the hub
    Add {
        #[arg(long)] ip:   String,
        #[arg(long)] key:  String,
        #[arg(long, default_value = "edge-node")] name: String,
        #[arg(short, long, default_value = ".apollo/hub_nodes.json")] storage: PathBuf,
    },
    /// List all registered nodes
    List {
        #[arg(short, long, default_value = ".apollo/hub_nodes.json")] storage: PathBuf,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct NodeRecord {
    name:   String,
    ip:     String,
    key:    String,
    #[serde(default)]
    status: NodeStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct NodeStatus {
    is_online:     bool,
    active_agents: usize,
    max_agents:    usize,
    last_seen:     u64,
    failure_count: u32,
}

#[derive(Deserialize)]
struct MetricsResponse {
    active_agents: usize,
    max_agents:    usize,
}

/// A catalog entry aggregated from a node's agent list.
#[derive(Serialize, Deserialize, Clone, Debug)]
struct CatalogEntry {
    agent_id:     String,
    version:      String,
    runtime:      String,
    capabilities: Vec<String>,
    checksum:     String,
    available_on: Vec<String>, // node IPs that have this agent registered
}

struct HubState {
    nodes:   Mutex<Vec<NodeRecord>>,
    catalog: Mutex<Vec<CatalogEntry>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Start { listen, storage } => {
            let nodes = load_nodes(&storage).unwrap_or_default();
            let state = Arc::new(HubState {
                nodes:   Mutex::new(nodes),
                catalog: Mutex::new(Vec::new()),
            });

            // Health + catalog poller
            let poller = Arc::clone(&state);
            let storage_clone = storage.clone();
            tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(3))
                    .build()
                    .unwrap();
                let mut tick = 0u64;
                loop {
                    tick += 1;
                    let nodes_snap = { poller.nodes.lock().unwrap().clone() };
                    let _now = now_unix();

                    for node in nodes_snap {
                        // Circuit breaker: back off unhealthy nodes
                        if node.status.failure_count >= 5 && tick % 6 != 0 { continue; }

                        let client_c = client.clone();
                        let poller_c = Arc::clone(&poller);
                        let storage_c = storage_clone.clone();

                        tokio::spawn(async move {
                            // Poll /metrics
                            let metrics_url = format!("http://{}/metrics", node.ip);
                            let res = client_c.get(&metrics_url)
                                .header("X-Apollo-Key", &node.key)
                                .send().await;

                            let mut status = node.status.clone();
                            match res {
                                Ok(r) if r.status().is_success() => {
                                    if let Ok(m) = r.json::<MetricsResponse>().await {
                                        status.is_online     = true;
                                        status.active_agents = m.active_agents;
                                        status.max_agents    = m.max_agents;
                                        status.last_seen     = now_unix();
                                        status.failure_count = 0;
                                    }
                                }
                                _ => {
                                    status.is_online = false;
                                    status.failure_count += 1;
                                }
                            }

                            // Poll /agents/list every 5th tick (less frequently)
                            if tick % 5 == 0 && status.is_online {
                                tokio::time::sleep(Duration::from_millis(50)).await;
                                let list_url = format!("http://{}/agents/list", node.ip);
                                match client_c.get(&list_url)
                                    .header("X-Apollo-Key", &node.key)
                                    .send().await
                                {
                                    Err(_) => {}
                                    Ok(r) => {
                                    if r.status().is_success() {
                                    let text = r.text().await.unwrap_or_default();
                                    if let Ok(records) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                                        let mut catalog = poller_c.catalog.lock().unwrap();
                                        for record in records {
                                            let id = record["id"].as_str().unwrap_or("").to_string();
                                            let version = record["spec"]["version"].as_str().unwrap_or("").to_string();
                                            let runtime = record["spec"]["runtime"]["type"].as_str()
                                                .or_else(|| record["spec"]["runtime"]["kind"].as_str())
                                                .unwrap_or("").to_string();
                                            let capabilities: Vec<String> = record["spec"]["capabilities"]
                                                .as_array()
                                                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                                                .unwrap_or_default();
                                            let checksum = record["checksum"].as_str().unwrap_or("").to_string();

                                            if let Some(entry) = catalog.iter_mut().find(|e| e.agent_id == id) {
                                                if !entry.available_on.contains(&node.ip) {
                                                    entry.available_on.push(node.ip.clone());
                                                }
                                                entry.version = version;
                                            } else {
                                                catalog.push(CatalogEntry {
                                                    agent_id: id,
                                                    version,
                                                    runtime,
                                                    capabilities,
                                                    checksum,
                                                    available_on: vec![node.ip.clone()],
                                                });
                                            }
                                        }
                                    }
                                    } // if r.status().is_success()
                                    } // Ok(r)
                                }
                            }

                            let mut nodes = poller_c.nodes.lock().unwrap();
                            if let Some(pos) = nodes.iter().position(|n| n.ip == node.ip) {
                                nodes[pos].status = status;
                            }

                            // Persist node state
                            let snap: Vec<NodeRecord> = nodes.clone();
                            let _ = save_nodes(&storage_c, &snap);
                        });
                    }
                    sleep(Duration::from_secs(10)).await;
                }
            });

            // Run the blocking tiny_http server on a dedicated OS thread so
            // the tokio async runtime remains free to drive background tasks.
            let listen_clone = listen.clone();
            tokio::task::spawn_blocking(move || {
                if let Err(e) = run_api_server_blocking(&listen_clone, state) {
                    eprintln!("Hub API error: {}", e);
                }
            }).await?;
        }

        Commands::Add { ip, key, name, storage } => {
            let mut nodes = load_nodes(&storage).unwrap_or_default();
            if nodes.iter().any(|n| n.ip == ip) {
                println!("Node {} already registered.", ip);
            } else {
                nodes.push(NodeRecord { name: name.clone(), ip: ip.clone(), key, status: Default::default() });
                save_nodes(&storage, &nodes)?;
                println!("✓ Registered node '{}' at {}", name, ip);
            }
        }

        Commands::List { storage } => {
            let nodes = load_nodes(&storage).unwrap_or_default();
            if nodes.is_empty() {
                println!("No nodes registered.");
            } else {
                println!("{:<15} {:<22} {:<10} {:<6} {}", "NAME", "IP", "STATUS", "FAIL", "AGENTS");
                for n in nodes {
                    let status = if n.status.is_online { "ONLINE" } else if n.status.failure_count > 0 { "FAILED" } else { "UNKNOWN" };
                    println!("{:<15} {:<22} {:<10} {:<6} {}/{}",
                        n.name, n.ip, status, n.status.failure_count,
                        n.status.active_agents, n.status.max_agents);
                }
            }
        }
    }
    Ok(())
}

fn run_api_server_blocking(listen: &str, state: Arc<HubState>) -> Result<()> {
    let server = tiny_http::Server::http(listen).map_err(|e| anyhow!(e))?;
    println!("APOLLO Hub listening on http://{}", listen);

    for req in server.incoming_requests() {
        let url = req.url().to_string();
        match url.as_str() {

            // Fleet node status
            "/status" | "/nodes/status" => {
                let nodes = state.nodes.lock().unwrap().clone();
                let json = serde_json::to_string(&nodes).unwrap_or_default();
                respond_json(req, json);
            }

            // Best node (fewest active agents, must be online)
            "/nodes/best" => {
                let nodes = state.nodes.lock().unwrap();
                let best  = nodes.iter()
                    .filter(|n| n.status.is_online && n.status.active_agents < n.status.max_agents)
                    .min_by_key(|n| n.status.active_agents);
                match best {
                    Some(n) => {
                        let json = format!(r#"{{"node":"{}","ip":"{}","active_agents":{},"max_agents":{}}}"#,
                            n.name, n.ip, n.status.active_agents, n.status.max_agents);
                        respond_json(req, json);
                    }
                    None => {
                        let _ = req.respond(tiny_http::Response::from_string(
                            r#"{"error":"no available nodes"}"#).with_status_code(503));
                    }
                }
            }

            // Agent catalog (aggregated across all nodes)
            "/catalog" => {
                let catalog = state.catalog.lock().unwrap().clone();
                let json = serde_json::to_string(&catalog).unwrap_or_default();
                respond_json(req, json);
            }

            // Fleet summary
            "/summary" => {
                let nodes = state.nodes.lock().unwrap();
                let nodes_total: usize = nodes.len();
                let total_capacity: usize = nodes.iter().map(|n| n.status.max_agents).sum();
                let total_active:   usize = nodes.iter().map(|n| n.status.active_agents).sum();
                let online: usize = nodes.iter().filter(|n| n.status.is_online).count();
                drop(nodes);
                let catalog_count = state.catalog.lock().unwrap().len();
                let json = format!(
                    r#"{{"nodes_total":{},"nodes_online":{},"agents_active":{},"fleet_capacity":{},"catalog_agents":{}}}"#,
                    nodes_total, online, total_active, total_capacity, catalog_count
                );
                respond_json(req, json);
            }

            _ => {
                let _ = req.respond(tiny_http::Response::from_string("HUB ACTIVE — try /status /nodes/best /catalog /summary"));
            }
        }
    }
    Ok(())
}

fn respond_json(req: tiny_http::Request, body: String) {
    let _ = req.respond(
        tiny_http::Response::from_string(body)
            .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json").unwrap()),
    );
}

fn load_nodes(path: &PathBuf) -> Result<Vec<NodeRecord>> {
    if !path.exists() { return Ok(vec![]); }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn save_nodes(path: &PathBuf, nodes: &[NodeRecord]) -> Result<()> {
    if let Some(p) = path.parent() { fs::create_dir_all(p)?; }
    fs::write(path, serde_json::to_string_pretty(nodes)?)?;
    Ok(())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
