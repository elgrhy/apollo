//! Outbound webhook dispatch for agent lifecycle events.
//!
//! When a `webhook_url` is configured, Apollo POSTs a signed JSON payload on
//! every agent start, stop, crash, and capacity warning. Providers use this to
//! drive billing, routing updates, and observability dashboards.
//!
//! Signature: `X-Apollo-Signature: sha256=<hex(HMAC-SHA256(body, secret))>`
//! Retry: up to 3 attempts with 1s/2s/4s backoff.

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct WebhookConfig {
    pub url:    String,
    pub secret: Option<String>,
}

impl WebhookConfig {
    pub fn new(url: String, secret: Option<String>) -> Self {
        Self { url, secret }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WebhookPayload {
    pub event:      String,
    pub timestamp:  u64,
    pub node_id:    String,
    pub tenant_id:  String,
    pub agent_id:   String,
    pub status:     String,
    pub port:       Option<u16>,
    pub pid:        Option<u32>,
    pub message:    Option<String>,
}

impl WebhookPayload {
    pub fn agent_start(node_id: &str, tenant_id: &str, agent_id: &str, port: u16, pid: u32) -> Self {
        Self {
            event:     "AGENT_START".into(),
            timestamp: now_unix(),
            node_id:   node_id.into(),
            tenant_id: tenant_id.into(),
            agent_id:  agent_id.into(),
            status:    "running".into(),
            port:      Some(port),
            pid:       Some(pid),
            message:   None,
        }
    }

    pub fn agent_stop(node_id: &str, tenant_id: &str, agent_id: &str) -> Self {
        Self {
            event:     "AGENT_STOP".into(),
            timestamp: now_unix(),
            node_id:   node_id.into(),
            tenant_id: tenant_id.into(),
            agent_id:  agent_id.into(),
            status:    "stopped".into(),
            port:      None,
            pid:       None,
            message:   None,
        }
    }

    pub fn capacity_warning(node_id: &str, active: usize, max: usize) -> Self {
        Self {
            event:     "CAPACITY_WARNING".into(),
            timestamp: now_unix(),
            node_id:   node_id.into(),
            tenant_id: "system".into(),
            agent_id:  "system".into(),
            status:    "warning".into(),
            port:      None,
            pid:       None,
            message:   Some(format!("Node at {}/{} capacity", active, max)),
        }
    }

    pub fn scale_needed(active: usize, max: usize, region: &str) -> Self {
        Self {
            event:     "SCALE_NEEDED".into(),
            timestamp: now_unix(),
            node_id:   "hub".into(),
            tenant_id: "system".into(),
            agent_id:  "system".into(),
            status:    "alert".into(),
            port:      None,
            pid:       None,
            message:   Some(format!("Fleet region '{}' at {}/{} capacity", region, active, max)),
        }
    }
}

/// Fire-and-forget webhook dispatch. Retries 3× with exponential backoff.
/// Spawns a tokio task so the caller is never blocked.
pub fn fire(cfg: &WebhookConfig, payload: WebhookPayload) {
    let url    = cfg.url.clone();
    let secret = cfg.secret.clone();
    tokio::spawn(async move {
        let body = match serde_json::to_string(&payload) {
            Ok(b) => b,
            Err(_) => return,
        };
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();

        let mut delay = 1u64;
        for attempt in 0..3 {
            if attempt > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(delay)).await;
                delay *= 2;
            }
            let mut req = client.post(&url)
                .header("Content-Type", "application/json")
                .header("X-Apollo-Event", &payload.event)
                .body(body.clone());

            if let Some(ref s) = secret {
                let sig = hmac_sha256(s, body.as_bytes());
                req = req.header("X-Apollo-Signature", format!("sha256={}", sig));
            }

            if let Ok(resp) = req.send().await {
                if resp.status().is_success() {
                    return;
                }
            }
        }
    });
}

fn hmac_sha256(secret: &str, body: &[u8]) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC key");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
