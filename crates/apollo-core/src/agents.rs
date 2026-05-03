//! Persistent agent registry with URL sourcing, versioning, and rollback.

use anyhow::{Context, Result, anyhow};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use sha2::Digest;

use crate::types::{AgentRecord, AgentSpec, NodeProfile};
use crate::detect::detect_node_capabilities;
use crate::fetch::{resolve_agent_source, make_staging_dir, cleanup_staging};
use crate::runtime_registry::ensure_runtime;

fn agents_json_path(base_dir: &Path) -> PathBuf {
    base_dir.join("agents.json")
}

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

/// Overwrite `base_dir/agents.json`.
pub fn save_agent_registry(base_dir: &Path, records: &[AgentRecord]) -> Result<()> {
    if !base_dir.exists() {
        fs::create_dir_all(base_dir).context("Failed to create base directory")?;
    }
    let json = serde_json::to_string_pretty(records)?;
    fs::write(agents_json_path(base_dir), json)?;
    Ok(())
}

/// Register or update an agent package from a local path or remote source.
///
/// `source` may be:
///   - Absolute local path: `/opt/agents/openclaw`
///   - Relative local path: `./examples/openclaw`
///   - HTTPS archive URL: `https://example.com/openclaw-1.0.tar.gz`
///   - HTTPS zip URL:     `https://example.com/openclaw-1.0.zip`
///   - Git URL:           `https://github.com/org/openclaw.git` or `git@github.com:...`
pub async fn register_agent_package(base_dir: &Path, source: &str) -> Result<AgentRecord> {
    // Resolve source (may download)
    let staging_dir = make_staging_dir(base_dir)?;
    let package_dir = resolve_agent_source(source, &staging_dir).await?;

    let result = do_register(base_dir, &package_dir).await;
    cleanup_staging(&staging_dir);
    result
}

async fn do_register(base_dir: &Path, package_dir: &Path) -> Result<AgentRecord> {
    let yaml_path = package_dir.join("agent.yaml");
    if !yaml_path.exists() {
        return Err(anyhow!("agent.yaml not found in {:?}", package_dir));
    }

    let yaml_content = fs::read_to_string(&yaml_path)?;
    let spec: AgentSpec = serde_yaml::from_str(&yaml_content)
        .with_context(|| format!("Invalid agent.yaml in {:?}", package_dir))?;

    // Validate node compatibility
    let node = detect_node_capabilities().await?;
    validate_agent_compatibility(&spec, &node)?;

    // Auto-install runtime if missing
    let runtimes_dir = base_dir.join("runtimes");
    fs::create_dir_all(&runtimes_dir)?;
    ensure_runtime(&spec.runtime, &runtimes_dir).await?;

    // Global store: copy files to base_dir/agents/{name}/
    let global_agent_dir = base_dir.join("agents").join(&spec.name);

    // Backup previous version if it exists
    let mut prev_version: Option<String> = None;
    let mut records = load_agent_registry(base_dir)?;
    if let Some(existing) = records.iter().find(|r| r.id == spec.name) {
        let existing_version = existing.spec.version.clone();
        // Always record prev_version so rollback works across multiple updates
        prev_version = Some(existing_version.clone());
        // Only create the backup dir if it doesn't already exist
        if global_agent_dir.exists() {
            let backup_dir = base_dir
                .join("agents")
                .join(format!("{}.v{}", spec.name, existing_version));
            if !backup_dir.exists() {
                let _ = copy_dir_all(&global_agent_dir, &backup_dir);
                println!("[REGISTRY] Backed up v{} to {:?}", existing_version, backup_dir);
            }
        }
    }

    fs::create_dir_all(&global_agent_dir)?;
    copy_dir_all(package_dir, &global_agent_dir)?;

    let checksum = format!("{:x}", sha2::Sha256::digest(yaml_content.as_bytes()));

    let record = AgentRecord {
        id: spec.name.clone(),
        spec,
        checksum,
        created_at: now_unix(),
        prev_version,
    };

    if let Some(pos) = records.iter().position(|r| r.id == record.id) {
        println!("[REGISTRY] Updated agent '{}'", record.id);
        records[pos] = record.clone();
    } else {
        println!("[REGISTRY] Registered new agent '{}'", record.id);
        records.push(record.clone());
    }
    save_agent_registry(base_dir, &records)?;

    Ok(record)
}

/// Rollback an agent to its previous version.
pub fn rollback_agent(base_dir: &Path, name: &str) -> Result<()> {
    let mut records = load_agent_registry(base_dir)?;
    let pos = records
        .iter()
        .position(|r| r.id == name)
        .ok_or_else(|| anyhow!("Agent '{}' not found", name))?;

    let prev_ver = records[pos]
        .prev_version
        .clone()
        .ok_or_else(|| anyhow!("No previous version stored for '{}'", name))?;

    let backup_dir = base_dir
        .join("agents")
        .join(format!("{}.v{}", name, prev_ver));

    if !backup_dir.exists() {
        return Err(anyhow!("Backup directory not found: {:?}", backup_dir));
    }

    let current_dir = base_dir.join("agents").join(name);
    fs::remove_dir_all(&current_dir)?;
    copy_dir_all(&backup_dir, &current_dir)?;

    // Reload yaml from backup
    let yaml_content = fs::read_to_string(current_dir.join("agent.yaml"))?;
    let spec: AgentSpec = serde_yaml::from_str(&yaml_content)?;
    let checksum = format!("{:x}", sha2::Sha256::digest(yaml_content.as_bytes()));

    records[pos] = AgentRecord {
        id: name.to_string(),
        spec,
        checksum,
        created_at: now_unix(),
        prev_version: None,
    };
    save_agent_registry(base_dir, &records)?;
    println!("[REGISTRY] Rolled back '{}' to version {}", name, prev_ver);
    Ok(())
}

/// Remove a registered agent (stops any running instances first).
pub fn remove_agent(base_dir: &Path, name: &str) -> Result<()> {
    let mut records = load_agent_registry(base_dir)?;
    let len_before = records.len();
    records.retain(|r| r.id != name);
    if records.len() == len_before {
        return Err(anyhow!("Agent '{}' not found in registry", name));
    }

    let agent_dir = base_dir.join("agents").join(name);
    if agent_dir.exists() {
        fs::remove_dir_all(&agent_dir)?;
    }

    save_agent_registry(base_dir, &records)?;
    println!("[REGISTRY] Removed agent '{}'", name);
    Ok(())
}

fn validate_agent_compatibility(spec: &AgentSpec, node: &NodeProfile) -> Result<()> {
    if !spec.compatibility.os.is_empty() {
        let os = node.os.as_str();
        // Accept both "macos" and "darwin"
        let matched = spec.compatibility.os.iter().any(|o| {
            o == os || (os == "darwin" && o == "macos") || (os == "macos" && o == "darwin")
        });
        if !matched {
            return Err(anyhow!(
                "Agent requires OS {:?}, this node is '{}'",
                spec.compatibility.os,
                node.os
            ));
        }
    }
    if !spec.compatibility.arch.is_empty() && !spec.compatibility.arch.contains(&node.arch) {
        return Err(anyhow!(
            "Agent requires arch {:?}, this node is '{}'",
            spec.compatibility.arch,
            node.arch
        ));
    }
    // LLM requirement check
    if spec.llm.required && node.llm.is_none() && !spec.llm.fallback {
        return Err(anyhow!(
            "Agent requires an LLM but none found on node (set llm.fallback: true to bypass)"
        ));
    }
    Ok(())
}

/// Recursively copy all files from `src` into `dst`.
fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}
