//! mars-core — shared primitives for the MARS workspace.
//!
//! All other crates depend on this one. It provides:
//! - All data types (Profile, LLMConfig, CapabilityMap, …)
//! - LLM routing (call_llm + per-provider functions)
//! - HTTP client singleton
//! - Config helpers (.env read/write)
//! - License check
//! - mars_print! macro
//! - IOT_MODE global flag

pub mod types;
pub mod net;
pub mod config;
pub mod license;
pub mod llm;
pub mod agents;

pub use types::*;
pub use net::{CLIENT, friendly_net_error};
pub use config::{enable_bootstrap, load_llm_config_from_env, load_extension_protocol};
pub use license::check_license;
pub use types::LicenseCache;
pub use llm::call_llm;
pub use agents::{load_agent_registry, save_agent_registry, register_agent, now_unix};

use std::sync::atomic::AtomicBool;

/// Global IoT mode flag — set via `--iot`. Suppresses all interactive output.
pub static IOT_MODE: AtomicBool = AtomicBool::new(false);

/// Conditional println — silent in IoT mode.
#[macro_export]
macro_rules! mars_print {
    ($($arg:tt)*) => {
        if !$crate::IOT_MODE.load(std::sync::atomic::Ordering::Relaxed) {
            println!($($arg)*);
        }
    };
}
