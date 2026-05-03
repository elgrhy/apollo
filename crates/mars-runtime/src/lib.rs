use async_trait::async_trait;
use mars_core::types::{AgentSpec, AgentInstance};
use anyhow::Result;

#[async_trait]
pub trait AgentRuntime {
    /// Prepare the environment for the agent globally.
    async fn install(&self, spec: &AgentSpec) -> Result<()>;
    
    /// Prepare a tenant-specific sandbox for the agent.
    async fn activate(&self, tenant_id: &str, spec: &AgentSpec) -> Result<()>;
    
    /// Start an agent instance for a specific tenant.
    async fn start(&self, tenant_id: &str, spec: &AgentSpec) -> Result<AgentInstance>;
    
    /// Stop a running agent instance.
    async fn stop(&self, pid: u32) -> Result<()>;
    
    /// Get the current status of an agent instance.
    async fn status(&self, instance_id: &str) -> Result<String>;
    
    /// Perform a health check on the agent.
    async fn health_check(&self, instance_id: &str) -> Result<bool>;

    /// Gracefully shutdown all active instances.
    async fn shutdown(&self) -> Result<()>;
}

pub mod process;
