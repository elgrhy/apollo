//! Persistent agent registry — reads/writes `base_dir/agents.json`.

use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use sha2::Digest;

use crate::types::{AgentRecord, AgentSpec, NodeProfile};
use crate::detect::detect_node_capabilities;

fn agents_json_path(base_dir: &Path) -> PathBuf {
    base_dir.join("agents.json")
}

/// Current Unix timestamp in seconds.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Load all agent records from `base_dir/agents.json`.
pub fn load_agent_registry(base_dir: &Path) -> Result<Vec<AgentRecord>> {
    let path = agents_json_path(base_dir);
    if !path.exists() {
        return Ok(vec![]);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", path.display()))
}

/// Overwrite `base_dir/agents.json` with the given slice of records.
pub fn save_agent_registry(base_dir: &Path, records: &[AgentRecord]) -> Result<()> {
    if !base_dir.exists() {
        fs::create_dir_all(base_dir).context("Failed to create base directory")?;
    }
    let json = serde_json::to_string_pretty(records).context("Failed to serialise agent registry")?;
    fs::write(agents_json_path(base_dir), json).context("Failed to write agents.json")?;
    Ok(())
}

/// Validates an agent package against node capabilities and registers it in `base_dir`.
pub async fn register_agent_package(base_dir: &Path, package_dir: PathBuf) -> Result<AgentRecord> {
    let yaml_path = package_dir.join("agent.yaml");
    if !yaml_path.exists() {
        return Err(anyhow!("agent.yaml not found in {:?}", package_dir));
    }

    let yaml_content = fs::read_to_string(&yaml_path)?;
    let spec: AgentSpec = serde_yaml::from_str(&yaml_content)?;

    let node = detect_node_capabilities().await?;
    validate_agent_compatibility(&spec, &node)?;

    // Global Store: Copy package content to base_dir/agents/{name}
    let global_agent_dir = base_dir.join("agents").join(&spec.name);
    if !global_agent_dir.exists() {
        fs::create_dir_all(&global_agent_dir)?;
    }
    
    for entry in fs::read_dir(&package_dir)? {
        let entry = entry?;
        let dest = global_agent_dir.join(entry.file_name());
        if entry.file_type()?.is_file() {
            fs::copy(entry.path(), dest)?;
        }
    }

    let checksum = format!("{:x}", sha2::Sha256::digest(yaml_content.as_bytes()));

    let record = AgentRecord {
        id: spec.name.clone(),
        spec,
        checksum,
        created_at: now_unix(),
    };

    let mut records = load_agent_registry(base_dir)?;
    if let Some(pos) = records.iter().position(|r| r.id == record.id) {
        records[pos] = record.clone();
    } else {
        records.push(record.clone());
    }
    save_agent_registry(base_dir, &records)?;

    Ok(record)
}

fn validate_agent_compatibility(spec: &AgentSpec, node: &NodeProfile) -> Result<()> {
    if !spec.compatibility.os.is_empty() && !spec.compatibility.os.contains(&node.os) {
        return Err(anyhow!("Agent incompatible with OS: {}", node.os));
    }
    if !spec.compatibility.arch.is_empty() && !spec.compatibility.arch.contains(&node.arch) {
        return Err(anyhow!("Agent incompatible with Architecture: {}", node.arch));
    }
    if !node.runtimes.contains(&spec.runtime.kind) {
        return Err(anyhow!("Required runtime '{}' not found on node", spec.runtime.kind));
    }
    if spec.llm.required && node.llm.is_none() && !spec.llm.fallback {
        return Err(anyhow!("Agent requires LLM but none found on node and fallback disabled"));
    }
    Ok(())
}
