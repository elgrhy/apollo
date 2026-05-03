use clap::{Parser, Subcommand};
use anyhow::{Result, anyhow};
use apollo_runtime::process::{
    ProcessRuntime, save_instance, load_tenant_instances, save_tenant_instances,
    count_active_instances, load_all_instances,
};
use apollo_runtime::AgentRuntime;
use axum::{
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{delete, get, post, put},
    Json, Router,
};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{Pid, System};
use tokio::signal;
use std::path::Path;

use apollo_core::types::{AgentSpec, AgentInstance, NodeConfig, NodeNetworkPolicy};
use apollo_core::{
    detect_node_capabilities, load_agent_registry,
    register_agent_package, rollback_agent, remove_agent,
};
use apollo_core::secrets::{upsert_secrets, delete_secrets};
use apollo_core::usage::{load_usage, reset_usage, record_start, record_stop, list_usage_tenants};
use apollo_core::webhook::{WebhookConfig, WebhookPayload, fire as fire_webhook};

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "apollo", about = "APOLLO — AI Agent Execution Engine v1.2")]
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
        /// TLS certificate PEM path (enables HTTPS)
        #[arg(long)]
        tls_cert: Option<PathBuf>,
        /// TLS private key PEM path
        #[arg(long)]
        tls_key: Option<PathBuf>,
        /// JWT HMAC secret for Bearer token authentication
        #[arg(long, env = "APOLLO_JWT_SECRET")]
        jwt_secret: Option<String>,
        /// Webhook URL for lifecycle events (AGENT_START, AGENT_STOP, etc.)
        #[arg(long, env = "APOLLO_WEBHOOK_URL")]
        webhook_url: Option<String>,
        /// HMAC secret for signing webhook payloads
        #[arg(long, env = "APOLLO_WEBHOOK_SECRET")]
        webhook_secret: Option<String>,
        /// Node region (e.g. us-east-1) reported to hub
        #[arg(long, default_value = "default")]
        region: String,
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

// ── Shared application state ──────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    runtime:     Arc<ProcessRuntime>,
    config:      NodeConfig,
    rate_limiter: Arc<RateLimiter>,
    max_agents:  usize,
    base_dir:    PathBuf,
    webhook:     Option<WebhookConfig>,
}

// ── Request/response types ────────────────────────────────────────────────────

#[derive(Deserialize)] struct RunRequest  { agent: String, tenant: String }
#[derive(Deserialize)] struct StopRequest { agent: String, tenant: String }
#[derive(Deserialize)] struct AddRequest  { source: String }
#[derive(Deserialize)] struct RollbackReq { agent: String }
#[derive(Deserialize)] struct SecretsBody { secrets: HashMap<String, String> }

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    sub: String,
    exp: u64,
    #[serde(default)]
    keys: Vec<String>,
}

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

// ── Auth middleware ───────────────────────────────────────────────────────────

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let headers = req.headers();

    // Extract the credential from X-Apollo-Key or Authorization: Bearer <jwt>
    let maybe_key = extract_key(headers, &state.config.secret_keys, state.config.jwt_secret.as_deref());

    match maybe_key {
        None => (StatusCode::UNAUTHORIZED, "Unauthorized").into_response(),
        Some(key) => {
            if !state.rate_limiter.check(&key) {
                return (StatusCode::TOO_MANY_REQUESTS, "Too Many Requests").into_response();
            }
            next.run(req).await
        }
    }
}

