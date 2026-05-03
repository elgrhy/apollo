use anyhow::{Context, Result};
use crate::net::{CLIENT, friendly_net_error};

const OPENAI_URL: &str = "https://api.openai.com/v1/chat/completions";
const GROQ_URL:   &str = "https://api.groq.com/openai/v1/chat/completions";

pub async fn call_openai_compat(
    provider: &str,
    api_key:  &str,
    model:    &str,
    messages: &[serde_json::Value],
) -> Result<String> {
    let url            = if provider == "groq" { GROQ_URL } else { OPENAI_URL };
    let provider_label = if provider == "groq" { "Groq" } else { "OpenAI" };
    let body           = serde_json::json!({ "model": model, "messages": messages });

    let res = CLIENT.post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send().await
        .map_err(|e| friendly_net_error(e, url))?;

    if !res.status().is_success() {
        let status   = res.status();
        let err_body: serde_json::Value = res.json().await.unwrap_or_default();
        let msg = err_body["error"]["message"].as_str().unwrap_or("unknown error");
        return Err(anyhow::anyhow!("❌ {} API error ({}): {}", provider_label, status, msg));
    }

    let json: serde_json::Value = res.json().await
        .context("OpenAI-compat returned non-JSON")?;

    json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Unexpected {} response: {}", provider_label, json))
}
