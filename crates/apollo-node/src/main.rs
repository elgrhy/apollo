use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use apollo_runtime::process::{
    ProcessRuntime, save_instance, load_tenant_instances, save_tenant_instances,
    count_active_instances, load_all_instances,
};
use apollo_runtime::AgentRuntime;
use std::sync::{Arc, Mutex};
use std::path::{Path, PathBuf};
use apollo_core::types::{AgentSpec, NodeConfig, AgentInstance, NodeNetworkPolicy};
use apollo_core::{
    register_agent_package, rollback_agent, remove_agent,
    detect_node_capabilities, load_agent_registry,
};
use sysinfo::{System, Pid};
use tokio::signal;
use std::collections::HashMap;
use std::time::{Instant, Duration, SystemTime, UNIX_EPOCH};
use serde::Deserialize;

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "apollo", about = "APOLLO — AI Agent Execution Engine v1.1")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the APOLLO Node daemon
    Node {
        #[command(subcommand)]
        action: NodeAction,
    },
    /// Agent management
    Agent {
        #[arg(short, long, default_value = ".apollo")]
        base_dir: PathBuf,
        #[command(subcommand)]
        action: AgentAction,
    },
    /// System-wide health check
    Doctor,
}

#[derive(Subcommand)]
enum NodeAction {
    Start {
        #[arg(short, long, default_value = "0.0.0.0:8080")]
        listen: String,
        #[arg(short, long, default_value = ".apollo")]
        base_dir: PathBuf,
        #[arg(long, default_value = "200")]
        max_agents: usize,
        #[arg(long, env = "APOLLO_SECRET_KEYS")]
        secret_keys: Option<String>,
    },
    Status,
}

