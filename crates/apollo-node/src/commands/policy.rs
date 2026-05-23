//! CLI commands for per-tenant governance and policy.

use anyhow::Result;
use clap::Subcommand;
use apollo_core::policy::TenantPolicy;
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum PolicyCmd {
    /// Show the current policy for a tenant
    Get { tenant: String },
    /// Set policy for a tenant (from JSON file or flags)
    Set {
        tenant: String,
        /// Load policy from a JSON file
        #[arg(long)]
        file: Option<std::path::PathBuf>,
        /// Maximum running instances for this tenant
        #[arg(long)]
        max_instances: Option<u32>,
        /// Comma-separated allowed agent IDs
        #[arg(long)]
        allowed_agents: Option<String>,
        /// Comma-separated blocked agent IDs
        #[arg(long)]
        blocked_agents: Option<String>,
        /// Data residency region (e.g. eu-west-1)
        #[arg(long)]
        data_residency: Option<String>,
        /// Max tokens per day across all models
        #[arg(long)]
        max_tokens_per_day: Option<u64>,
        /// Require full audit logging
        #[arg(long)]
        require_audit: bool,
    },
    /// Delete policy for a tenant (resets to permissive default)
    Delete { tenant: String },
    /// Show compliance report for a tenant
    Compliance { tenant: String },
}

pub fn run(cmd: PolicyCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        PolicyCmd::Get { tenant } => {
            let p = client.policy_get(&tenant)?;
            fmt::section(&format!("Policy — {}", tenant));
            fmt::kv("Max instances",      &opt_str(p.max_instances.map(|v| v.to_string())));
            fmt::kv("Allowed agents",     &opt_list(p.allowed_agents.as_deref()));
            fmt::kv("Blocked agents",     &list_str(&p.blocked_agents));
            fmt::kv("Data residency",     &opt_str(p.data_residency.clone()));
            fmt::kv("Max tokens/day",     &opt_str(p.max_tokens_per_day.map(|v| format!("{}", v))));
            fmt::kv("Require audit",      &p.require_audit.to_string());
            fmt::kv("Blocked tools",      &list_str(&p.blocked_tools));
            fmt::kv("Allowed models",     &opt_list(p.model_policy.allowed_models.as_deref()));
            fmt::kv("Require local",      &p.model_policy.require_local.to_string());
        }

        PolicyCmd::Set {
            tenant, file, max_instances, allowed_agents, blocked_agents,
            data_residency, max_tokens_per_day, require_audit,
        } => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let mut policy: TenantPolicy = if let Some(path) = file {
                let raw = std::fs::read_to_string(&path)?;
                serde_json::from_str(&raw)?
            } else {
                // Build fresh from flags — avoids a GET+PUT double-request
                TenantPolicy {
                    tenant_id:  tenant.clone(),
                    created_at: now,
                    updated_at: now,
                    ..Default::default()
                }
            };

            policy.tenant_id = tenant.clone();
            if let Some(n) = max_instances         { policy.max_instances = Some(n); }
            if let Some(s) = allowed_agents        { policy.allowed_agents = Some(s.split(',').map(|s| s.trim().to_string()).collect()); }
            if let Some(s) = blocked_agents        { policy.blocked_agents = s.split(',').map(|s| s.trim().to_string()).collect(); }
            if let Some(r) = data_residency        { policy.data_residency = Some(r); }
            if let Some(t) = max_tokens_per_day    { policy.max_tokens_per_day = Some(t); }
            if require_audit                        { policy.require_audit = true; }

            client.policy_set(&tenant, &policy)?;
            fmt::ok(&format!("Policy saved for tenant '{}'", tenant));
        }

        PolicyCmd::Delete { tenant } => {
            client.policy_delete(&tenant)?;
            fmt::ok(&format!("Policy deleted for '{}' — now using permissive default", tenant));
        }

        PolicyCmd::Compliance { tenant } => {
            let report = client.policy_compliance(&tenant)?;
            fmt::section(&format!("Compliance Report — {}", tenant));
            fmt::kv("Policy active",       &report.policy_active.to_string());
            fmt::kv("Require audit",       &report.require_audit.to_string());
            fmt::kv("Data residency",      &report.data_residency.as_deref().unwrap_or("—").to_string());
            fmt::kv("Agent restrictions",  &report.agent_restrictions.to_string());
            fmt::kv("Tool restrictions",   &report.tool_restrictions.to_string());
            fmt::kv("Model restrictions",  &report.model_restrictions.to_string());
            fmt::kv("Max instances",       &report.max_instances.map(|n| n.to_string()).unwrap_or_else(|| "—".into()));
            fmt::kv("Max tokens/day",      &report.max_tokens_per_day.map(|n| n.to_string()).unwrap_or_else(|| "—".into()));
            println!();
            fmt::ok("Compliance report generated.");
        }
    }
    Ok(())
}

fn opt_str(v: Option<String>) -> String { v.unwrap_or_else(|| "—".into()) }
fn opt_list(v: Option<&[String]>) -> String {
    v.map(|l| l.join(", ")).unwrap_or_else(|| "—".into())
}
fn list_str(v: &[String]) -> String {
    if v.is_empty() { "—".into() } else { v.join(", ") }
}
