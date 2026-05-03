//! Per-tenant secret storage.
//!
//! Secrets are stored in `base_dir/secrets/{tenant_id}.json` as a flat
//! key→value map. At spawn time they are merged into the agent's environment
//! so agent code never touches key management.
//!
//! Encryption is intentionally deferred to a future key-management integration
//! (Vault, AWS Secrets Manager, etc.). The file is protected by OS filesystem
//! permissions; ensure `base_dir` is not world-readable.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TenantSecrets {
    pub secrets: HashMap<String, String>,
}

fn secrets_path(base_dir: &Path, tenant_id: &str) -> PathBuf {
    base_dir
        .join("secrets")
        .join(format!("{}.json", sanitize(tenant_id)))
}

pub fn load_secrets(base_dir: &Path, tenant_id: &str) -> TenantSecrets {
    let path = secrets_path(base_dir, tenant_id);
    if !path.exists() {
        return TenantSecrets::default();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_secrets(base_dir: &Path, tenant_id: &str, secrets: &TenantSecrets) -> Result<()> {
    let path = secrets_path(base_dir, tenant_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create secrets dir")?;
    }
    #[cfg(unix)]
    {
        // Write with restricted permissions before populating
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true).mode(0o600);
        let mut f = opts.open(&path).context("open secrets file")?;
        use std::io::Write;
        write!(f, "{}", serde_json::to_string_pretty(secrets)?)?;
        return Ok(());
    }
    #[allow(unreachable_code)]
    {
        fs::write(path, serde_json::to_string_pretty(secrets)?)?;
        Ok(())
    }
}

pub fn delete_secrets(base_dir: &Path, tenant_id: &str) -> Result<()> {
    let path = secrets_path(base_dir, tenant_id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Merge a map of new secrets into the existing set for a tenant.
pub fn upsert_secrets(base_dir: &Path, tenant_id: &str, new: HashMap<String, String>) -> Result<()> {
    let mut existing = load_secrets(base_dir, tenant_id);
    existing.secrets.extend(new);
    save_secrets(base_dir, tenant_id, &existing)
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
