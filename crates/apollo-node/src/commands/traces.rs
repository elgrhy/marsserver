//! CLI commands for distributed tracing / observability.

use anyhow::Result;
use clap::Subcommand;
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum TracesCmd {
    /// List trace summaries for a tenant+agent
    List {
        tenant: String,
        agent:  String,
    },
    /// Show full trace with all spans
    Get {
        tenant:   String,
        agent:    String,
        trace_id: String,
    },
    /// Show per-model token cost summary (billing)
    Tokens { tenant: String },
}

pub fn run(cmd: TracesCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        TracesCmd::List { tenant, agent } => {
            let summaries = client.traces_list(&tenant, &agent)?;
            if summaries.is_empty() {
                fmt::info("No traces found.");
                return Ok(());
            }
            fmt::section("Traces");
            fmt::print_table(
                &["TRACE ID", "SPANS", "STATUS", "COST $", "DURATION ms", "FINALIZED"],
                &summaries.iter().map(|s| vec![
                    s.trace_id[..8].to_string(),
                    s.span_count.to_string(),
                    fmt::status_str(&format!("{:?}", s.status)),
                    format!("{:.4}", s.total_cost_usd),
                    s.duration_ms.map(|d| d.to_string()).unwrap_or_else(|| "—".into()),
                    "—".into(),  // finalized flag not in summary
                ]).collect::<Vec<_>>(),
            );
        }

        TracesCmd::Get { tenant, agent, trace_id } => {
            let spans = client.traces_get(&tenant, &agent, &trace_id)?;
            if spans.is_empty() {
                fmt::info("Trace not found or empty.");
                return Ok(());
            }
            fmt::section(&format!("Trace {}", &trace_id[..8.min(trace_id.len())]));
            fmt::print_table(
                &["SPAN", "NAME", "STATUS", "DURATION ms", "TOKENS IN", "TOKENS OUT", "COST $"],
                &spans.iter().map(|s| vec![
                    s.span_id[..8].to_string(),
                    s.name.clone(),
                    fmt::status_str(&format!("{:?}", s.status)),
                    s.duration_ms.map(|d| d.to_string()).unwrap_or_else(|| "—".into()),
                    s.token_usage.as_ref().map(|t| t.input_tokens.to_string()).unwrap_or_else(|| "—".into()),
                    s.token_usage.as_ref().map(|t| t.output_tokens.to_string()).unwrap_or_else(|| "—".into()),
                    s.token_usage.as_ref().map(|t| format!("{:.4}", t.cost_usd)).unwrap_or_else(|| "—".into()),
                ]).collect::<Vec<_>>(),
            );
        }

        TracesCmd::Tokens { tenant } => {
            let stats = client.traces_tokens(&tenant)?;
            fmt::section(&format!("Token Usage — tenant: {}", tenant));
            fmt::kv("Total input tokens",  &stats.total_input_tokens.to_string());
            fmt::kv("Total output tokens", &stats.total_output_tokens.to_string());
            fmt::kv("Total cost (USD)",    &format!("${:.4}", stats.total_cost_usd));
            fmt::kv("Trace count",         &stats.trace_count.to_string());
            // TokenStats doesn't have per-model breakdown in v2.0 — show summary only
        }
    }
    Ok(())
}
