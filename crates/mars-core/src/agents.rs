//! Persistent agent registry — reads/writes `.mars/agents.json`.

use anyhow::{Context, Result};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::types::AgentRecord;

fn agents_json_path() -> PathBuf {
    PathBuf::from(".mars/agents.json")
}

/// Current Unix timestamp in seconds.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Load all agent records from `.mars/agents.json`.
/// Returns an empty `Vec` if the file does not exist yet.
pub fn load_agent_registry() -> Result<Vec<AgentRecord>> {
    let path = agents_json_path();
    if !path.exists() {
        return Ok(vec![]);
    }
    let raw = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse {}", path.display()))
}

/// Overwrite `.mars/agents.json` with the given slice of records.
pub fn save_agent_registry(records: &[AgentRecord]) -> Result<()> {
    let mars_dir = PathBuf::from(".mars");
    if !mars_dir.exists() {
        fs::create_dir_all(&mars_dir).context("Failed to create .mars/ directory")?;
    }
    let json = serde_json::to_string_pretty(records).context("Failed to serialise agent registry")?;
    fs::write(agents_json_path(), json).context("Failed to write agents.json")?;
    Ok(())
}

/// Append or update a single record (matched by `record.id`).
/// If an entry with the same `id` already exists it is replaced; otherwise appended.
pub fn register_agent(record: AgentRecord) -> Result<()> {
    let mut records = load_agent_registry()?;
    if let Some(pos) = records.iter().position(|r| r.id == record.id) {
        records[pos] = record;
    } else {
        records.push(record);
    }
    save_agent_registry(&records)
}
