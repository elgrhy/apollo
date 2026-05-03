use anyhow::{Context, Result};
use crate::net::{CLIENT, friendly_net_error};

const ANTHROPIC_URL: &str = "https://api.anthropic.com/v1/messages";

pub async fn call_anthropic(
    api_key:  &str,
    model:    &str,
    messages: &[serde_json::Value],
) -> Result<String> {
    let mut sys  = None::<String>;
    let mut rest = Vec::new();
    for msg in messages {
        if msg["role"].as_str() == Some("system") {
            sys = msg["content"].as_str().map(|s| s.to_string());
        } else {
            rest.push(msg.clone());
        }
    }

    let mut body = serde_json::json!({
        "model": model, "max_tokens": 8096, "messages": rest
    });
    if let Some(s) = sys {
        body["system"] = serde_json::json!(s);
    }

    let res = CLIENT.post(ANTHROPIC_URL)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send().await
        .map_err(|e| friendly_net_error(e, ANTHROPIC_URL))?;

    if !res.status().is_success() {
        let status   = res.status();
        let err_body: serde_json::Value = res.json().await.unwrap_or_default();
        let msg = err_body["error"]["message"].as_str().unwrap_or("unknown error");
        return Err(anyhow::anyhow!("❌ Anthropic API error ({}): {}", status, msg));
    }

    let json: serde_json::Value = res.json().await
        .context("Anthropic returned non-JSON")?;

    json.get("content")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Unexpected Anthropic response: {}", json))
}
