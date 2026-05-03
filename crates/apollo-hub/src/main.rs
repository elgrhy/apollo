use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use std::fs;
use tokio::time::{sleep, Duration};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "apollo-hub", about = "APOLLO Hub — Fleet Coordination Layer v1.2")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the Hub coordination service
    Start {
        #[arg(short, long, default_value = "0.0.0.0:9191")]
        listen: String,
        #[arg(short, long, default_value = ".apollo/hub_nodes.json")]
        storage: PathBuf,
        /// Webhook URL to call when fleet capacity exceeds threshold
        #[arg(long, env = "APOLLO_SCALE_WEBHOOK")]
        webhook_url: Option<String>,
        /// HMAC secret for signing scale webhook payloads
        #[arg(long, env = "APOLLO_SCALE_WEBHOOK_SECRET")]
        webhook_secret: Option<String>,
        /// Fraction of fleet capacity that triggers scale webhook (0.0–1.0)
        #[arg(long, default_value = "0.80")]
        scale_threshold: f64,
    },
    /// Register a node with the hub
    Add {
        #[arg(long)] ip:   String,
        #[arg(long)] key:  String,
        #[arg(long, default_value = "edge-node")] name: String,
        #[arg(long, default_value = "default")]   region: String,
        #[arg(short, long, default_value = ".apollo/hub_nodes.json")] storage: PathBuf,
    },
    /// List all registered nodes
    List {
        #[arg(short, long, default_value = ".apollo/hub_nodes.json")] storage: PathBuf,
    },
}

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
struct NodeRecord {
    name:   String,
    ip:     String,
    key:    String,
    #[serde(default = "default_region")]
    region: String,
    #[serde(default)]
    status: NodeStatus,
}

fn default_region() -> String { "default".to_string() }

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

#[derive(Serialize, Deserialize, Clone, Debug)]
struct CatalogEntry {
    agent_id:     String,
    version:      String,
    runtime:      String,
    capabilities: Vec<String>,
    checksum:     String,
    available_on: Vec<String>,
}

// ── Shared hub state ──────────────────────────────────────────────────────────

#[derive(Clone)]
struct HubState {
    nodes:   Arc<Mutex<Vec<NodeRecord>>>,
    catalog: Arc<Mutex<Vec<CatalogEntry>>>,
    scale_fired: Arc<Mutex<bool>>,
}

// ── Query params ──────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
struct RegionQuery {
    region: Option<String>,
}

// ── Main ──────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Start { listen, storage, webhook_url, webhook_secret, scale_threshold } => {
            let nodes = load_nodes(&storage).unwrap_or_default();
            let state = HubState {
                nodes:       Arc::new(Mutex::new(nodes)),
                catalog:     Arc::new(Mutex::new(Vec::new())),
                scale_fired: Arc::new(Mutex::new(false)),
            };

            // Background poller
            let poller      = state.clone();
            let storage_c   = storage.clone();
            let wh_url      = webhook_url.clone();
            let wh_secret   = webhook_secret.clone();
            tokio::spawn(async move {
                run_poller(poller, storage_c, wh_url, wh_secret, scale_threshold).await;
            });

            // Axum API server
            let app = Router::new()
                .route("/",             get(handle_root))
                .route("/status",       get(handle_nodes_status))
                .route("/nodes/status", get(handle_nodes_status))
                .route("/nodes/best",   get(handle_nodes_best))
                .route("/catalog",      get(handle_catalog))
                .route("/summary",      get(handle_summary))
                .route("/regions",      get(handle_regions))
                .with_state(state);

            let addr: std::net::SocketAddr = listen.parse()
                .map_err(|e| anyhow!("Invalid listen address: {}", e))?;

            println!("APOLLO Hub listening on http://{}", addr);
            axum_server::bind(addr)
                .serve(app.into_make_service())
                .await
                .map_err(|e| anyhow!("Hub server error: {}", e))
        }

        Commands::Add { ip, key, name, region, storage } => {
            let mut nodes = load_nodes(&storage).unwrap_or_default();
            if nodes.iter().any(|n| n.ip == ip) {
                println!("Node {} already registered.", ip);
            } else {
                nodes.push(NodeRecord { name: name.clone(), ip: ip.clone(), key, region, status: Default::default() });
                save_nodes(&storage, &nodes)?;
                println!("✓ Registered node '{}' at {}", name, ip);
            }
            Ok(())
        }

        Commands::List { storage } => {
            let nodes = load_nodes(&storage).unwrap_or_default();
            if nodes.is_empty() {
                println!("No nodes registered.");
            } else {
                println!("{:<15} {:<22} {:<12} {:<10} {:<6} {}", "NAME", "IP", "REGION", "STATUS", "FAIL", "AGENTS");
                for n in nodes {
                    let status = if n.status.is_online { "ONLINE" } else if n.status.failure_count > 0 { "FAILED" } else { "UNKNOWN" };
                    println!("{:<15} {:<22} {:<12} {:<10} {:<6} {}/{}",
                        n.name, n.ip, n.region, status, n.status.failure_count,
                        n.status.active_agents, n.status.max_agents);
                }
            }
            Ok(())
        }
    }
}

