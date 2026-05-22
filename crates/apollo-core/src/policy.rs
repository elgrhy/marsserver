//! Governance & Policy Enforcement Layer.
//!
//! Each tenant can have a `TenantPolicy` that constrains:
//! - Which agents they may run
//! - How many concurrent instances they may have
//! - CPU/memory per agent
//! - Allowed tools (enforced at the spawn-time env level)
//! - Data residency (region constraint)
//! - Per-tenant rate limits
//! - Daily token budget
//! - Model usage rules
//!
//! Policies are stored in `base_dir/policies/{tenant_id}.json` and evaluated
//! by `PolicyEngine::check_*` methods before every agent operation.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Data types ────────────────────────────────────────────────────────────────

/// Per-model governance rules that can be attached to a tenant policy.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelPolicy {
    /// If set, only these model IDs may be used by this tenant.
    pub allowed_models:            Option<Vec<String>>,
    /// Default model when the agent doesn't specify one.
    pub preferred_model:           Option<String>,
    /// Reject any single LLM call whose estimated cost exceeds this.
    pub max_cost_per_request_usd:  Option<f64>,
    /// If true, only local/on-prem models are permitted.
    pub require_local:             bool,
}

/// Full per-tenant policy record.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TenantPolicy {
    pub tenant_id: String,

    // ── Capacity constraints ──────────────────────────────────────────────────
    /// Maximum concurrent agent instances for this tenant.
    pub max_instances:      Option<u32>,
    /// Per-instance CPU cap (fraction, e.g. 0.5 = 50% of one core).
    pub max_cpu_pct:        Option<f32>,
    /// Per-instance memory cap in MB.
    pub max_memory_mb:      Option<u64>,

    // ── Agent whitelist / blacklist ───────────────────────────────────────────
    /// If set, only agents in this list may be started.
    pub allowed_agents:     Option<Vec<String>>,
    /// Agents in this list are always blocked.
    pub blocked_agents:     Vec<String>,

    // ── Tool governance ───────────────────────────────────────────────────────
    /// If set, only these tool names (as declared in agent.yaml) are allowed.
    pub allowed_tools:      Option<Vec<String>>,
    /// These tool names are always blocked.
    pub blocked_tools:      Vec<String>,

    // ── Data residency ────────────────────────────────────────────────────────
    /// If set, the tenant's agents may only run in this region.
    pub data_residency:     Option<String>,

    // ── Rate & quota ──────────────────────────────────────────────────────────
    /// Tenant-level request rate cap (RPS). Overrides the node default.
    pub rate_limit_rps:     Option<u32>,
    /// Total LLM tokens allowed per 24-hour rolling window.
    pub max_tokens_per_day: Option<u64>,

    // ── Model governance ─────────────────────────────────────────────────────
    pub model_policy:       ModelPolicy,

    // ── Safety guardrails ─────────────────────────────────────────────────────
    /// Inject these env vars into every agent process (overrides agent.yaml).
    pub forced_env:         HashMap<String, String>,
    /// Block these env keys from being injected (even from secrets).
    pub blocked_env_keys:   Vec<String>,

    // ── Compliance ───────────────────────────────────────────────────────────
    /// Audit every start/stop event (always true for regulated tenants).
    pub require_audit:      bool,
    /// Maximum run time per agent instance in seconds.
    pub max_run_secs:       Option<u64>,

    pub created_at: u64,
    pub updated_at: u64,
}

impl Default for TenantPolicy {
    fn default() -> Self {
        let now = now_unix();
        TenantPolicy {
            tenant_id:        String::new(),
            max_instances:    None,
            max_cpu_pct:      None,
            max_memory_mb:    None,
            allowed_agents:   None,
            blocked_agents:   vec![],
            allowed_tools:    None,
            blocked_tools:    vec![],
            data_residency:   None,
            rate_limit_rps:   None,
            max_tokens_per_day: None,
            model_policy:     ModelPolicy::default(),
            forced_env:       HashMap::new(),
            blocked_env_keys: vec![],
            require_audit:    false,
            max_run_secs:     None,
            created_at:       now,
            updated_at:       now,
        }
    }
}

