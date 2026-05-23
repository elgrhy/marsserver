//! CLI commands for the job scheduler.

use anyhow::Result;
use clap::Subcommand;
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum ScheduleCmd {
    /// List all scheduled jobs
    List,
    /// Create a new scheduled job
    Create {
        #[arg(long)] name:   String,
        #[arg(long)] tenant: String,
        #[arg(long)] agent:  String,
        /// Cron expression (e.g. "0 * * * *")
        #[arg(long, conflicts_with_all = ["interval", "once"])]
        cron: Option<String>,
        /// Interval in seconds
        #[arg(long, conflicts_with_all = ["cron", "once"])]
        interval: Option<u64>,
        /// Unix timestamp for one-shot execution
        #[arg(long, conflicts_with_all = ["cron", "interval"])]
        once: Option<u64>,
    },
    /// Show a scheduled job
    Get { job_id: String },
    /// Delete a scheduled job
    Delete { job_id: String },
    /// Manually trigger a job now
    Run { job_id: String },
    /// Show run history for a job
    History { job_id: String },
}

pub fn run(cmd: ScheduleCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        ScheduleCmd::List => {
            let jobs = client.schedule_list()?;
            if jobs.is_empty() {
                fmt::info("No scheduled jobs. Use 'apollo schedule create' to add one.");
                return Ok(());
            }
            fmt::section("Scheduled Jobs");
            fmt::print_table(
                &["ID", "NAME", "TENANT", "AGENT", "TYPE", "NEXT RUN", "RUNS", "FAILS", "ENABLED"],
                &jobs.iter().map(|j| {
                    let sched_type = match &j.schedule {
                        apollo_core::scheduler::Schedule::Cron { expression } =>
                            format!("cron({})", expression),
                        apollo_core::scheduler::Schedule::Interval { secs } =>
                            format!("every {}s", secs),
                        apollo_core::scheduler::Schedule::Once { at } =>
                            format!("once@{}", at),
                    };
                    vec![
                        j.id[..8].to_string(),
                        j.name.clone(),
                        j.tenant_id.clone(),
                        j.agent_id.clone(),
                        sched_type,
                        format_next(j.next_run),
                        j.run_count.to_string(),
                        j.fail_count.to_string(),
                        if j.enabled { "✓".into() } else { "—".into() },
                    ]
                }).collect::<Vec<_>>(),
            );
        }

        ScheduleCmd::Create { name, tenant, agent, cron, interval, once } => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let id = format!("{:x}", now ^ (name.len() as u64 * 0x9e3779b97f4a7c15));
            let schedule = if let Some(expr) = cron {
                serde_json::json!({"type": "cron", "expression": expr})
            } else if let Some(secs) = interval {
                serde_json::json!({"type": "interval", "secs": secs})
            } else if let Some(at) = once {
                serde_json::json!({"type": "once", "at": at})
            } else {
                anyhow::bail!("Specify one of --cron, --interval, or --once");
            };
            let body = serde_json::json!({
                "id": id,
                "name": name,
                "tenant_id": tenant,
                "agent_id": agent,
                "schedule": schedule,
                "enabled": true,
                "extra_env": {},
                "created_at": now,
                "updated_at": now,
                "last_run": null,
                "next_run": now,
                "run_count": 0,
                "fail_count": 0,
            });
            let job = client.schedule_create(&body)?;
            fmt::ok(&format!("Created job '{}' (id: {})", job.name, &job.id[..8.min(job.id.len())]));
        }

        ScheduleCmd::Get { job_id } => {
            let j = client.schedule_get(&job_id)?;
            fmt::section(&format!("Job — {}", j.name));
            fmt::kv("ID",        &j.id);
            fmt::kv("Tenant",    &j.tenant_id);
            fmt::kv("Agent",     &j.agent_id);
            fmt::kv("Enabled",   &j.enabled.to_string());
            fmt::kv("Runs",      &j.run_count.to_string());
            fmt::kv("Failures",  &j.fail_count.to_string());
            fmt::kv("Next run",  &format_next(j.next_run));
        }

        ScheduleCmd::Delete { job_id } => {
            client.schedule_delete(&job_id)?;
            fmt::ok(&format!("Deleted job '{}'", &job_id[..8.min(job_id.len())]));
        }

        ScheduleCmd::Run { job_id } => {
            client.schedule_run(&job_id)?;
            fmt::ok(&format!("Triggered job '{}'", &job_id[..8.min(job_id.len())]));
        }

        ScheduleCmd::History { job_id } => {
            let history = client.schedule_history(&job_id)?;
            if history.is_empty() {
                fmt::info("No run history for this job.");
                return Ok(());
            }
            fmt::section("Job Run History");
            fmt::print_table(
                &["JOB ID", "STATUS", "FIRED AT", "DURATION ms", "MESSAGE"],
                &history.iter().rev().take(20).map(|r| vec![
                    r.job_id[..8.min(r.job_id.len())].to_string(),
                    r.status.clone(),
                    r.fired_at.to_string(),
                    r.duration_ms.map(|d| d.to_string()).unwrap_or_else(|| "—".into()),
                    r.message.clone().unwrap_or_else(|| "—".into()),
                ]).collect::<Vec<_>>(),
            );
        }
    }
    Ok(())
}

fn format_next(ts: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if ts <= now {
        "overdue".into()
    } else {
        let diff = ts - now;
        if diff < 60 { format!("in {}s", diff) }
        else if diff < 3600 { format!("in {}m {}s", diff/60, diff%60) }
        else { format!("in {}h {}m", diff/3600, (diff%3600)/60) }
    }
}
