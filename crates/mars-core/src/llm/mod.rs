//! LLM routing — dispatches to the correct provider.

mod openai;
mod anthropic;
mod google;
mod models;

pub use models::fetch_available_models;

use crate::types::LLMConfig;
use anyhow::{Context, Result};
use crate::net::{CLIENT, friendly_net_error};

/// Route a chat request to the correct LLM backend.
pub async fn call_llm(config: &LLMConfig, messages: &[serde_json::Value]) -> Result<String> {
    if config.llm_type == "local" {
        return call_local(config, messages).await;
    }
    let provider = config.provider.as_deref().unwrap_or("openai");
    let api_key  = config.api_key.as_deref().unwrap_or("");
    let model    = config.model.as_deref().unwrap_or_else(|| match provider {
        "anthropic" => "claude-opus-4-6",
        "google"    => "gemini-2.0-flash",
        "groq"      => "llama3-70b-8192",
        _           => "gpt-4o",
    });

    match provider {
        "anthropic" => anthropic::call_anthropic(api_key, model, messages).await,
        "google"    => google::call_google(api_key, model, messages).await,
        _           => openai::call_openai_compat(provider, api_key, model, messages).await,
    }
}

async fn call_local(config: &LLMConfig, messages: &[serde_json::Value]) -> Result<String> {
    let endpoint  = config.endpoint.as_deref()
        .unwrap_or("http://localhost:11434/api/chat");
    let model     = config.model.as_deref().unwrap_or("llama3");
    let is_ollama = endpoint.contains(":11434")
        || config.provider.as_deref() == Some("ollama");

    let body = if is_ollama {
        serde_json::json!({ "model": model, "messages": messages, "stream": false })
    } else {
        serde_json::json!({ "model": model, "messages": messages })
    };

    let res = CLIENT.post(endpoint).json(&body).send().await
        .map_err(|e| friendly_net_error(e, endpoint))?;

    if !res.status().is_success() {
        let status    = res.status();
        let body_text = res.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!(
            "❌ Local LLM returned {} — {}",
            status,
            body_text.chars().take(200).collect::<String>()
        ));
    }

    let json: serde_json::Value = res.json().await
        .context("Local LLM returned non-JSON")?;

    if is_ollama {
        json["message"]["content"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Unexpected Ollama response: {}", json))
    } else {
        json.get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("Unexpected local LLM response: {}", json))
    }
}
