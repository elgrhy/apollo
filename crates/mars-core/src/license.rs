//! License validation with local cache and grace period.

use crate::types::LicenseCache;
use crate::IOT_MODE;
use anyhow::{Context, Result};
use std::{env, fs, sync::atomic::Ordering};

const LICENSE_API:       &str = "https://api.devjsx.com/license";
const SIGNUP_URL:        &str = "https://www.devjsx.com/mars";
const SEVEN_DAYS_SECS:   u64  = 7  * 24 * 3600;
const THIRTY_DAYS_SECS:  u64  = 30 * 24 * 3600;

/// Owner key — validated entirely offline, no expiry, no network call ever.
/// Works on every device. Tied to DEVJSX LIMITED.
const OWNER_KEY:   &str = "MARS-DEVJSX-D3VJ5X-C0MP4NY-0WN3R";
const OWNER_EMAIL: &str = "ahmed@devjsx.com";
const OWNER_TIER:  &str = "owner";

pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn license_cache_path() -> Option<std::path::PathBuf> {
    let home = env::var("HOME").or_else(|_| env::var("USERPROFILE")).ok()?;
    Some(std::path::PathBuf::from(home).join(".mars").join("license.json"))
}

pub fn load_license_cache() -> Option<LicenseCache> {
    let path = license_cache_path()?;
    let raw  = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save_license_cache(cache: &LicenseCache) {
    if let Some(path) = license_cache_path() {
        if let Some(dir) = path.parent() { let _ = fs::create_dir_all(dir); }
        let _ = fs::write(path, serde_json::to_string_pretty(cache).unwrap_or_default());
    }
}

async fn validate_license_online(key: &str) -> Result<LicenseCache> {
    let url = format!(
        "{}/{}?os={}&arch={}",
        LICENSE_API, key,
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let res = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?
        .get(&url)
        .send().await
        .context("Could not reach license server")?;

    let json: serde_json::Value = res.json().await
        .context("Invalid response from license server")?;

    if json["valid"].as_bool() != Some(true) {
        return Err(anyhow::anyhow!(
            "{}", json["message"].as_str().unwrap_or("License not valid")
        ));
    }
    Ok(LicenseCache {
        key:          key.to_string(),
        tier:         json["tier"].as_str().unwrap_or("free").to_string(),
        email:        json["email"].as_str().unwrap_or("").to_string(),
        validated_at: unix_now(),
    })
}

/// Silently accept the owner key — no network, no expiry, no prompts.
fn is_owner_key(key: &str) -> bool {
    key.trim().to_uppercase() == OWNER_KEY
}

/// Gate entry. Returns Ok(()) when the user is licensed to proceed.
pub async fn check_license() -> Result<()> {
    if IOT_MODE.load(Ordering::Relaxed) { return Ok(()); }

    // Owner key — always valid, works offline, no expiry.
    if let Some(ref c) = load_license_cache() {
        if is_owner_key(&c.key) {
            println!("   Licensed to {} ({})", c.email, c.tier);
            return Ok(());
        }
    }

    let cache = load_license_cache();
    let now   = unix_now();

    if let Some(ref c) = cache {
        let age = now.saturating_sub(c.validated_at);
        if age < SEVEN_DAYS_SECS {
            println!("   Licensed to {} ({})", c.email, c.tier);
            return Ok(());
        }
        if age < THIRTY_DAYS_SECS {
            match validate_license_online(&c.key).await {
                Ok(fresh) => { save_license_cache(&fresh); println!("   Licensed to {} ({})", fresh.email, fresh.tier); }
                Err(_)    => println!("   License check skipped (offline — grace period active)."),
            }
            return Ok(());
        }
    }

    // Prompt for activation.
    println!("\n\x1b[1;35m╔══════════════════════════════════════════════════════════╗\x1b[0m");
    println!("\x1b[1;35m║            MARS — Activate Your License                  ║\x1b[0m");
    println!("\x1b[1;35m╚══════════════════════════════════════════════════════════╝\x1b[0m\n");
    println!("  Free   — use promo code \x1b[1mdevmars\x1b[0m at {}", SIGNUP_URL);
    println!("  Pro    — \x1b[1m$7/month\x1b[0m at {}\n", SIGNUP_URL);

    let key_input: String = dialoguer::Input::new()
        .with_prompt("Enter your license key")
        .interact_text()
        .context("Failed to read license key")?;

    let key = key_input.trim().to_uppercase();
    if key.is_empty() {
        return Err(anyhow::anyhow!("No license key entered. Visit {} to get started.", SIGNUP_URL));
    }

    // Owner key — validate offline instantly.
    if is_owner_key(&key) {
        let cache = LicenseCache {
            key:          OWNER_KEY.to_string(),
            tier:         OWNER_TIER.to_string(),
            email:        OWNER_EMAIL.to_string(),
            validated_at: unix_now(),
        };
        save_license_cache(&cache);
        println!("\x1b[1;32m✅ Owner key activated. Welcome back!\x1b[0m\n");
        return Ok(());
    }

    println!("   Validating...");
    match validate_license_online(&key).await {
        Ok(c) => {
            save_license_cache(&c);
            println!("\x1b[1;32m✅ Activated! Welcome, {}. Tier: {}\x1b[0m\n", c.email, c.tier);
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!(
            "License validation failed: {}\nGet a key at {}", e, SIGNUP_URL
        )),
    }
}