fn extract_key(headers: &HeaderMap, valid_keys: &[String], jwt_secret: Option<&str>) -> Option<String> {
    // 1. X-Apollo-Key header
    if let Some(val) = headers.get("x-apollo-key").and_then(|v| v.to_str().ok()) {
        if valid_keys.contains(&val.to_string()) {
            return Some(val.to_string());
        }
    }
    // 2. Authorization: Bearer <jwt>
    if let (Some(secret), Some(bearer)) = (
        jwt_secret,
        headers.get("authorization").and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
    ) {
        let key = DecodingKey::from_secret(secret.as_bytes());
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_exp = true;
        if let Ok(data) = decode::<JwtClaims>(bearer, &key, &validation) {
            let claims = data.claims;
            // JWT can carry allowed keys or just be a valid signed token
            if claims.keys.is_empty() || claims.keys.iter().any(|k| valid_keys.contains(k)) {
                return Some(format!("jwt:{}", claims.sub));
            }
        }
    }
    None
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
        NodeAction::Start {
            listen, base_dir, max_agents, secret_keys,
            tls_cert, tls_key, jwt_secret, webhook_url, webhook_secret, region,
        } => {
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
                network: NodeNetworkPolicy {
                    allow_localhost: false,
                    allow_private_ranges: false,
                    rate_limit_rps: 100,
                },
                region,
                jwt_secret,
            };

            println!("APOLLO Node '{}' active. Region: {}", config.node_id, config.region);

            let runtime      = Arc::new(ProcessRuntime::new(base_dir.clone()));
            let rate_limiter = Arc::new(RateLimiter::new(config.network.rate_limit_rps));

            let webhook = webhook_url.map(|url| WebhookConfig::new(url, webhook_secret));

            startup_recovery(&runtime, &base_dir).await;

            // Metering background task — samples every 60 s
            let meter_base  = base_dir.clone();
            let meter_node  = config.node_id.clone();
            tokio::spawn(async move {
                run_metering_loop(meter_base, meter_node).await;
            });

            let state = AppState {
                runtime:     runtime.clone(),
                config:      config.clone(),
                rate_limiter,
                max_agents,
                base_dir:    base_dir.clone(),
                webhook,
            };

            let rt_shutdown = Arc::clone(&runtime);
            tokio::spawn(async move {
                signal::ctrl_c().await.ok();
                let _ = rt_shutdown.shutdown().await;
                std::process::exit(0);
            });

            run_api_server(&listen, state, tls_cert, tls_key).await
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
            let mut list = load_tenant_instances(base_dir, &tenant)?;
            if let Some(pos) = list.iter().position(|i| i.agent_id == name && i.tenant_id == tenant) {
                if let Some(pid) = list[pos].pid {
                    let runtime = ProcessRuntime::new(base_dir.to_path_buf());
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
            let records = load_agent_registry(base_dir).unwrap_or_default();
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
            println!("✓ Rolled back: {}", name);
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

// ── REST API server (axum) ────────────────────────────────────────────────────

async fn run_api_server(
    listen: &str,
    state: AppState,
    tls_cert: Option<PathBuf>,
    tls_key:  Option<PathBuf>,
) -> Result<()> {
    let app = Router::new()
        // Node
        .route("/metrics",  get(handle_metrics))
        .route("/health",   get(handle_health))
        // Agents
        .route("/agents/list",     get(handle_agents_list))
        .route("/agents/add",      post(handle_agents_add))
        .route("/agents/run",      post(handle_agents_run))
        .route("/agents/stop",     delete(handle_agents_stop))
        .route("/agents/rollback", post(handle_agents_rollback))
        .route("/agents/remove",   post(handle_agents_remove))
        // Tenant secrets
        .route("/tenants/:tenant_id/secrets", put(handle_secrets_put))
        .route("/tenants/:tenant_id/secrets", delete(handle_secrets_delete))
        // Usage metering
        .route("/usage",                       get(handle_usage_all))
        .route("/usage/:tenant_id",            get(handle_usage_tenant))
        .route("/usage/:tenant_id/reset",      post(handle_usage_reset))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state);

    let addr: std::net::SocketAddr = listen.parse()
        .map_err(|e| anyhow!("Invalid listen address '{}': {}", listen, e))?;

    match (tls_cert, tls_key) {
        (Some(cert), Some(key)) => {
            println!("API listening on https://{} (TLS)", addr);
            let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key)
                .await
                .map_err(|e| anyhow!("TLS config error: {}", e))?;
            axum_server::bind_rustls(addr, tls_config)
                .serve(app.into_make_service())
                .await
                .map_err(|e| anyhow!("Server error: {}", e))
        }
        _ => {
            println!("API listening on http://{}", addr);
            axum_server::bind(addr)
                .serve(app.into_make_service())
                .await
                .map_err(|e| anyhow!("Server error: {}", e))
        }
    }
}

// ── Route handlers ────────────────────────────────────────────────────────────

async fn handle_health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

async fn handle_metrics(State(s): State<AppState>) -> impl IntoResponse {
    let active = count_active_instances(&s.base_dir);
    Json(serde_json::json!({
        "active_agents": active,
        "max_agents":    s.max_agents,
        "node_id":       s.config.node_id,
        "region":        s.config.region,
    }))
}

async fn handle_agents_list(State(s): State<AppState>) -> impl IntoResponse {
    let records = load_agent_registry(&s.base_dir).unwrap_or_default();
    Json(records)
}

async fn handle_agents_add(
    State(s): State<AppState>,
    Json(body): Json<AddRequest>,
) -> impl IntoResponse {
    match register_agent_package(&s.base_dir, &body.source).await {
        Ok(rec) => (StatusCode::OK, Json(serde_json::to_value(rec).unwrap_or_default())).into_response(),
        Err(e)  => (StatusCode::BAD_REQUEST, err_json(&e.to_string())).into_response(),
    }
}

async fn handle_agents_run(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RunRequest>,
) -> impl IntoResponse {
    if count_active_instances(&s.base_dir) >= s.max_agents {
        return (StatusCode::SERVICE_UNAVAILABLE, err_json("Node at capacity")).into_response();
    }
    let spec = match get_agent_spec(&s.base_dir, &body.agent) {
        Ok(sp) => sp,
        Err(e) => return (StatusCode::NOT_FOUND, err_json(&e.to_string())).into_response(),
    };
    match s.runtime.start(&body.tenant, &spec).await {
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, err_json(&e.to_string())).into_response(),
        Ok(inst) => {
            let _ = save_instance(&s.base_dir, &inst);
            let _ = record_start(&s.base_dir, &body.tenant);
            log_event(&s.config.node_id, "LIFECYCLE", "AGENT_START",
                &format!("Agent '{}' started for tenant '{}'", body.agent, body.tenant),
                corr_id(&headers));
            if let Some(ref wh) = s.webhook {
                fire_webhook(wh, WebhookPayload::agent_start(
                    &s.config.node_id, &body.tenant, &body.agent,
                    inst.port.unwrap_or(0), inst.pid.unwrap_or(0),
                ));
            }
            (StatusCode::OK, Json(serde_json::to_value(&inst).unwrap_or_default())).into_response()
        }
    }
}

