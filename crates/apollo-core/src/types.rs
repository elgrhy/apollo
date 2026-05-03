//! Shared data types used across the entire APOLLO workspace.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Node Configuration ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeConfig {
    pub node_id:     String,
    pub provider_id: String,
    pub secret_keys: Vec<String>,
    pub profile:     NodeProfile,
    pub network:     NodeNetworkPolicy,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NodeNetworkPolicy {
    pub allow_localhost:      bool,
    pub allow_private_ranges: bool,
    pub rate_limit_rps:       u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NodeProfile {
    pub os:       String,
    pub arch:     String,
    pub ram_gb:   u32,
    pub runtimes: Vec<String>,
    pub llm:      Option<NodeLLMProfile>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeLLMProfile {
    pub provider: String,
    pub model:    String,
    pub endpoint: String,
}

// ── Tenant & Resource Plans ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TenantRecord {
    pub id:            String,
    pub plan:          ResourcePlan,
    pub active_agents: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResourcePlan {
    pub max_agents:   u32,
    pub cpu_limit:    f32,
    pub memory_limit: String,
}

// ── Agent Records ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRecord {
    pub id:          String,
    pub spec:        AgentSpec,
    pub checksum:    String,
    pub created_at:  u64,
    pub prev_version: Option<String>,   // last version before this update
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentInstance {
    pub id:         String,
    pub agent_id:   String,
    pub tenant_id:  String,
    pub status:     String,
    pub pid:        Option<u32>,
    pub port:       Option<u16>,
    pub stats:      ExecutionStats,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ExecutionStats {
    pub cpu_usage_pct: f32,
    pub memory_mb:     u64,
    pub uptime_secs:   u64,
    pub restart_count: u32,
    pub last_restart:  u64,
    pub is_failed:     bool,
}

// ── Control Plane Protocol ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteCommand {
    pub id:     String,
    pub action: String,
    pub agent:  String,
    pub tenant: String,
    pub params: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandResult {
    pub command_id: String,
    pub status:     String,
    pub message:    String,
}

// ── Agent Specification (agent.yaml) ─────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentSpec {
    pub name:           String,
    pub version:        String,
    pub runtime:        AgentRuntimeConfig,
    pub llm:            AgentLLMConfig,
    pub capabilities:   Vec<String>,
    pub triggers:       Vec<String>,
    pub resources:      AgentResourceLimits,
    pub permissions:    AgentPermissionConfig,
    pub compatibility:  AgentCompatibility,
    pub restart_policy: Option<RestartPolicy>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub window_secs:  u32,
}

/// Runtime configuration — accepts both `type:` and `kind:` YAML keys.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRuntimeConfig {
    /// Runtime type: python3 | node | go | deno | bun | ruby | gx | rust | shell | <custom>
    #[serde(rename = "type", alias = "kind")]
    pub kind:    String,
    /// Entry point relative to the agent package directory.
    pub entry:   String,
    /// Optional extra env vars injected into the agent process.
    pub env:     Option<HashMap<String, String>>,
    /// Override launch command. Use `{entry}` as placeholder for the resolved entry path.
    /// Example: "gx run {entry}" or "deno run --allow-net {entry}"
    pub command: Option<String>,
    /// Optional auto-install URLs for missing runtimes. Apollo downloads and installs
    /// to `base_dir/runtimes/{kind}/` if the runtime binary is not found on the node.
    pub install: Option<RuntimeInstallConfig>,
}

/// Per-platform download URLs for auto-installing a runtime binary.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RuntimeInstallConfig {
    pub linux:   Option<String>,
    pub macos:   Option<String>,
    pub windows: Option<String>,
    /// Fallback: a cross-platform install script path (run via shell/bash).
    pub script:  Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentLLMConfig {
    pub required: bool,
    pub provider: String,
    pub fallback: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentResourceLimits {
    pub cpu:     f32,
    pub memory:  String,
    pub timeout: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentPermissionConfig {
    pub network:    String,
    pub filesystem: String,
    pub processes:  String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentCompatibility {
    pub os:   Vec<String>,
    pub arch: Vec<String>,
}

// ── Event Spine ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApolloEvent {
    pub timestamp:      u64,
    pub node_id:        String,
    pub level:          String,
    pub category:       String,
    pub action:         String,
    pub message:        String,
    pub correlation_id: Option<String>,
    pub metadata:       Option<HashMap<String, String>>,
}

pub fn log_event(event: ApolloEvent) {
    let log_path = std::path::PathBuf::from(".apollo/events.jsonl");
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(&event) {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true).append(true).open(log_path)
        {
            let _ = writeln!(file, "{}", json);
        }
    }
    println!("[{}] {} | {} | {}", event.level, event.category, event.action, event.message);
}
