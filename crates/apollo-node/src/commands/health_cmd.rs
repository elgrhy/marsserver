//! CLI commands for health intelligence.

use anyhow::Result;
use clap::Subcommand;
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum HealthCmd {
    /// Show health score and details for one agent
    Agent { tenant: String, agent: String },
    /// Show health for all agents under a tenant
    Tenant { tenant: String },
    /// Show fleet-wide health summary
    Fleet,
}

pub fn run(cmd: HealthCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        HealthCmd::Agent { tenant, agent } => {
            let rec = client.health_agent(&tenant, &agent)?;
            fmt::section(&format!("Health — {}/{}", tenant, agent));
            fmt::kv("Status",        &fmt::status_str(&format!("{:?}", rec.status)));
            let score = rec.health_score as u32;
            fmt::kv("Score",         &format!("{} {}", score, fmt::health_bar(score)));
            fmt::kv("Instance ID",   &rec.instance_id);
            fmt::kv("Crash count",   &rec.crash_count.to_string());
            fmt::kv("Crash pattern", &rec.crash_pattern.clone().unwrap_or_else(|| "none".into()));
            fmt::kv("Memory trend",  &format!("{:.2} MB/min", rec.memory_trend_mb_per_min));
            fmt::kv("Avg CPU",       &format!("{:.1}%", rec.avg_cpu_pct));
            fmt::kv("Avg memory",    &format!("{} MB", rec.avg_memory_mb));
            if !rec.recent_crashes.is_empty() {
                println!();
                fmt::print_table(
                    &["TIME", "EXIT CODE", "OOM", "SIGNAL"],
                    &rec.recent_crashes.iter().rev().take(5).map(|c| vec![
                        format!("T+{}s", c.timestamp),
                        c.exit_code.map(|e| e.to_string()).unwrap_or_else(|| "—".into()),
                        c.was_oom.to_string(),
                        c.signal.clone().unwrap_or_else(|| "—".into()),
                    ]).collect::<Vec<_>>(),
                );
            }
        }

        HealthCmd::Tenant { tenant } => {
            let records = client.health_tenant(&tenant)?;
            if records.is_empty() {
                fmt::info(&format!("No health records for tenant '{}'.", tenant));
                return Ok(());
            }
            fmt::section(&format!("Health — tenant: {}", tenant));
            fmt::print_table(
                &["AGENT", "STATUS", "SCORE", "CRASHES", "PATTERN"],
                &records.iter().map(|r| vec![
                    r.agent_id.clone(),
                    fmt::status_str(&format!("{:?}", r.status)),
                    format!("{} {}", r.health_score as u32, fmt::health_bar(r.health_score as u32)),
                    r.crash_count.to_string(),
                    r.crash_pattern.clone().unwrap_or_else(|| "—".into()),
                ]).collect::<Vec<_>>(),
            );
        }

        HealthCmd::Fleet => {
            let summary = client.health_fleet()?;
            fmt::section("Fleet Health Summary");
            fmt::kv("Total agents", &summary.total_instances.to_string());
            fmt::kv("Healthy",      &summary.healthy.to_string());
            fmt::kv("Degraded",     &summary.degraded.to_string());
            fmt::kv("Critical",     &summary.critical.to_string());
            fmt::kv("Dead",         &summary.dead.to_string());
            fmt::kv("Avg score",    &format!("{:.1}", summary.avg_health_score));
            if summary.total_instances > 0 {
                let healthy_pct = summary.healthy * 100 / summary.total_instances;
                println!();
                println!("  Fleet health: {}", fmt::health_bar(healthy_pct as u32));
            }
        }
    }
    Ok(())
}
