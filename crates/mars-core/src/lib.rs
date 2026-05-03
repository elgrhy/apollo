//! mars-core — shared primitives for the MARS workspace.

pub mod types;
pub mod agents;
pub mod detect;

pub use types::*;
pub use detect::detect_node_capabilities;
pub use agents::{load_agent_registry, save_agent_registry, register_agent_package, now_unix};

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
