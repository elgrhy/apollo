use async_trait::async_trait;
use mars_core::types::{AgentSpec, AgentRecord};
use anyhow::Result;

#[async_trait]
pub trait AgentRuntime {
    /// Prepare the environment for the agent (e.g., download binaries, create sandbox).
    async fn install(&self, spec: &AgentSpec) -> Result<()>;
    
    /// Start the agent and return a record of the running instance.
    async fn start(&self, spec: &AgentSpec) -> Result<AgentRecord>;
    
    /// Stop a running agent instance.
    async fn stop(&self, id: &str) -> Result<()>;
    
    /// Get the current status of an agent instance.
    async fn status(&self, id: &str) -> Result<String>;
    
    /// Perform a health check on the agent.
    async fn health_check(&self, id: &str) -> Result<bool>;
}

pub mod process;
// pub mod docker; // Future
// pub mod wasm;   // Future
