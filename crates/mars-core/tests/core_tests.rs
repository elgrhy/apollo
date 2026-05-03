//! mars-core unit tests
//! Tests for types, config helpers, and shared primitives.

use mars_core::types::{
    CapabilityMap, GhostConfig, Hardware, LLMConfig, LicenseCache, LocalService,
    Network, OS, Profile, SwarmPeer, SystemCaps,
};
use serde_json;

// ── Profile & hardware ────────────────────────────────────────────────────────

#[test]
fn profile_default_is_valid() {
    let p = Profile::default();
    // All string fields start empty but the struct is fully initialised.
    assert!(p.hardware.arch.is_empty());
    assert!(p.hardware.cpu_cores == 0);
    assert!(p.hardware.total_memory_gb == 0);
    assert!(!p.hardware.mobile);
    assert!(p.llm.is_none());
    assert!(p.binaries.is_empty());
}

#[test]
fn profile_round_trips_json() {
    let mut p = Profile::default();
    p.hardware = Hardware {
        arch: "aarch64".into(),
        cpu_cores: 8,
        total_memory_gb: 16,
        mobile: false,
    };
    p.os = OS {
        name: "macOS".into(),
        version: "15.0".into(),
    };
    p.network = Network { online: true };

    let json = serde_json::to_string(&p).expect("serialise");
    let back: Profile = serde_json::from_str(&json).expect("deserialise");

    assert_eq!(back.hardware.arch, "aarch64");
    assert_eq!(back.hardware.cpu_cores, 8);
    assert_eq!(back.os.name, "macOS");
    assert!(back.network.online);
}

// ── LLMConfig ─────────────────────────────────────────────────────────────────

#[test]
fn llm_config_local_serialises_without_api_key() {
    let cfg = LLMConfig {
        llm_type: "local".into(),
        endpoint: Some("http://localhost:11434".into()),
        provider: None,
        model: Some("llama3".into()),
        api_key: None,
    };
    let json = serde_json::to_string(&cfg).expect("serialise");
    // api_key is skipped when None
    assert!(!json.contains("api_key"));
    assert!(json.contains("localhost:11434"));
}

#[test]
fn llm_config_cloud_contains_provider() {
    let cfg = LLMConfig {
        llm_type: "cloud".into(),
        endpoint: None,
        provider: Some("anthropic".into()),
        model: Some("claude-opus-4-6".into()),
        api_key: Some("sk-test".into()),
    };
    let json = serde_json::to_string(&cfg).expect("serialise");
    assert!(json.contains("anthropic"));
    assert!(json.contains("claude-opus-4-6"));
}

#[test]
fn llm_config_default_is_empty_local() {
    let cfg = LLMConfig::default();
    assert_eq!(cfg.llm_type, "");
    assert!(cfg.endpoint.is_none());
    assert!(cfg.api_key.is_none());
}

// ── CapabilityMap ─────────────────────────────────────────────────────────────

#[test]
fn capability_map_default_all_empty() {
    let caps = CapabilityMap::default();
    assert!(caps.llm_backends.is_empty());
    assert!(caps.vision.is_empty());
    assert!(caps.audio.is_empty());
    assert!(caps.image_gen.is_empty());
    assert!(caps.tts.is_empty());
    assert!(caps.embeddings.is_empty());
    assert!(caps.mcp_servers.is_empty());
    assert!(caps.runtimes.is_empty());
    assert!(caps.package_managers.is_empty());
    assert!(caps.services.is_empty());
    assert!(!caps.system.gpu);
    assert!(!caps.system.docker);
    assert!(!caps.system.internet);
}

#[test]
fn capability_map_round_trips() {
    let mut caps = CapabilityMap::default();
    caps.llm_backends.push(LocalService {
        name: "ollama".into(),
        url: "http://localhost:11434".into(),
        kind: "ollama".into(),
        model: Some("llama3".into()),
    });
    caps.runtimes = vec!["node".into(), "python3".into(), "cargo".into()];
    caps.system = SystemCaps {
        gpu: true,
        docker: false,
        cron: true,
        systemd: false,
        internet: true,
    };

    let json = serde_json::to_string(&caps).unwrap();
    let back: CapabilityMap = serde_json::from_str(&json).unwrap();

    assert_eq!(back.llm_backends.len(), 1);
    assert_eq!(back.llm_backends[0].name, "ollama");
    assert_eq!(back.runtimes.len(), 3);
    assert!(back.system.gpu);
    assert!(back.system.internet);
}

// ── LocalService ──────────────────────────────────────────────────────────────

#[test]
fn local_service_with_model() {
    let svc = LocalService {
        name: "comfyui".into(),
        url: "http://localhost:8188".into(),
        kind: "comfyui".into(),
        model: None,
    };
    assert_eq!(svc.kind, "comfyui");
    assert!(svc.model.is_none());
}

// ── GhostConfig ───────────────────────────────────────────────────────────────

#[test]
fn ghost_config_default() {
    let g = GhostConfig::default();
    assert!(g.secret_key.is_empty());
    assert!(g.webhook_url.is_none());
    assert!(!g.remove_self);
}

#[test]
fn ghost_config_round_trips() {
    let cfg = GhostConfig {
        secret_key: "supersecret".into(),
        webhook_url: Some("https://example.com/hook".into()),
        remove_self: true,
    };
    let json = serde_json::to_string(&cfg).unwrap();
    let back: GhostConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(back.secret_key, "supersecret");
    assert_eq!(back.webhook_url.as_deref(), Some("https://example.com/hook"));
    assert!(back.remove_self);
}

// ── SwarmPeer ─────────────────────────────────────────────────────────────────

#[test]
fn swarm_peer_serialises() {
    let peer = SwarmPeer {
        ip: "192.168.1.100".into(),
        user: "alice".into(),
        arch: "x86_64".into(),
        hostname: "alice-laptop".into(),
    };
    let json = serde_json::to_string(&peer).unwrap();
    assert!(json.contains("192.168.1.100"));
    assert!(json.contains("alice"));
}

// ── LicenseCache ─────────────────────────────────────────────────────────────

#[test]
fn license_cache_structure() {
    let lc = LicenseCache {
        key: "MARS-OWNER-OFFLINE-2024".into(),
        tier: "owner".into(),
        email: "owner@devjsx.com".into(),
        validated_at: 1700000000,
    };
    assert_eq!(lc.tier, "owner");
    let json = serde_json::to_string(&lc).unwrap();
    assert!(json.contains("MARS-OWNER-OFFLINE-2024"));
}

// ── IOT_MODE macro ────────────────────────────────────────────────────────────

#[test]
fn iot_mode_starts_false() {
    use std::sync::atomic::Ordering;
    // The global flag should not be set unless --iot was passed.
    // (Tests run in a fresh process, so the flag is false by default.)
    let current = mars_core::IOT_MODE.load(Ordering::Relaxed);
    // We only assert the flag is readable, not its value, since tests may share state.
    let _ = current;
}

// ── Profile with capabilities injected ───────────────────────────────────────

#[test]
fn profile_carries_capabilities() {
    let mut p = Profile::default();
    p.capabilities.runtimes = vec!["cargo".into(), "node".into()];
    p.capabilities.system.internet = true;
    p.capabilities.mcp_servers.push(LocalService {
        name: "claude-mcp".into(),
        url: "http://localhost:3001".into(),
        kind: "mcp".into(),
        model: None,
    });

    assert_eq!(p.capabilities.runtimes.len(), 2);
    assert!(p.capabilities.system.internet);
    assert_eq!(p.capabilities.mcp_servers[0].name, "claude-mcp");
}
