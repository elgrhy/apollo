//! .env read/write helpers for LLM configuration.

use crate::types::LLMConfig;
use anyhow::{Context, Result};
use std::{collections::HashMap, fs};

/// Write LLM config to .env in the current working directory.
pub fn enable_bootstrap(config: &LLMConfig) -> Result<()> {
    let mut content = String::new();
    content.push_str(&format!("LLM_TYPE={}\n", config.llm_type));

    if config.llm_type == "local" {
        content.push_str(&format!(
            "LLM_ENDPOINT={}\n",
            config.endpoint.as_deref().unwrap_or("http://localhost:11434/api/chat")
        ));
        if let Some(m) = &config.model {
            content.push_str(&format!("LLM_MODEL={}\n", m));
        }
    } else {
        if let Some(p) = &config.provider { content.push_str(&format!("LLM_PROVIDER={}\n", p)); }
        if let Some(k) = &config.api_key  { content.push_str(&format!("LLM_API_KEY={}\n",  k)); }
        if let Some(m) = &config.model    { content.push_str(&format!("LLM_MODEL={}\n",    m)); }
    }

    fs::write(".env", content).context("Failed to write .env")
}

/// Read LLM config from .env in the current working directory.
pub fn load_llm_config_from_env() -> Result<LLMConfig> {
    let content = fs::read_to_string(".env").context("No .env file found")?;
    let mut map = HashMap::<String, String>::new();
    for line in content.lines() {
        if let Some((k, v)) = line.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Ok(LLMConfig {
        llm_type: map.get("LLM_TYPE").cloned().unwrap_or_else(|| "local".to_string()),
        endpoint: map.get("LLM_ENDPOINT").cloned(),
        provider: map.get("LLM_PROVIDER").cloned(),
        model:    map.get("LLM_MODEL").cloned(),
        api_key:  map.get("LLM_API_KEY").cloned(),
    })
}

/// Load LLM config for MARS-internal operations (consult, advisor).
/// Checks `MARS_LLM_*` env vars first; falls back to the user's `LLM_*` config from `.env`.
///
/// This separation lets operators run MARS's own AI operations on a different
/// provider/model than the one the user's agents are configured to use.
pub fn load_mars_llm_config() -> Result<LLMConfig> {
    // Check runtime env vars for MARS_LLM_* overrides
    let mars_key      = std::env::var("MARS_LLM_KEY").ok();
    let mars_provider = std::env::var("MARS_LLM_PROVIDER").ok();
    let mars_model    = std::env::var("MARS_LLM_MODEL").ok();
    let mars_type     = std::env::var("MARS_LLM_TYPE").ok();
    let mars_endpoint = std::env::var("MARS_LLM_ENDPOINT").ok();

    if mars_key.is_some() || mars_provider.is_some() || mars_type.as_deref() == Some("local") {
        return Ok(LLMConfig {
            llm_type: mars_type.unwrap_or_else(|| "cloud".to_string()),
            endpoint: mars_endpoint,
            provider: mars_provider,
            model:    mars_model,
            api_key:  mars_key,
        });
    }

    // No MARS_LLM_* override — fall back to user's LLM config
    load_llm_config_from_env()
}

/// Load extension_protocol.md if it exists.
pub fn load_extension_protocol() -> Option<String> {
    fs::read_to_string("extension_protocol.md").ok()
}

/// Load protocol.md if present, otherwise return the embedded fallback.
pub fn load_protocol(fallback: &str) -> String {
    match fs::read_to_string("protocol.md") {
        Ok(content) => {
            println!("   (protocol.md found on disk — using custom protocol)");
            content
        }
        Err(_) => fallback.to_string(),
    }
}