// ── Background poller ─────────────────────────────────────────────────────────

async fn run_poller(
    state: HubState,
    storage: PathBuf,
    webhook_url: Option<String>,
    webhook_secret: Option<String>,
    scale_threshold: f64,
) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let mut tick = 0u64;

    loop {
        tick += 1;
        let nodes_snap = { state.nodes.lock().unwrap().clone() };

        for node in nodes_snap {
            if node.status.failure_count >= 5 && tick % 6 != 0 { continue; }

            let client_c   = client.clone();
            let state_c    = state.clone();
            let storage_c  = storage.clone();

            tokio::spawn(async move {
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

                // Catalog poll every 5th tick, with 50ms gap to avoid rate limiter
                if tick % 5 == 0 && status.is_online {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    let list_url = format!("http://{}/agents/list", node.ip);
                    match client_c.get(&list_url).header("X-Apollo-Key", &node.key).send().await {
                        Err(_) => {}
                        Ok(r) if r.status().is_success() => {
                            let text = r.text().await.unwrap_or_default();
                            if let Ok(records) = serde_json::from_str::<Vec<serde_json::Value>>(&text) {
                                let mut catalog = state_c.catalog.lock().unwrap();
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
                                        catalog.push(CatalogEntry { agent_id: id, version, runtime, capabilities, checksum, available_on: vec![node.ip.clone()] });
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                let mut nodes = state_c.nodes.lock().unwrap();
                if let Some(pos) = nodes.iter().position(|n| n.ip == node.ip) {
                    nodes[pos].status = status;
                }
                let snap: Vec<NodeRecord> = nodes.clone();
                let _ = save_nodes(&storage_c, &snap);
            });
        }

        // Auto-scale check
        if let Some(ref url) = webhook_url {
            let nodes = state.nodes.lock().unwrap();
            let total_cap: usize = nodes.iter().map(|n| n.status.max_agents).sum();
            let total_act: usize = nodes.iter().map(|n| n.status.active_agents).sum();
            drop(nodes);

            if total_cap > 0 {
                let utilization = total_act as f64 / total_cap as f64;
                let mut fired = state.scale_fired.lock().unwrap();
                if utilization >= scale_threshold && !*fired {
                    *fired = true;
                    drop(fired);
                    fire_scale_webhook(url, webhook_secret.as_deref(), total_act, total_cap);
                } else if utilization < scale_threshold * 0.7 {
                    // Reset so it can fire again after scale-down
                    *fired = false;
                }
            }
        }

        sleep(Duration::from_secs(10)).await;
    }
}

fn fire_scale_webhook(url: &str, secret: Option<&str>, active: usize, max: usize) {
    use apollo_core::webhook::{WebhookConfig, WebhookPayload, fire};
    let cfg = WebhookConfig::new(url.to_string(), secret.map(|s| s.to_string()));
    let payload = WebhookPayload::scale_needed(active, max, "fleet");
    fire(&cfg, payload);
}

// ── Axum route handlers ───────────────────────────────────────────────────────

async fn handle_root() -> impl IntoResponse {
    Json(serde_json::json!({
        "service": "APOLLO Hub",
        "endpoints": ["/summary", "/nodes/status", "/nodes/best", "/catalog", "/regions"]
    }))
}

async fn handle_nodes_status(State(state): State<HubState>) -> impl IntoResponse {
    let nodes = state.nodes.lock().unwrap().clone();
    Json(nodes)
}

async fn handle_nodes_best(
    State(state): State<HubState>,
    Query(q): Query<RegionQuery>,
) -> impl IntoResponse {
    let nodes = state.nodes.lock().unwrap();
    let candidates = nodes.iter()
        .filter(|n| n.status.is_online && n.status.active_agents < n.status.max_agents)
        .filter(|n| q.region.as_deref().map_or(true, |r| n.region == r));

    match candidates.min_by_key(|n| n.status.active_agents) {
        Some(n) => Json(serde_json::json!({
            "node":          n.name,
            "ip":            n.ip,
            "region":        n.region,
            "active_agents": n.status.active_agents,
            "max_agents":    n.status.max_agents,
        })).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "no available nodes"}))).into_response(),
    }
}

