//! CLI commands for automatic architecture selection.

use anyhow::Result;
use clap::Subcommand;
use apollo_core::arch_selector::{ArchitectureDecision, QuickClassifyRequest};
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum ArchCmd {
    /// Analyse a WorkflowDef file and get a full architecture decision
    Select {
        /// Path to WorkflowDef JSON file
        #[arg(long)] file: std::path::PathBuf,
        /// Node region to check against (default: us-east-1)
        #[arg(long, default_value = "us-east-1")] region: String,
    },
    /// Quick heuristic classification without a full WorkflowDef
    Classify {
        /// Number of distinct tools/agents
        #[arg(long)] tools:    usize,
        /// Number of parallel branches (0 = sequential)
        #[arg(long)] branches: usize,
        /// Error tolerance: 0=none 1=low 2=medium 3=high
        #[arg(long, default_value = "0")] tolerance: u8,
        /// Whether governance is strict
        #[arg(long)] strict: bool,
        /// Tenant ID for governance lookup
        #[arg(long, default_value = "default")] tenant: String,
    },
}

pub fn run(cmd: ArchCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        ArchCmd::Select { file, .. } => {
            let raw = std::fs::read_to_string(&file)?;
            let def: apollo_core::orchestration::WorkflowDef = serde_json::from_str(&raw)?;
            let decision = client.arch_select(&def)?;
            print_decision(&decision);
        }

        ArchCmd::Classify { tools, branches, tolerance, strict, tenant } => {
            let req = QuickClassifyRequest {
                tenant_id:         tenant,
                tool_count:        tools,
                parallel_branches: branches,
                error_tolerance:   tolerance,
                governance_strict: strict,
            };
            let resp = client.arch_classify(&req)?;
            fmt::section("Quick Architecture Classification");
            fmt::kv("Architecture", &format!("{:?}", resp.architecture));
            fmt::kv("Confidence",   &format!("{:.0}%", resp.confidence * 100.0));
            fmt::kv("Summary",      &resp.one_line);
            if !resp.reasoning.is_empty() {
                println!();
                for r in &resp.reasoning {
                    println!("  • {}", r);
                }
            }
        }
    }
    Ok(())
}

/// Shared printer used by arch_cmd and workflow arch subcommand.
pub fn print_decision(d: &ArchitectureDecision) {
    use colored::Colorize;
    fmt::section("Architecture Decision");
    let arch_str = format!("{:?}", d.architecture);
    let colored_arch = match arch_str.as_str() {
        "Deterministic" => arch_str.green().bold().to_string(),
        "SingleAgent"   => arch_str.yellow().bold().to_string(),
        "MultiAgent"    => arch_str.cyan().bold().to_string(),
        _               => arch_str.white().to_string(),
    };
    fmt::kv("Selected",    &colored_arch);
    fmt::kv("Confidence",  &format!("{:.0}%", d.confidence * 100.0));
    println!();

    fmt::kv("Scores",      "");
    println!("    Deterministic  {:>5.0}  {}",
        d.scores.deterministic, fmt::health_bar(d.scores.deterministic as u32));
    println!("    SingleAgent    {:>5.0}  {}",
        d.scores.single_agent, fmt::health_bar(d.scores.single_agent as u32));
    println!("    MultiAgent     {:>5.0}  {}",
        d.scores.multi_agent, fmt::health_bar(d.scores.multi_agent as u32));

    println!();
    fmt::kv("DAG steps",        &d.dag.step_count.to_string());
    fmt::kv("Distinct agents",  &d.dag.distinct_agents.to_string());
    fmt::kv("Max parallel",     &d.dag.max_parallel_width.to_string());
    fmt::kv("Critical path",    &format!("{} steps", d.dag.critical_path_length));
    fmt::kv("Governance",       &format!("{} ({:.0}% strict)",
        if d.governance.all_agents_allowed { "all agents OK" } else { "agents BLOCKED" },
        d.governance.strictness_score * 100.0));

    println!();
    println!("  {}", "Reasoning:".bold());
    for r in &d.reasoning {
        println!("  • {}", r);
    }

    println!();
    println!("  {}", "Recommended config:".bold());
    fmt::kv("  Max concurrency",  &d.config.max_concurrency.to_string());
    fmt::kv("  Fail fast",        &d.config.fail_fast.to_string());
    fmt::kv("  Phase count",      &d.config.phase_count.to_string());
    if let Some(timeout) = d.config.suggested_total_timeout_secs {
        fmt::kv("  Timeout",      &format!("{}s", timeout));
    }
    if !d.config.retry_eligible_steps.is_empty() {
        fmt::kv("  Retry eligible", &d.config.retry_eligible_steps.join(", "));
    }
}
