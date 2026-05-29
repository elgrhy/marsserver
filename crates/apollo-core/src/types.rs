//! Shared data types used across the entire APOLLO workspace.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── Node Configuration ───────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeConfig {
    pub node_id:     String,
    pub provider_id: String,
    pub secret_keys: Vec<String>,
    pub profile:     NodeProfile,
    pub network:     NodeNetworkPolicy,
    #[serde(default = "default_region")]
    pub region:      String,
    pub jwt_secret:  Option<String>,
}

fn default_region() -> String { "default".to_string() }

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NodeNetworkPolicy {
    pub rate_limit_rps: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct NodeProfile {
    pub os:       String,
    pub arch:     String,
    pub ram_gb:   u32,
    pub runtimes: Vec<String>,
    pub llm:      Option<NodeLLMProfile>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NodeLLMProfile {
    pub provider: String,
    pub model:    String,
    pub endpoint: String,
}

// ── Tenant & Resource Plans ──────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TenantRecord {
    pub id:            String,
    pub plan:          ResourcePlan,
    pub active_agents: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResourcePlan {
    pub max_agents:   u32,
    pub cpu_limit:    f32,
    pub memory_limit: String,
}

// ── Agent Records ────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRecord {
    pub id:          String,
    pub spec:        AgentSpec,
    pub checksum:    String,
    pub created_at:  u64,
    pub prev_version: Option<String>,   // last version before this update
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentInstance {
    pub id:         String,
    pub agent_id:   String,
    pub tenant_id:  String,
    pub status:     String,
    pub pid:        Option<u32>,
    pub port:       Option<u16>,
    pub stats:      ExecutionStats,
    pub created_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ExecutionStats {
    pub cpu_usage_pct: f32,
    pub memory_mb:     u64,
    pub uptime_secs:   u64,
    pub restart_count: u32,
    pub last_restart:  u64,
    pub is_failed:     bool,
}

// ── Control Plane Protocol ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RemoteCommand {
    pub id:     String,
    pub action: String,
    pub agent:  String,
    pub tenant: String,
    pub params: Option<HashMap<String, String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandResult {
    pub command_id: String,
    pub status:     String,
    pub message:    String,
}

// ── Agent Specification (agent.yaml) ─────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentSpec {
    pub name:           String,
    pub version:        String,
    pub runtime:        AgentRuntimeConfig,
    pub llm:            AgentLLMConfig,
    pub capabilities:   Vec<String>,
    pub triggers:       Vec<String>,
    pub resources:      AgentResourceLimits,
    pub permissions:    AgentPermissionConfig,
    pub compatibility:  AgentCompatibility,
    pub restart_policy: Option<RestartPolicy>,
    #[serde(default)]
    pub volumes:        Vec<VolumeSpec>,
}

/// A named persistent volume mounted into the tenant workspace.
/// Apollo creates `base_dir/volumes/{tenant_id}/{agent}/{name}/` and exposes
/// it as `APOLLO_VOLUME_{NAME_UPPER}=<abs_path>` in the agent's environment.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VolumeSpec {
    pub name: String,
    /// Advisory size hint (e.g. "1gb"). Quota enforcement is provider-side.
    #[serde(default)]
    pub size: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RestartPolicy {
    pub max_restarts: u32,
    pub window_secs:  u32,
}

/// Runtime configuration — accepts both `type:` and `kind:` YAML keys.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentRuntimeConfig {
    /// Runtime type: python3 | node | go | deno | bun | ruby | gx | rust | shell | <custom>
    #[serde(rename = "type", alias = "kind")]
    pub kind:    String,
    /// Entry point relative to the agent package directory.
    pub entry:   String,
    /// Optional extra env vars injected into the agent process.
    pub env:     Option<HashMap<String, String>>,
    /// Override launch command. Use `{entry}` as placeholder for the resolved entry path.
    /// Example: "gx run {entry}" or "deno run --allow-net {entry}"
    pub command: Option<String>,
    /// Optional auto-install URLs for missing runtimes. Apollo downloads and installs
    /// to `base_dir/runtimes/{kind}/` if the runtime binary is not found on the node.
    pub install: Option<RuntimeInstallConfig>,
}

/// Per-platform download URLs for auto-installing a runtime binary.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct RuntimeInstallConfig {
    pub linux:   Option<String>,
    pub macos:   Option<String>,
    pub windows: Option<String>,
    /// Fallback: a cross-platform install script path (run via shell/bash).
    pub script:  Option<String>,
    /// Expected SHA-256 hex digest of the downloaded archive.
    /// When set, the download is rejected if the digest does not match.
    pub sha256:  Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentLLMConfig {
    pub required: bool,
    pub provider: String,
    pub fallback: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentResourceLimits {
    pub cpu:     f32,
    pub memory:  String,
    pub timeout: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentPermissionConfig {
    pub network:    String,
    pub filesystem: String,
    pub processes:  String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentCompatibility {
    pub os:   Vec<String>,
    pub arch: Vec<String>,
}

// ── Event Spine ───────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ApolloEvent {
    pub timestamp:      u64,
    pub node_id:        String,
    pub level:          String,
    pub category:       String,
    pub action:         String,
    pub message:        String,
    pub correlation_id: Option<String>,
    pub metadata:       Option<HashMap<String, String>>,
}

/// Append an event to `{base_dir}/events.jsonl`, rotating at 100 MB.
/// Keeps up to 3 generations: events.jsonl → events.jsonl.1 → events.jsonl.2
pub fn log_event_to(base_dir: &std::path::Path, event: ApolloEvent) {
    let log_path = base_dir.join("events.jsonl");
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Rotate if the file exceeds 100 MB
    if let Ok(meta) = std::fs::metadata(&log_path) {
        if meta.len() > 100 * 1024 * 1024 {
            for gen in (1u8..3).rev() {
                let from = base_dir.join(format!("events.jsonl.{}", gen));
                let to   = base_dir.join(format!("events.jsonl.{}", gen + 1));
                let _ = std::fs::rename(&from, &to);
            }
            let _ = std::fs::rename(&log_path, base_dir.join("events.jsonl.1"));
        }
    }
    if let Ok(json) = serde_json::to_string(&event) {
        use std::io::Write;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true).append(true).open(&log_path)
        {
            let _ = writeln!(file, "{}", json);
        }
    }
}

/// Convenience wrapper — resolves base_dir from `APOLLO_BASE_DIR` env var or `.apollo/`.
pub fn log_event(event: ApolloEvent) {
    let base_dir = std::env::var("APOLLO_BASE_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from(".apollo"));
    log_event_to(&base_dir, event);
}