async fn handle_catalog(State(state): State<HubState>) -> impl IntoResponse {
    let catalog = state.catalog.lock().unwrap().clone();
    Json(catalog)
}

async fn handle_summary(State(state): State<HubState>) -> impl IntoResponse {
    let nodes = state.nodes.lock().unwrap();
    let nodes_total:    usize = nodes.len();
    let nodes_online:   usize = nodes.iter().filter(|n| n.status.is_online).count();
    let agents_active:  usize = nodes.iter().map(|n| n.status.active_agents).sum();
    let fleet_capacity: usize = nodes.iter().map(|n| n.status.max_agents).sum();
    drop(nodes);
    let catalog_agents = state.catalog.lock().unwrap().len();
    Json(serde_json::json!({
        "nodes_total":    nodes_total,
        "nodes_online":   nodes_online,
        "agents_active":  agents_active,
        "fleet_capacity": fleet_capacity,
        "catalog_agents": catalog_agents,
    }))
}

async fn handle_regions(State(state): State<HubState>) -> impl IntoResponse {
    let nodes = state.nodes.lock().unwrap();
    let mut regions: std::collections::HashMap<String, serde_json::Value> =
        std::collections::HashMap::new();
    for n in nodes.iter() {
        let entry = regions.entry(n.region.clone()).or_insert_with(|| {
            serde_json::json!({"nodes_total": 0, "nodes_online": 0, "agents_active": 0, "fleet_capacity": 0})
        });
        *entry.get_mut("nodes_total").unwrap() = serde_json::json!(
            entry["nodes_total"].as_i64().unwrap_or(0) + 1
        );
        if n.status.is_online {
            *entry.get_mut("nodes_online").unwrap() = serde_json::json!(
                entry["nodes_online"].as_i64().unwrap_or(0) + 1
            );
        }
        *entry.get_mut("agents_active").unwrap() = serde_json::json!(
            entry["agents_active"].as_i64().unwrap_or(0) + n.status.active_agents as i64
        );
        *entry.get_mut("fleet_capacity").unwrap() = serde_json::json!(
            entry["fleet_capacity"].as_i64().unwrap_or(0) + n.status.max_agents as i64
        );
    }
    Json(regions)
}

// ── Storage helpers ───────────────────────────────────────────────────────────

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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
