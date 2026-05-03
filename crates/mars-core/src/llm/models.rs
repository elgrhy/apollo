use crate::net::CLIENT;

/// Fetch the live model list from a cloud provider.
/// Returns an empty Vec on any error — callers fall back to static defaults.
pub async fn fetch_available_models(provider: &str, api_key: &str) -> Vec<String> {
    match provider {
        "openai" => {
            let Ok(r) = CLIENT
                .get("https://api.openai.com/v1/models")
                .header("Authorization", format!("Bearer {}", api_key))
                .send().await else { return vec![] };
            let Ok(json) = r.json::<serde_json::Value>().await else { return vec![] };
            let mut models: Vec<String> = json["data"]
                .as_array().unwrap_or(&vec![])
                .iter()
                .filter_map(|m| m["id"].as_str())
                .filter(|id| {
                    id.starts_with("gpt-") || id.starts_with("o1") ||
                    id.starts_with("o3")   || id.starts_with("o4")
                })
                .map(|s| s.to_string())
                .collect();
            models.sort();
            models
        }
        "groq" => {
            let Ok(r) = CLIENT
                .get("https://api.groq.com/openai/v1/models")
                .header("Authorization", format!("Bearer {}", api_key))
                .send().await else { return vec![] };
            let Ok(json) = r.json::<serde_json::Value>().await else { return vec![] };
            let mut models: Vec<String> = json["data"]
                .as_array().unwrap_or(&vec![])
                .iter()
                .filter_map(|m| m["id"].as_str())
                .map(|s| s.to_string())
                .collect();
            models.sort();
            models
        }
        "google" => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models?key={}", api_key
            );
            let Ok(r) = CLIENT.get(&url).send().await else { return vec![] };
            let Ok(json) = r.json::<serde_json::Value>().await else { return vec![] };
            let mut models: Vec<String> = json["models"]
                .as_array().unwrap_or(&vec![])
                .iter()
                .filter(|m| {
                    m["supportedGenerationMethods"]
                        .as_array()
                        .map(|a| a.iter().any(|v| v.as_str() == Some("generateContent")))
                        .unwrap_or(false)
                })
                .filter_map(|m| m["name"].as_str())
                .map(|s| s.trim_start_matches("models/").to_string())
                .collect();
            models.sort();
            models
        }
        "anthropic" => vec![
            "claude-opus-4-6".into(),
            "claude-sonnet-4-6".into(),
            "claude-haiku-4-5-20251001".into(),
        ],
        _ => vec![],
    }
}
