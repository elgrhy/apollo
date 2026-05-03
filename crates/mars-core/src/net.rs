//! Shared HTTP client and network error helpers.

lazy_static::lazy_static! {
    /// Shared reqwest client with a 30-second timeout.
    /// Re-used across all LLM calls and capability probes to avoid
    /// creating a new connection pool for every request.
    pub static ref CLIENT: reqwest::Client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("Failed to build HTTP client");
}

/// Convert a reqwest network error into a human-readable message.
pub fn friendly_net_error(e: reqwest::Error, target: &str) -> anyhow::Error {
    if e.is_timeout() {
        anyhow::anyhow!(
            "⏱️  Request timed out. The server at {} took too long to respond.\n\
             If this is a local LLM, make sure it is running (e.g. `ollama serve`).",
            target
        )
    } else if e.is_connect() {
        anyhow::anyhow!(
            "🔌 Cannot connect to {}.\n\
             Make sure the service is running and reachable.",
            target
        )
    } else {
        anyhow::anyhow!("🌐 Network error talking to {}: {}", target, e)
    }
}
