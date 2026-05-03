use crate::types::{NodeProfile, NodeLLMProfile};
use anyhow::Result;
use sysinfo::System;
use std::env;

pub async fn detect_node_capabilities() -> Result<NodeProfile> {
    let mut sys = System::new_all();
    sys.refresh_all();

    let os = env::consts::OS.to_string();
    let os = if os == "macos" { "darwin".to_string() } else { os };
    let arch = env::consts::ARCH.to_string();
    let ram_gb = (sys.total_memory() / 1024 / 1024 / 1024) as u32;

    let mut runtimes = Vec::new();
    if which::which("python3").is_ok() { runtimes.push("python3".to_string()); }
    if which::which("node").is_ok() { runtimes.push("node".to_string()); }
    if which::which("rustc").is_ok() { runtimes.push("rustc".to_string()); }

    // Detect LLM (Ollama as primary provider-grade local target)
    let llm = detect_ollama().await;

    Ok(NodeProfile {
        os,
        arch,
        ram_gb,
        runtimes,
        llm,
    })
}

async fn detect_ollama() -> Option<NodeLLMProfile> {
    let client = reqwest::Client::new();
    let res = client.get("http://localhost:11434/api/tags")
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await;

    if let Ok(response) = res {
        if response.status().is_success() {
            // For Phase 1, we just assume it's available if it responds
            return Some(NodeLLMProfile {
                provider: "ollama".to_string(),
                model: "llama3.2:latest".to_string(), // In a real app we'd probe available models
                endpoint: "http://localhost:11434/api/chat".to_string(),
            });
        }
    }
    None
}
