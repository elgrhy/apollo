//! Shared data types used across the entire MARS workspace.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Node Configuration ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeConfig {
    pub node_id:     String,
    pub provider_id: String,
    pub secret_keys: Vec<String>, // Multiple keys supported
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
    pub id:   String,
    pub plan: ResourcePlan,
    pub active_agents: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResourcePlan {
    pub max_agents:   u32,
    pub cpu_limit:    f32,
    pub memory_limit: String,
}

// ── Agent Records (Activation Layer) ──────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRecord {
    pub id:         String,
    pub spec:       AgentSpec,
    pub checksum:   String,
    pub created_at: u64,
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

// ── Control Plane Protocol ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteCommand {
    pub id:      String,
    pub action:  String,
    pub agent:   String,
    pub tenant:  String,
    pub params:  Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandResult {
    pub command_id: String,
    pub status:     String,
    pub message:    String,
}

// ── Agent Specification (agent.yaml) ──────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentSpec {
    pub name:          String,
    pub version:       String,
    pub runtime:       AgentRuntimeConfig,
    pub llm:           AgentLLMConfig,
    pub capabilities:  Vec<String>,
    pub triggers:      Vec<String>,
    pub resources:     AgentResourceLimits,
    pub permissions:   AgentPermissionConfig,
    pub compatibility: AgentCompatibility,
    pub restart_policy: Option<RestartPolicy>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub window_secs:  u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRuntimeConfig {
    #[serde(rename = "type")]
    pub kind:  String,
    pub entry: String,
    pub env:   Option<HashMap<String, String>>,
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