/// Result of a policy check.
#[derive(Debug, Clone)]
pub enum PolicyDecision {
    Allow,
    Deny { reason: String, code: PolicyViolation },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyViolation {
    AgentBlocked,
    AgentNotAllowed,
    RegionMismatch,
    CapacityExceeded,
    ToolBlocked,
    TokenBudgetExceeded,
    ModelNotAllowed,
    RunTimeLimitExceeded,
    Custom(String),
}

// ── Storage ───────────────────────────────────────────────────────────────────

fn policy_path(base_dir: &Path, tenant_id: &str) -> PathBuf {
    base_dir.join("policies").join(format!("{}.json", sanitize(tenant_id)))
}

/// Load a tenant's policy. Returns a permissive default if none is set.
pub fn load_policy(base_dir: &Path, tenant_id: &str) -> TenantPolicy {
    let path = policy_path(base_dir, tenant_id);
    if !path.exists() {
        return TenantPolicy { tenant_id: tenant_id.to_string(), ..Default::default() };
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| TenantPolicy { tenant_id: tenant_id.to_string(), ..Default::default() })
}

/// Persist a tenant policy.
pub fn save_policy(base_dir: &Path, policy: &TenantPolicy) -> Result<()> {
    let path = policy_path(base_dir, &policy.tenant_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create policies dir")?;
    }
    fs::write(path, serde_json::to_string_pretty(policy)?)?;
    Ok(())
}

/// Delete a tenant policy (resets to permissive default).
pub fn delete_policy(base_dir: &Path, tenant_id: &str) -> Result<()> {
    let path = policy_path(base_dir, tenant_id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// List all tenants that have an explicit policy.
pub fn list_policy_tenants(base_dir: &Path) -> Vec<String> {
    let dir = base_dir.join("policies");
    if !dir.exists() { return vec![]; }
    fs::read_dir(&dir)
        .ok()
        .map(|entries| {
            entries.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().to_string();
                    name.strip_suffix(".json").map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── Policy evaluation ─────────────────────────────────────────────────────────

/// Central policy engine.
pub struct PolicyEngine {
    pub base_dir: PathBuf,
}

impl PolicyEngine {
    pub fn new(base_dir: PathBuf) -> Self { Self { base_dir } }

    /// Check whether a tenant may start a given agent in a given region.
    pub fn check_run(
        &self,
        tenant_id: &str,
        agent_name: &str,
        node_region: &str,
        current_instances: u32,
    ) -> PolicyDecision {
        let p = load_policy(&self.base_dir, tenant_id);

        // Agent block list
        if p.blocked_agents.contains(&agent_name.to_string()) {
            return PolicyDecision::Deny {
                reason: format!("Agent '{}' is blocked by tenant policy", agent_name),
                code: PolicyViolation::AgentBlocked,
            };
        }

        // Agent allow list
        if let Some(ref allow) = p.allowed_agents {
            if !allow.contains(&agent_name.to_string()) {
                return PolicyDecision::Deny {
                    reason: format!("Agent '{}' is not in tenant's allowed_agents list", agent_name),
                    code: PolicyViolation::AgentNotAllowed,
                };
            }
        }

        // Data residency
        if let Some(ref required_region) = p.data_residency {
            if required_region != node_region && required_region != "any" {
                return PolicyDecision::Deny {
                    reason: format!(
                        "Tenant data residency policy requires region '{}', node is '{}'",
                        required_region, node_region
                    ),
                    code: PolicyViolation::RegionMismatch,
                };
            }
        }

        // Instance capacity
        if let Some(max) = p.max_instances {
            if current_instances >= max {
                return PolicyDecision::Deny {
                    reason: format!(
                        "Tenant capacity limit reached ({}/{})", current_instances, max
                    ),
                    code: PolicyViolation::CapacityExceeded,
                };
            }
        }

        PolicyDecision::Allow
    }

    /// Check whether a tool call is permitted for a tenant.
    pub fn check_tool(&self, tenant_id: &str, tool_name: &str) -> PolicyDecision {
        let p = load_policy(&self.base_dir, tenant_id);

        if p.blocked_tools.contains(&tool_name.to_string()) {
            return PolicyDecision::Deny {
                reason: format!("Tool '{}' is blocked by tenant policy", tool_name),
                code: PolicyViolation::ToolBlocked,
            };
        }

        if let Some(ref allow) = p.allowed_tools {
            if !allow.contains(&tool_name.to_string()) {
                return PolicyDecision::Deny {
                    reason: format!("Tool '{}' is not in tenant's allowed_tools list", tool_name),
                    code: PolicyViolation::ToolBlocked,
                };
            }
        }

        PolicyDecision::Allow
    }

    /// Check whether a model is allowed for a tenant.
    pub fn check_model(&self, tenant_id: &str, model_id: &str) -> PolicyDecision {
        let p = load_policy(&self.base_dir, tenant_id);
        let mp = &p.model_policy;

        if let Some(ref allowed) = mp.allowed_models {
            if !allowed.contains(&model_id.to_string()) {
                return PolicyDecision::Deny {
                    reason: format!("Model '{}' is not allowed by tenant policy", model_id),
                    code: PolicyViolation::ModelNotAllowed,
                };
            }
        }

        PolicyDecision::Allow
    }

    /// Get the effective env vars for a tenant (forced + blocked).
    /// Returns: (vars_to_inject, keys_to_block)
    pub fn effective_env(
        &self,
        tenant_id: &str,
    ) -> (HashMap<String, String>, Vec<String>) {
        let p = load_policy(&self.base_dir, tenant_id);
        (p.forced_env, p.blocked_env_keys)
    }

    /// Get the effective rate limit for a tenant (tenant override or None).
    pub fn effective_rate_limit(&self, tenant_id: &str) -> Option<u32> {
        load_policy(&self.base_dir, tenant_id).rate_limit_rps
    }
}

// ── Compliance report ─────────────────────────────────────────────────────────

/// A compliance summary report for a tenant.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ComplianceReport {
    pub tenant_id:         String,
    pub policy_active:     bool,
    pub data_residency:    Option<String>,
    pub agent_restrictions: bool,
    pub tool_restrictions:  bool,
    pub model_restrictions: bool,
    pub require_audit:      bool,
    pub max_instances:      Option<u32>,
    pub max_tokens_per_day: Option<u64>,
    pub generated_at:       u64,
}

pub fn compliance_report(base_dir: &Path, tenant_id: &str) -> ComplianceReport {
    let p = load_policy(base_dir, tenant_id);
    ComplianceReport {
        tenant_id:          tenant_id.to_string(),
        policy_active:      policy_path(base_dir, tenant_id).exists(),
        data_residency:     p.data_residency,
        agent_restrictions: p.allowed_agents.is_some() || !p.blocked_agents.is_empty(),
        tool_restrictions:  p.allowed_tools.is_some() || !p.blocked_tools.is_empty(),
        model_restrictions: p.model_policy.allowed_models.is_some(),
        require_audit:      p.require_audit,
        max_instances:      p.max_instances,
        max_tokens_per_day: p.max_tokens_per_day,
        generated_at:       now_unix(),
    }
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
