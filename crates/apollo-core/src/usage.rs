//! Per-tenant usage metering.
//!
//! Every 60 s the node's metering task reads CPU and memory for each running
//! agent process and accumulates totals in `base_dir/usage/{tenant_id}.json`.
//! The `/usage/{tenant_id}` endpoint exposes these for billing pipelines.
//! `POST /usage/{tenant_id}/reset` resets counters for a new billing cycle.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TenantUsage {
    pub tenant_id:          String,
    pub cpu_seconds:        f64,
    pub memory_gb_seconds:  f64,
    pub total_starts:       u64,
    pub total_stops:        u64,
    pub current_instances:  u64,
    pub period_start:       u64,
    pub last_updated:       u64,
}

fn usage_path(base_dir: &Path, tenant_id: &str) -> PathBuf {
    base_dir
        .join("usage")
        .join(format!("{}.json", sanitize(tenant_id)))
}

pub fn load_usage(base_dir: &Path, tenant_id: &str) -> TenantUsage {
    let path = usage_path(base_dir, tenant_id);
    if !path.exists() {
        return TenantUsage { tenant_id: tenant_id.into(), ..Default::default() };
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| TenantUsage { tenant_id: tenant_id.into(), ..Default::default() })
}

pub fn save_usage(base_dir: &Path, usage: &TenantUsage) -> Result<()> {
    let path = usage_path(base_dir, &usage.tenant_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create usage dir")?;
    }
    fs::write(path, serde_json::to_string_pretty(usage)?)?;
    Ok(())
}

pub fn reset_usage(base_dir: &Path, tenant_id: &str) -> Result<TenantUsage> {
    let now = now_unix();
    let fresh = TenantUsage {
        tenant_id:    tenant_id.into(),
        period_start: now,
        last_updated: now,
        ..Default::default()
    };
    save_usage(base_dir, &fresh)?;
    Ok(fresh)
}

/// Accumulate a metering sample (called every 60 s per running instance).
pub fn record_sample(
    base_dir: &Path,
    tenant_id: &str,
    cpu_pct: f32,
    memory_mb: u64,
    interval_secs: f64,
) -> Result<()> {
    let mut u = load_usage(base_dir, tenant_id);
    u.tenant_id         = tenant_id.into();
    u.cpu_seconds      += (cpu_pct as f64 / 100.0) * interval_secs;
    u.memory_gb_seconds += (memory_mb as f64 / 1024.0) * interval_secs;
    u.last_updated      = now_unix();
    if u.period_start == 0 { u.period_start = u.last_updated; }
    save_usage(base_dir, &u)
}

pub fn record_start(base_dir: &Path, tenant_id: &str) -> Result<()> {
    let mut u = load_usage(base_dir, tenant_id);
    u.tenant_id = tenant_id.into();
    u.total_starts += 1;
    u.current_instances += 1;
    u.last_updated = now_unix();
    if u.period_start == 0 { u.period_start = u.last_updated; }
    save_usage(base_dir, &u)
}

pub fn record_stop(base_dir: &Path, tenant_id: &str) -> Result<()> {
    let mut u = load_usage(base_dir, tenant_id);
    u.tenant_id = tenant_id.into();
    u.total_stops += 1;
    u.current_instances = u.current_instances.saturating_sub(1);
    u.last_updated = now_unix();
    save_usage(base_dir, &u)
}

/// List all tenants that have usage records.
pub fn list_usage_tenants(base_dir: &Path) -> Vec<String> {
    let dir = base_dir.join("usage");
    if !dir.exists() { return vec![]; }
    fs::read_dir(&dir)
        .ok()
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.strip_suffix(".json").map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