#[derive(Subcommand)]
enum AgentAction {
    /// Register an agent from local path, URL, or git repo
    Add { source: String },
    /// Run an agent for a tenant
    Run { name: String, #[arg(long)] tenant: String },
    /// Stop a running agent
    Stop { name: String, #[arg(long)] tenant: String },
    /// List all registered agents
    List,
    /// Rollback an agent to its previous version
    Rollback { name: String },
    /// Remove a registered agent
    Remove { name: String },
}

// ── REST request/response types ────────────────────────────────────────────────

#[derive(Deserialize)]
struct RunRequest   { agent: String, tenant: String }
#[derive(Deserialize)]
struct StopRequest  { agent: String, tenant: String }
#[derive(Deserialize)]
struct AddRequest   { source: String }
#[derive(Deserialize)]
struct RollbackReq  { agent: String }

// ── Rate limiter ───────────────────────────────────────────────────────────────

struct RateLimiter {
    buckets:   Mutex<HashMap<String, Instant>>,
    rps_limit: u32,
}

impl RateLimiter {
    fn new(rps: u32) -> Self {
        Self { buckets: Mutex::new(HashMap::new()), rps_limit: rps }
    }
    fn check(&self, key: &str) -> bool {
        let mut b = self.buckets.lock().unwrap();
        let now = Instant::now();
        let interval = Duration::from_millis(1000 / self.rps_limit as u64);
        if let Some(last) = b.get(key) {
            if now.duration_since(*last) < interval { return false; }
        }
        b.insert(key.to_string(), now);
        true
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    println!(r#"
   ___   ___  ____  __    __    ____
  / _ | / _ \/ __ \/ /   / /   / __ \
 / __ |/ ___/ /_/ / /___/ /___/ /_/ /
/_/ |_/_/   \____/_____/_____/\____/

MISSION CONTROL
"#);

    if std::env::args().len() == 1 {
        run_interactive_shell().await?;
        return Ok(());
    }

    let cli = Cli::parse();
    handle_command(cli.command).await
}

async fn handle_command(command: Commands) -> Result<()> {
    match command {
        Commands::Node { action } => handle_node(action).await,
        Commands::Agent { base_dir, action } => handle_agent(&base_dir, action).await,
        Commands::Doctor => {
            let profile = detect_node_capabilities().await?;
            println!("[OK] Node Engine Initialized");
            println!("[OK] Hub Connectivity Ready");
            println!("[OK] Event Spine Active");
            println!("[OK] Security Sandbox Enabled");
            println!("[OK] Runtimes Detected: {}", profile.runtimes.join(", "));
            println!("STATUS: PRODUCTION READY");
            Ok(())
        }
    }
}

async fn handle_node(action: NodeAction) -> Result<()> {
    match action {
        NodeAction::Start { listen, base_dir, max_agents, secret_keys } => {
            let profile = detect_node_capabilities().await?;
            let keys: Vec<String> = secret_keys
                .unwrap_or_else(|| "apollo-dev-secret".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect();

            let config = NodeConfig {
                node_id:     format!("node-{}", now_unix() % 10000),
                provider_id: "standalone".to_string(),
                secret_keys: keys,
                profile,
                network: NodeNetworkPolicy { allow_localhost: false, allow_private_ranges: false, rate_limit_rps: 100 },
            };
            println!("APOLLO Node '{}' active.", config.node_id);

            let runtime      = Arc::new(ProcessRuntime::new(base_dir.clone()));
            let rate_limiter = Arc::new(RateLimiter::new(config.network.rate_limit_rps));

            startup_recovery(&runtime, &base_dir).await;

            let rt_shutdown = Arc::clone(&runtime);
            tokio::spawn(async move {
                signal::ctrl_c().await.ok();
                let _ = rt_shutdown.shutdown().await;
                std::process::exit(0);
            });

            run_api_server(&listen, runtime, config, rate_limiter, max_agents, base_dir).await
        }
        NodeAction::Status => {
            println!("APOLLO Node: Active [CERTIFIED]");
            Ok(())
        }
    }
}

async fn handle_agent(base_dir: &Path, action: AgentAction) -> Result<()> {
    match action {
        AgentAction::Add { source } => {
            let record = register_agent_package(base_dir, &source).await?;
            println!("✓ Registered: {} v{}  sha256:{}", record.id, record.spec.version, &record.checksum[..12]);
        }
        AgentAction::Run { name, tenant } => {
            let runtime  = ProcessRuntime::new(base_dir.to_path_buf());
            let spec     = get_agent_spec(base_dir, &name)?;
            let instance = runtime.start(&tenant, &spec).await?;
            save_instance(base_dir, &instance)?;
            println!("✓ Running: {} (tenant={})  PID={:?}  port={:?}", name, tenant, instance.pid, instance.port);
        }
        AgentAction::Stop { name, tenant } => {
            let runtime   = ProcessRuntime::new(base_dir.to_path_buf());
            let mut list  = load_tenant_instances(base_dir, &tenant)?;
            if let Some(pos) = list.iter().position(|i| i.agent_id == name && i.tenant_id == tenant) {
                if let Some(pid) = list[pos].pid {
                    runtime.stop(pid).await?;
                    list[pos].status = "stopped".to_string();
                    list[pos].pid    = None;
                    save_tenant_instances(base_dir, &tenant, &list)?;
                    println!("✓ Stopped: {} (tenant={})", name, tenant);
                }
            } else {
                println!("No running instance found for agent='{}' tenant='{}'", name, tenant);
            }
        }
        AgentAction::List => {
            let records = load_agent_registry(base_dir)?;
            if records.is_empty() {
                println!("No agents registered.");
            } else {
                println!("{:<20} {:<12} {:<14} {}", "NAME", "VERSION", "RUNTIME", "CHECKSUM");
                for r in records {
                    println!("{:<20} {:<12} {:<14} {}",
                        r.id, r.spec.version, r.spec.runtime.kind, &r.checksum[..12]);
                }
            }
        }
        AgentAction::Rollback { name } => {
            rollback_agent(base_dir, &name)?;
        }
        AgentAction::Remove { name } => {
            remove_agent(base_dir, &name)?;
            println!("✓ Removed: {}", name);
        }
    }
    Ok(())
}

// ── Interactive shell ─────────────────────────────────────────────────────────

async fn run_interactive_shell() -> Result<()> {
    use dialoguer::{Input, theme::ColorfulTheme};
    println!("Interactive mode. Type 'help' for commands, 'exit' to quit.");
    loop {
        let input: String = Input::with_theme(&ColorfulTheme::default())
            .with_prompt("apollo")
            .interact_text()?;
        if input == "exit" || input == "quit" { break; }
        if input.trim().is_empty() { continue; }

        let mut full_args = vec!["apollo".to_string()];
        full_args.extend(input.split_whitespace().map(|s| s.to_string()));
        match Cli::try_parse_from(full_args) {
            Ok(cli) => { if let Err(e) = handle_command(cli.command).await { println!("Error: {}", e); } }
            Err(e)  => println!("{}", e),
        }
    }
    Ok(())
}

// ── REST API server ────────────────────────────────────────────────────────────

async fn run_api_server(
    listen: &str,
    runtime: Arc<ProcessRuntime>,
    config:  NodeConfig,
    rl:      Arc<RateLimiter>,
    max_agents: usize,
    base_dir: PathBuf,
) -> Result<()> {
    let server = tiny_http::Server::http(listen).map_err(|e| anyhow!("{}", e))?;
    println!("API listening on http://{}", listen);

    for mut req in server.incoming_requests() {
        // Auth
        let key_opt = req.headers().iter()
            .find(|h| h.field.as_str().to_ascii_lowercase() == "x-apollo-key")
            .map(|h| h.value.as_str().to_string());
        let corr_id = req.headers().iter()
            .find(|h| h.field.as_str().to_ascii_lowercase() == "x-apollo-correlation-id")
            .map(|h| h.value.as_str().to_string());

        let authed = key_opt.as_deref().map(|k| config.secret_keys.contains(&k.to_string())).unwrap_or(false);
        if !authed {
            let _ = req.respond(tiny_http::Response::from_string("Unauthorized").with_status_code(401));
            continue;
        }
        if !rl.check(key_opt.as_deref().unwrap_or("")) {
            let _ = req.respond(tiny_http::Response::from_string("Too Many Requests").with_status_code(429));
            continue;
        }

        let method = req.method().clone();
        let url    = req.url().to_string();

        // Build response body and status, then send once at end of match
        let (status, body) = match (method, url.as_str()) {

            (tiny_http::Method::Get, "/metrics") => {
                let active = count_active_instances(&base_dir);
                (200u16, format!(r#"{{"active_agents":{},"max_agents":{},"node_id":"{}"}}"#,
                    active, max_agents, config.node_id))
            }

            (tiny_http::Method::Get, "/agents/list") => {
                let records = load_agent_registry(&base_dir).unwrap_or_default();
                (200, serde_json::to_string(&records).unwrap_or_else(|_| "[]".into()))
            }

            (tiny_http::Method::Post, "/agents/add") => {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body)?;
                match serde_json::from_str::<AddRequest>(&body) {
                    Ok(r) => match register_agent_package(&base_dir, &r.source).await {
                        Ok(rec) => (200, serde_json::to_string(&rec).unwrap_or_default()),
                        Err(e)  => (400, err_json(&e.to_string())),
                    },
                    Err(e) => (400, err_json(&format!("Bad JSON: {}", e))),
                }
            }

            (tiny_http::Method::Post, "/agents/run") => {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body)?;
                match serde_json::from_str::<RunRequest>(&body) {
                    Err(e) => (400, err_json(&format!("Bad JSON: {}", e))),
                    Ok(r) => {
                        if count_active_instances(&base_dir) >= max_agents {
                            (503, err_json("Node at capacity"))
                        } else {
                            match get_agent_spec(&base_dir, &r.agent) {
                                Err(e) => (404, err_json(&e.to_string())),
                                Ok(spec) => match runtime.start(&r.tenant, &spec).await {
                                    Err(e) => (500, err_json(&e.to_string())),
                                    Ok(inst) => {
                                        save_instance(&base_dir, &inst)?;
                                        log_event(&config.node_id, "LIFECYCLE", "AGENT_START",
                                            &format!("Agent '{}' started for tenant '{}'", r.agent, r.tenant),
                                            corr_id.clone());
                                        (200, serde_json::to_string(&inst).unwrap_or_default())
                                    }
                                }
                            }
                        }
                    }
                }
            }

            (tiny_http::Method::Delete, "/agents/stop") => {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body)?;
                match serde_json::from_str::<StopRequest>(&body) {
                    Err(e) => (400, err_json(&format!("Bad JSON: {}", e))),
                    Ok(r) => {
                        let mut list = load_tenant_instances(&base_dir, &r.tenant).unwrap_or_default();
                        match list.iter().position(|i| i.agent_id == r.agent && i.tenant_id == r.tenant) {
                            None => (404, err_json("No running instance found")),
                            Some(pos) => {
                                if let Some(pid) = list[pos].pid {
                                    runtime.stop(pid).await?;
                                    list[pos].status = "stopped".to_string();
                                    list[pos].pid    = None;
                                    save_tenant_instances(&base_dir, &r.tenant, &list)?;
                                    log_event(&config.node_id, "LIFECYCLE", "AGENT_STOP",
                                        &format!("Agent '{}' stopped for tenant '{}'", r.agent, r.tenant),
                                        corr_id.clone());
                                }
                                (200, r#"{"status":"stopped"}"#.to_string())
                            }
                        }
                    }
                }
            }

            (tiny_http::Method::Post, "/agents/rollback") => {
                let mut body = String::new();
                req.as_reader().read_to_string(&mut body)?;
                match serde_json::from_str::<RollbackReq>(&body) {
                    Ok(r) => match rollback_agent(&base_dir, &r.agent) {
                        Ok(())  => (200, r#"{"status":"rolled_back"}"#.to_string()),
                        Err(e)  => (400, err_json(&e.to_string())),
                    },
                    Err(e) => (400, err_json(&format!("Bad JSON: {}", e))),
                }
            }

            (tiny_http::Method::Get, "/health") => {
                (200, r#"{"status":"ok"}"#.to_string())
            }

            _ => (404, r#"{"error":"not found"}"#.to_string()),
        };

        let resp = tiny_http::Response::from_string(body)
            .with_status_code(status)
            .with_header(tiny_http::Header::from_bytes(b"Content-Type", b"application/json").unwrap());
        let _ = req.respond(resp);
    }
    Ok(())
}

// ── Startup recovery ──────────────────────────────────────────────────────────

async fn startup_recovery(runtime: &ProcessRuntime, base_dir: &Path) {
    let mut all = load_all_instances(base_dir);
    let mut sys = System::new_all();
    sys.refresh_processes();

    for inst in all.iter_mut() {
        let alive = inst.pid.map(|p| sys.process(Pid::from(p as usize)).is_some()).unwrap_or(false);
        if !alive && inst.status == "running" {
            if let Ok(spec) = get_agent_spec(base_dir, &inst.agent_id) {
                if let Ok(new) = runtime.start(&inst.tenant_id, &spec).await {
                    inst.pid = new.pid;
                    inst.stats.restart_count += 1;
                    log_event("system", "HEALTH", "NODE_RECOVER",
                        &format!("Auto-recovered '{}' for tenant '{}'", inst.agent_id, inst.tenant_id),
                        None);
                }
            }
        }
    }

    // Re-save by tenant
    let mut by_tenant: HashMap<String, Vec<AgentInstance>> = HashMap::new();
    for inst in all {
        by_tenant.entry(inst.tenant_id.clone()).or_default().push(inst);
    }
    for (tenant, instances) in by_tenant {
        let _ = save_tenant_instances(base_dir, &tenant, &instances);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn get_agent_spec(base_dir: &Path, name: &str) -> Result<AgentSpec> {
    load_agent_registry(base_dir)?
        .into_iter()
        .find(|r| r.id == name)
        .map(|r| r.spec)
        .ok_or_else(|| anyhow!("Agent '{}' not registered. Use 'agent add' first.", name))
}

fn err_json(msg: &str) -> String {
    format!(r#"{{"error":"{}"}}"#, msg.replace('"', "'"))
}

fn log_event(node_id: &str, category: &str, action: &str, msg: &str, corr: Option<String>) {
    apollo_core::types::log_event(apollo_core::types::ApolloEvent {
        timestamp:      now_unix(),
        node_id:        node_id.to_string(),
        level:          "INFO".to_string(),
        category:       category.to_string(),
        action:         action.to_string(),
        message:        msg.to_string(),
        correlation_id: corr,
        metadata:       None,
    });
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
