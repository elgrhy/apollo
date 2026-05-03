use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use std::fs;
use tokio::time::{sleep, Duration};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(name = "mars-hub")]
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
        
        #[arg(short, long, default_value = ".mars/hub_nodes.json")]
        storage: PathBuf,
    },
    /// Register a new MARS Node
    Add {
        #[arg(long)]
        ip: String,
        #[arg(long)]
        key: String,
        #[arg(long, default_value = "edge-node")]
        name: String,
    },
    /// List all registered nodes
    List,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct NodeRecord {
    name: String,
    ip: String,
    key: String,
    #[serde(default)]
    status: NodeStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
struct NodeStatus {
    is_online: bool,
    active_agents: usize,
    max_agents: usize,
    last_seen: u64,
    failure_count: u32,
}

#[derive(Deserialize)]
struct MetricsResponse {
    active_agents: usize,
    max_agents: usize,
}

struct HubState {
    nodes: Mutex<Vec<NodeRecord>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { listen, storage } => {
            let nodes = load_nodes(&storage).unwrap_or_default();
            let state = Arc::new(HubState {
                nodes: Mutex::new(nodes),
            });

            // Reliability Poller (Circuit Breaker Implementation)
            let poller_state = Arc::clone(&state);
            tokio::spawn(async move {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(2))
                    .build()
                    .unwrap();

                loop {
                    let nodes_to_poll = {
                        let nodes = poller_state.nodes.lock().unwrap();
                        nodes.clone()
                    };

                    let now = now_unix();
                    for node in nodes_to_poll {
                        // Circuit Breaker: Back-off for unhealthy nodes
                        if node.status.failure_count >= 5 && now % 6 != 0 {
                            continue;
                        }

                        let client_clone = client.clone();
                        let poller_state_clone = Arc::clone(&poller_state);
                        
                        tokio::spawn(async move {
                            let url = format!("http://{}/metrics", node.ip);
                            let res = client_clone.get(&url)
                                .header("X-Mars-Key", &node.key)
                                .send()
                                .await;

                            let mut status = node.status;
                            match res {
                                Ok(resp) if resp.status().is_success() => {
                                    if let Ok(metrics) = resp.json::<MetricsResponse>().await {
                                        status.is_online = true;
                                        status.active_agents = metrics.active_agents;
                                        status.max_agents = metrics.max_agents;
                                        status.last_seen = now_unix();
                                        status.failure_count = 0;
                                    }
                                }
                                _ => {
                                    status.is_online = false;
                                    status.failure_count += 1;
                                }
                            }

                            let mut nodes = poller_state_clone.nodes.lock().unwrap();
                            if let Some(pos) = nodes.iter().position(|n| n.ip == node.ip) {
                                nodes[pos].status = status;
                            }
                        });
                    }
                    sleep(Duration::from_secs(10)).await;
                }
            });

            run_api_server(&listen, state).await?;
        }
        Commands::Add { ip, key, name } => {
            println!("✓ Registering node: {}", ip);
            let path = PathBuf::from(".mars/hub_nodes.json");
            let mut nodes = load_nodes(&path).unwrap_or_default();
            nodes.push(NodeRecord { name, ip, key, status: Default::default() });
            save_nodes(&path, &nodes)?;
        }
        Commands::List => {
            let path = PathBuf::from(".mars/hub_nodes.json");
            let nodes = load_nodes(&path).unwrap_or_default();
            for n in nodes {
                let status = if n.status.is_online { "ONLINE" } else if n.status.failure_count > 0 { "FAILED" } else { "UNKNOWN" };
                println!("{:<15} {:<20} {:<10} (failures: {})", n.name, n.ip, status, n.status.failure_count);
            }
        }
    }

    Ok(())
}

async fn run_api_server(listen: &str, state: Arc<HubState>) -> Result<()> {
    let server = tiny_http::Server::http(listen).map_err(|e| anyhow!(e))?;
    println!("MARS Hub listening on http://{}", listen);
    for request in server.incoming_requests() {
        if request.url() == "/status" {
            let nodes = state.nodes.lock().unwrap();
            let json = serde_json::to_string(&*nodes)?;
            let _ = request.respond(tiny_http::Response::from_string(json).with_header(tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).unwrap()));
        } else {
            let _ = request.respond(tiny_http::Response::from_string("HUB ACTIVE"));
        }
    }
    Ok(())
}

fn load_nodes(path: &PathBuf) -> Result<Vec<NodeRecord>> {
    if !path.exists() { return Ok(vec![]); }
    let raw = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&raw)?)
}

fn save_nodes(path: &PathBuf, nodes: &Vec<NodeRecord>) -> Result<()> {
    if let Some(parent) = path.parent() { fs::create_dir_all(parent)?; }
    let json = serde_json::to_string_pretty(nodes)?;
    fs::write(path, json)?;
    Ok(())
}

fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}
