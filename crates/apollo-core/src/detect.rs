//! Node capability detection — OS, arch, RAM, available runtimes, LLM.

use crate::types::{NodeProfile, NodeLLMProfile};
use anyhow::Result;
use sysinfo::System;
use std::{env, path::Path};

/// Detect all capabilities of this node. Includes runtimes installed both
/// system-wide (via PATH) and locally in `base_dir/runtimes/`.
pub async fn detect_node_capabilities() -> Result<NodeProfile> {
    detect_with_runtimes_dir(None).await
}

pub async fn detect_node_capabilities_with_dir(runtimes_dir: &Path) -> Result<NodeProfile> {
    detect_with_runtimes_dir(Some(runtimes_dir)).await
}

async fn detect_with_runtimes_dir(runtimes_dir: Option<&Path>) -> Result<NodeProfile> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let os = env::consts::OS.to_string();
    let os = if os == "macos" { "darwin".to_string() } else { os };
    let arch = env::consts::ARCH.to_string();
    let ram_gb = (sys.total_memory() / 1024 / 1024 / 1024) as u32;

    let mut runtimes = Vec::new();

    // System runtimes (via PATH)
    let system_candidates = [
        "python3", "python", "node", "nodejs", "rustc", "go", "deno",
        "bun", "ruby", "php", "perl", "java", "dotnet", "pwsh", "gx",
        "julia", "swift", "zig",
    ];
    for binary in &system_candidates {
        if which::which(binary).is_ok() {
            let name = normalize_runtime_name(binary);
            if !runtimes.contains(&name) {
                runtimes.push(name);
            }
        }
    }

    // Also check locally-installed runtimes in runtimes_dir
    if let Some(dir) = runtimes_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let kind = entry.file_name().to_string_lossy().to_string();
                    if !runtimes.contains(&kind) {
                        runtimes.push(kind);
                    }
                }
            }
        }
    }

    // Also accept "shell" as always available (sh is always present on Unix, cmd/pwsh on Windows)
    #[cfg(unix)]
    if !runtimes.contains(&"shell".to_string()) {
        runtimes.push("shell".to_string());
    }
    #[cfg(windows)]
    if !runtimes.contains(&"powershell".to_string()) {
        runtimes.push("powershell".to_string());
    }

    let llm = detect_ollama().await;

    Ok(NodeProfile {
        os,
        arch,
        ram_gb,
        runtimes,
        llm,
    })
}

fn normalize_runtime_name(binary: &str) -> String {
    match binary {
        "python" => "python3",
        "nodejs" => "node",
        _ => binary,
    }
    .to_string()
}

async fn detect_ollama() -> Option<NodeLLMProfile> {
    let client = reqwest::Client::new();
    let res = client
        .get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await;

    if let Ok(response) = res {
        if response.status().is_success() {
            return Some(NodeLLMProfile {
                provider: "ollama".to_string(),
                model: "llama3.2:latest".to_string(),
                endpoint: "http://localhost:11434/api/chat".to_string(),
            });
        }
    }
    None
}
