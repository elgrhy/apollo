//! Shared data types used across the entire MARS workspace.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── System profile ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Profile {
    pub hardware:     Hardware,
    pub os:           OS,
    pub environment:  Environment,
    pub network:      Network,
    pub binaries:     HashMap<String, String>,
    pub llm:          Option<LLMConfig>,
    pub capabilities: CapabilityMap,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Hardware {
    pub arch:             String,
    pub cpu_cores:        usize,
    pub total_memory_gb:  u64,
    /// True when running on a mobile OS (iOS, Android) or ARM device.
    pub mobile:           bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct OS {
    pub name:    String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Environment {
    pub shell:    String,
    pub user:     String,
    pub hostname: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct Network {
    pub online: bool,
}

// ── LLM config ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct LLMConfig {
    #[serde(rename = "type")]
    pub llm_type: String,
    pub endpoint: Option<String>,
    pub provider: Option<String>,
    pub model:    Option<String>,
    /// API key — never embedded in model field, never logged.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key:  Option<String>,
}

// ── Capability map ────────────────────────────────────────────────────────────

/// A single locally-running service (LLM backend, vision model, MCP server, …)
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LocalService {
    pub name:  String,
    pub url:   String,
    pub kind:  String,
    pub model: Option<String>,
}

/// System-level capability flags.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct SystemCaps {
    pub gpu:      bool,
    pub docker:   bool,
    pub cron:     bool,
    pub systemd:  bool,
    pub internet: bool,
}

/// Full multi-modal capability map — passed verbatim to the active agent.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CapabilityMap {
    pub llm_backends:     Vec<LocalService>,
    pub vision:           Vec<LocalService>,
    pub audio:            Vec<LocalService>,
    pub image_gen:        Vec<LocalService>,
    pub tts:              Vec<LocalService>,
    pub embeddings:       Vec<LocalService>,
    pub mcp_servers:      Vec<LocalService>,
    pub runtimes:         Vec<String>,
    pub package_managers: Vec<String>,
    pub services:         Vec<String>,
    pub system:           SystemCaps,
}

// ── Ghost protocol ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct GhostConfig {
    pub secret_key:  String,
    pub webhook_url: Option<String>,
    pub remove_self: bool,
}

// ── Swarm ─────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SwarmPeer {
    pub ip:       String,
    pub user:     String,
    pub arch:     String,
    pub hostname: String,
}

// ── License ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct LicenseCache {
    pub key:          String,
    pub tier:         String,
    pub email:        String,
    pub validated_at: u64,
}

// ── Agent Records ─────────────────────────────────────────────────────────────

/// One persistent entry in the node's agent registry.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRecord {
    pub id:         String,
    pub spec:       AgentSpec,
    /// Unix timestamp (seconds since epoch) of when the agent was created/installed.
    pub created_at: u64,
    /// `"running"` | `"stopped"` | `"error"` | `"installed"`
    pub status:     String,
    /// Optional endpoint if the agent exposes a service.
    pub endpoint:   Option<String>,
}
// ── MARS Provider Edition — Agent Spec ──────────────────────────────────────

/// The full configuration for a MARS-compatible agent package.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentSpec {
    pub name:        String,
    pub version:     String,
    pub description: String,
    pub runtime:     RuntimeConfig,
    pub limits:      ResourceLimits,
    pub permissions: PermissionConfig,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RuntimeConfig {
    /// Entry point for the agent (binary name, script path, or WASM module).
    pub entry:   String,
    /// Language or execution environment (e.g., "rust", "python", "wasm", "node").
    pub kind:    String,
    /// Model requirements for this agent.
    pub model:   ModelRequirement,
    /// Capabilities or tools the agent requires.
    pub tools:   Vec<String>,
    /// Environment variables for the agent runtime.
    pub env:     Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum ModelRequirement {
    Required,
    Optional,
    None,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResourceLimits {
    /// CPU limit (e.g., 0.5 for half a core).
    pub cpu:     f32,
    /// Memory limit in MB.
    pub memory:  u64,
    /// Timeout in seconds for a single execution or request.
    pub timeout: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PermissionConfig {
    /// Network access policy (e.g., "none", "restricted", "full").
    pub network:    String,
    /// Filesystem access policy (e.g., "sandboxed", "read-only", "full").
    pub filesystem: String,
    /// Specific allowed domains for network access.
    pub allowed_hosts: Option<Vec<String>>,
}
