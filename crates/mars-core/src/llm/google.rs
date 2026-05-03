use anyhow::{Context, Result};
use crate::net::{CLIENT, friendly_net_error};

pub async fn call_google(
    api_key:  &str,
    model:    &str,
    messages: &[serde_json::Value],
) -> Result<String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let mut contents     = Vec::<serde_json::Value>::new();
    let mut system_parts = Vec::<serde_json::Value>::new();

    for msg in messages {
        let role    = msg["role"].as_str().unwrap_or("user");
        let content = msg["content"].as_str().unwrap_or("");
        if role == "system" {
            system_parts.push(serde_json::json!({ "text": content }));
        } else {
            let gemini_role = if role == "assistant" { "model" } else { "user" };
            contents.push(serde_json::json!({
                "role": gemini_role,
                "parts": [{ "text": content }]
            }));
        }
    }

    let mut body = serde_json::json!({ "contents": contents });
    if !system_parts.is_empty() {
        body["systemInstruction"] = serde_json::json!({ "parts": system_parts });
    }

    let res = CLIENT.post(&url).json(&body).send().await
        .map_err(|e| friendly_net_error(e, "https://generativelanguage.googleapis.com"))?;

    if !res.status().is_success() {
        let status   = res.status();
        let err_body: serde_json::Value = res.json().await.unwrap_or_default();
        let msg = err_body["error"]["message"].as_str().unwrap_or("unknown error");
        return Err(anyhow::anyhow!("❌ Google Gemini API error ({}): {}", status, msg));
    }

    let json: serde_json::Value = res.json().await
        .context("Gemini returned non-JSON")?;

    json.get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("text"))
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Unexpected Gemini response: {}", json))
}
