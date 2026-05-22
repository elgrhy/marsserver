//! CLI commands for usage metering and billing.

use anyhow::Result;
use clap::Subcommand;
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum UsageCmd {
    /// Show usage for all tenants
    List,
    /// Show usage for one tenant
    Get { tenant: String },
    /// Reset usage counters (billing cycle reset)
    Reset { tenant: String },
}

pub fn run(cmd: UsageCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        UsageCmd::List => {
            let records = client.usage_all()?;
            if records.is_empty() {
                fmt::info("No usage data yet.");
                return Ok(());
            }
            fmt::section("Usage — All Tenants");
            fmt::print_table(
                &["TENANT", "CPU-SECS", "MEM GB-SECS", "STARTS", "STOPS", "INSTANCES"],
                &records.iter().map(|r| vec![
                    r.tenant_id.clone(),
                    format!("{:.1}", r.cpu_seconds),
                    format!("{:.3}", r.memory_gb_seconds),
                    r.total_starts.to_string(),
                    r.total_stops.to_string(),
                    r.current_instances.to_string(),
                ]).collect::<Vec<_>>(),
            );
        }

        UsageCmd::Get { tenant } => {
            let r = client.usage_tenant(&tenant)?;
            fmt::section(&format!("Usage — {}", tenant));
            fmt::kv("CPU seconds",        &format!("{:.2}", r.cpu_seconds));
            fmt::kv("Memory GB-seconds",  &format!("{:.4}", r.memory_gb_seconds));
            fmt::kv("Total starts",       &r.total_starts.to_string());
            fmt::kv("Total stops",        &r.total_stops.to_string());
            fmt::kv("Current instances",  &r.current_instances.to_string());
            fmt::kv("Period start",       &r.period_start.to_string());
            fmt::kv("Last updated",       &r.last_updated.to_string());
        }

        UsageCmd::Reset { tenant } => {
            client.usage_reset(&tenant)?;
            fmt::ok(&format!("Usage counters reset for tenant '{}'", tenant));
        }
    }
    Ok(())
}