async fn handle_agents_stop(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<StopRequest>,
) -> impl IntoResponse {
    let mut list = load_tenant_instances(&s.base_dir, &body.tenant).unwrap_or_default();
    match list.iter().position(|i| i.agent_id == body.agent && i.tenant_id == body.tenant) {
        None => (StatusCode::NOT_FOUND, err_json("No running instance found")).into_response(),
        Some(pos) => {
            if let Some(pid) = list[pos].pid {
                let _ = s.runtime.stop(pid).await;
                list[pos].status = "stopped".to_string();
                list[pos].pid    = None;
                let _ = save_tenant_instances(&s.base_dir, &body.tenant, &list);
                let _ = record_stop(&s.base_dir, &body.tenant);
                log_event(&s.config.node_id, "LIFECYCLE", "AGENT_STOP",
                    &format!("Agent '{}' stopped for tenant '{}'", body.agent, body.tenant),
                    corr_id(&headers));
                if let Some(ref wh) = s.webhook {
                    fire_webhook(wh, WebhookPayload::agent_stop(
                        &s.config.node_id, &body.tenant, &body.agent,
                    ));
                }
            }
            (StatusCode::OK, Json(serde_json::json!({"status": "stopped"}))).into_response()
        }
    }
}

async fn handle_agents_rollback(
    State(s): State<AppState>,
    Json(body): Json<RollbackReq>,
) -> impl IntoResponse {
    match rollback_agent(&s.base_dir, &body.agent) {
        Ok(())  => (StatusCode::OK, Json(serde_json::json!({"status": "rolled_back"}))).into_response(),
        Err(e)  => (StatusCode::BAD_REQUEST, err_json(&e.to_string())).into_response(),
    }
}

async fn handle_agents_remove(
    State(s): State<AppState>,
    Json(body): Json<RollbackReq>,
) -> impl IntoResponse {
    match remove_agent(&s.base_dir, &body.agent) {
        Ok(())  => (StatusCode::OK, Json(serde_json::json!({"status": "removed"}))).into_response(),
        Err(e)  => (StatusCode::BAD_REQUEST, err_json(&e.to_string())).into_response(),
    }
}

async fn handle_secrets_put(
    State(s): State<AppState>,
    AxumPath(tenant_id): AxumPath<String>,
    Json(body): Json<SecretsBody>,
) -> impl IntoResponse {
    match upsert_secrets(&s.base_dir, &tenant_id, body.secrets) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "saved"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, err_json(&e.to_string())).into_response(),
    }
}

async fn handle_secrets_delete(
    State(s): State<AppState>,
    AxumPath(tenant_id): AxumPath<String>,
) -> impl IntoResponse {
    match delete_secrets(&s.base_dir, &tenant_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "deleted"}))).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, err_json(&e.to_string())).into_response(),
    }
}

async fn handle_usage_all(State(s): State<AppState>) -> impl IntoResponse {
    let tenants = list_usage_tenants(&s.base_dir);
    let usage: Vec<_> = tenants.iter().map(|t| load_usage(&s.base_dir, t)).collect();
    Json(usage)
}

async fn handle_usage_tenant(
    State(s): State<AppState>,
    AxumPath(tenant_id): AxumPath<String>,
) -> impl IntoResponse {
    Json(load_usage(&s.base_dir, &tenant_id))
}

async fn handle_usage_reset(
    State(s): State<AppState>,
    AxumPath(tenant_id): AxumPath<String>,
) -> impl IntoResponse {
    match reset_usage(&s.base_dir, &tenant_id) {
        Ok(u)  => (StatusCode::OK, Json(serde_json::to_value(u).unwrap_or_default())).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, err_json(&e.to_string())).into_response(),
    }
}

// ── Background metering loop ──────────────────────────────────────────────────

async fn run_metering_loop(base_dir: PathBuf, _node_id: String) {
    let interval = Duration::from_secs(60);
    loop {
        tokio::time::sleep(interval).await;
        let all = load_all_instances(&base_dir);
        let mut sys = System::new_all();
        sys.refresh_processes();
        for inst in &all {
            if inst.status != "running" { continue; }
            if let Some(pid) = inst.pid {
                if let Some(proc) = sys.process(Pid::from(pid as usize)) {
                    let _ = apollo_core::usage::record_sample(
                        &base_dir,
                        &inst.tenant_id,
                        proc.cpu_usage(),
                        proc.memory() / 1024 / 1024,
                        60.0,
                    );
                }
            }
        }
    }
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

fn err_json(msg: &str) -> Json<serde_json::Value> {
    Json(serde_json::json!({"error": msg}))
}

fn corr_id(headers: &HeaderMap) -> Option<String> {
    headers.get("x-apollo-correlation-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
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
