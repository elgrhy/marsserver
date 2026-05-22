//! apollo-core — shared primitives for the APOLLO workspace.
//!
//! v1.2: execution, security, metering, fleet, webhooks
//! v2.0: observability, policy, health intelligence, memory,
//!        model routing, scheduler, orchestration

// ── Execution layer (v1.x) ────────────────────────────────────────────────────
pub mod types;
pub mod agents;
pub mod detect;
pub mod fetch;
pub mod runtime_registry;
pub mod secrets;
pub mod webhook;
pub mod usage;

// ── Platform layer (v2.0) ─────────────────────────────────────────────────────

/// Distributed tracing and observability (step spans, token accounting).
pub mod tracing;

/// Governance and policy enforcement (per-tenant rules, quotas, residency).
pub mod policy;

/// Agent lifecycle intelligence (health scoring, crash detection, restart logic).
pub mod health;

/// Per-tenant per-agent persistent memory with similarity search.
pub mod memory;

/// Model governance and routing (cost-aware, latency-aware, policy-aware).
pub mod model_router;

/// Scheduled and event-driven agent execution (cron, interval, once).
pub mod scheduler;

/// Provider-oriented orchestration: blueprints, agent groups, workflow DAGs.
pub mod orchestration;

#[cfg(test)]
mod tests;

// ── Re-exports ────────────────────────────────────────────────────────────────

pub use types::*;
pub use detect::{detect_node_capabilities, detect_node_capabilities_with_dir};
pub use agents::{load_agent_registry, save_agent_registry, register_agent_package,
                 rollback_agent, remove_agent, now_unix};

use std::sync::atomic::AtomicBool;

/// Global IoT mode flag — set via `--iot`. Suppresses all interactive output.
pub static IOT_MODE: AtomicBool = AtomicBool::new(false);

/// Conditional println — silent in IoT mode.
#[macro_export]
macro_rules! apollo_print {
    ($($arg:tt)*) => {
        if !$crate::IOT_MODE.load(std::sync::atomic::Ordering::Relaxed) {
            println!($($arg)*);
        }
    };
}
