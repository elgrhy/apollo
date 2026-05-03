use crate::AgentRuntime;
use mars_core::types::{AgentSpec, AgentRecord};
use async_trait::async_trait;
use anyhow::{Result, Context};
use std::process::Stdio;
use tokio::process::Command;
use std::time::SystemTime;

pub struct ProcessRuntime {
    /// Base directory where agents are installed and run.
    pub base_dir: std::path::PathBuf,
}

impl ProcessRuntime {
    pub fn new(base_dir: std::path::PathBuf) -> Self {
        Self { base_dir }
    }
}

#[async_trait]
impl AgentRuntime for ProcessRuntime {
    async fn install(&self, spec: &AgentSpec) -> Result<()> {
        let agent_dir = self.base_dir.join(&spec.name);
        tokio::fs::create_dir_all(&agent_dir).await
            .with_context(|| format!("Failed to create directory for agent {}", spec.name))?;
        
        // In a real implementation, we would download the agent package here.
        // For now, we assume the binary/script is already placed or handled.
        Ok(())
    }

    async fn start(&self, spec: &AgentSpec) -> Result<AgentRecord> {
        let agent_dir = self.base_dir.join(&spec.name);
        
        let mut cmd = Command::new(&spec.runtime.entry);
        cmd.current_dir(&agent_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Set environment variables
        if let Some(env) = &spec.runtime.env {
            for (key, value) in env {
                cmd.env(key, value);
            }
        }

        // Apply resource limits (simplified for Phase 1)
        // In a production-grade system, we would use cgroups here.
        
        let _child = cmd.spawn()
            .with_context(|| format!("Failed to spawn agent {}", spec.name))?;

        let created_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        Ok(AgentRecord {
            id: spec.name.clone(),
            spec: spec.clone(),
            created_at,
            status: "running".to_string(),
            endpoint: None, // Will be filled if the agent exposes an API
        })
    }

    async fn stop(&self, _id: &str) -> Result<()> {
        // In a real implementation, we would keep track of PIDs.
        // For Phase 1, we might just search for the process or skip this.
        Ok(())
    }

    async fn status(&self, _id: &str) -> Result<String> {
        Ok("unknown".to_string())
    }

    async fn health_check(&self, _id: &str) -> Result<bool> {
        Ok(true)
    }
}
