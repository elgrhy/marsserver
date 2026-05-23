//! Synchronous wizard prompts for the journey-first CLI.
//!
//! All `dialoguer` calls live here — no async, no tokio.
//! Called from journey.rs via `tokio::task::block_in_place`.

use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use std::path::PathBuf;

use crate::fmt;

fn t() -> ColorfulTheme {
    ColorfulTheme::default()
}

// ── Main interactive menu ─────────────────────────────────────────────────────

pub const MENU_ITEMS: &[(&str, &str)] = &[
    ("init",      "Set up Apollo for the first time"),
    ("add",       "Add an agent to your workspace"),
    ("run",       "Start a registered agent"),
    ("stop",      "Stop a running agent"),
    ("status",    "Show running agents and node state"),
    ("logs",      "Tail an agent's output"),
    ("dashboard", "Live fleet monitoring terminal"),
    ("doctor",    "12-point system validation"),
    ("demo",      "Offline guided platform tour"),
    ("guide",     "Built-in documentation"),
    ("exit",      "Quit"),
];

/// Show the top-level landing menu. Returns index into MENU_ITEMS.
pub fn main_menu() -> Result<usize> {
    fmt::apollo_banner();
    println!("{}", "  AI Agent Execution Platform  ·  v2.0\n".dimmed());
    let labels: Vec<String> = MENU_ITEMS
        .iter()
        .map(|(cmd, desc)| format!("{:<14}  {}", cmd.bold(), desc.dimmed()))
        .collect();
    Ok(Select::with_theme(&t())
        .with_prompt("What would you like to do?")
        .items(&labels)
        .default(0)
        .interact()?)
}

// ── apollo init ───────────────────────────────────────────────────────────────

pub struct InitResult {
    pub base_dir:   PathBuf,
    pub secret_key: String,
    pub region:     String,
    pub port:       u16,
}

pub fn prompt_init() -> Result<InitResult> {
    fmt::apollo_banner();
    fmt::section("First-Time Setup");
    println!("  Let's configure your Apollo workspace.\n");

    let dir: String = Input::with_theme(&t())
        .with_prompt("  Workspace directory")
        .default(".apollo".into())
        .show_default(true)
        .interact_text()?;

    println!();
    println!("  {}", "API key — used by the CLI and SDK to authenticate with the node.".dimmed());
    let key: String = Input::with_theme(&t())
        .with_prompt("  Secret key")
        .default("apollo-dev-secret".into())
        .show_default(true)
        .interact_text()?;

    if key == "apollo-dev-secret" {
        fmt::warn_hint("Using the default key is fine for development. Use a strong random key for production.");
    }

    let port_str: String = Input::with_theme(&t())
        .with_prompt("  HTTP port")
        .default("8080".into())
        .show_default(true)
        .interact_text()?;
    let port = port_str.parse::<u16>().unwrap_or(8080);

    let region: String = Input::with_theme(&t())
        .with_prompt("  Region tag  (optional, e.g. us-east-1)")
        .default("default".into())
        .show_default(true)
        .interact_text()?;

    Ok(InitResult {
        base_dir: PathBuf::from(dir),
        secret_key: key,
        region,
        port,
    })
}

// ── apollo add ────────────────────────────────────────────────────────────────

pub struct AddSource {
    pub source:            String,
    pub generate_template: bool,
    pub template_dir:      Option<PathBuf>,
}

pub fn prompt_add(prefill: Option<&str>) -> Result<AddSource> {
    fmt::section("Add Agent");

    // If a source was already given on the command line, skip the menu
    if let Some(src) = prefill {
        if !src.is_empty() {
            return Ok(AddSource {
                source:            src.to_string(),
                generate_template: false,
                template_dir:      None,
            });
        }
    }

    println!("  {}", "Where is your agent?\n".dimmed());

    let methods = vec![
        format!("{:<16}  {}", "Template".bold(),
            "generate a starter agent on disk — edit & run".dimmed()),
        format!("{:<16}  {}", "Local path".bold(),
            "folder or .tar.gz / .zip on this machine".dimmed()),
        format!("{:<16}  {}", "Git repo".bold(),
            "https://github.com/org/my-agent.git".dimmed()),
        format!("{:<16}  {}", "HTTPS archive".bold(),
            "https://releases.example.com/agent.tar.gz".dimmed()),
    ];

    let method = Select::with_theme(&t())
        .with_prompt("Source")
        .items(&methods)
        .default(0)
        .interact()?;

    match method {
        0 => {
            println!();
            println!("  {}", "This creates a minimal agent.yaml + main.py you can edit and run immediately.".dimmed());
            let name: String = Input::with_theme(&t())
                .with_prompt("  Agent name")
                .default("my-agent".into())
                .interact_text()?;
            let dir = PathBuf::from(&name);
            Ok(AddSource {
                source:            name,
                generate_template: true,
                template_dir:      Some(dir),
            })
        }
        1 => {
            let path: String = Input::with_theme(&t())
                .with_prompt("  Path to agent directory")
                .interact_text()?;
            Ok(AddSource { source: path, generate_template: false, template_dir: None })
        }
        2 => {
            println!("  {}", "Example: https://github.com/your-org/my-agent.git".dimmed());
            let url: String = Input::with_theme(&t())
                .with_prompt("  Git URL")
                .interact_text()?;
            Ok(AddSource { source: url, generate_template: false, template_dir: None })
        }
        3 => {
            println!("  {}", "Example: https://releases.example.com/agent-1.0.tar.gz".dimmed());
            let url: String = Input::with_theme(&t())
                .with_prompt("  Archive URL")
                .interact_text()?;
            Ok(AddSource { source: url, generate_template: false, template_dir: None })
        }
        _ => unreachable!(),
    }
}

// ── apollo run ────────────────────────────────────────────────────────────────

pub struct RunResult {
    pub agent:  String,
    pub tenant: String,
}

pub fn prompt_run(
    prefill_agent:  Option<&str>,
    prefill_tenant: Option<&str>,
    agent_list:     &[String],
) -> Result<RunResult> {
    fmt::section("Run Agent");

    let agent = match prefill_agent {
        Some(a) if !a.is_empty() => a.to_string(),
        _ => {
            if agent_list.is_empty() {
                fmt::warn_hint("No agents registered yet.");
                return Err(anyhow::anyhow!(
                    "No agents registered.\n\n  Add one first:  apollo add"
                ));
            }
            let idx = Select::with_theme(&t())
                .with_prompt("  Which agent?")
                .items(agent_list)
                .default(0)
                .interact()?;
            agent_list[idx].clone()
        }
    };

    let tenant = match prefill_tenant {
        Some(tn) if !tn.is_empty() => tn.to_string(),
        _ => {
            Input::<String>::with_theme(&t())
                .with_prompt("  Tenant / user ID")
                .default("default".into())
                .show_default(true)
                .interact_text()?
        }
    };

    Ok(RunResult { agent, tenant })
}

// ── apollo stop ───────────────────────────────────────────────────────────────

/// Returns `Some(index)` into `running_labels`, or `None` if nothing to stop.
pub fn prompt_stop_index(running_labels: &[String]) -> Result<Option<usize>> {
    fmt::section("Stop Agent");

    if running_labels.is_empty() {
        fmt::info("No running agents found.");
        fmt::hint("Start one with:  apollo run");
        return Ok(None);
    }

    Ok(Some(
        Select::with_theme(&t())
            .with_prompt("  Which agent would you like to stop?")
            .items(running_labels)
            .default(0)
            .interact()?,
    ))
}

// ── apollo logs ───────────────────────────────────────────────────────────────

pub struct LogsSelection {
    pub index:  usize,
    pub follow: bool,
    pub lines:  usize,
}

/// Returns a selection, or `None` if there's nothing to tail.
pub fn prompt_logs_index(agent_labels: &[String]) -> Result<Option<LogsSelection>> {
    fmt::section("Agent Logs");

    if agent_labels.is_empty() {
        fmt::info("No agent instances found.");
        fmt::hint("Start one with:  apollo run");
        return Ok(None);
    }

    let idx = Select::with_theme(&t())
        .with_prompt("  Which agent?")
        .items(agent_labels)
        .default(0)
        .interact()?;

    let follow = Confirm::with_theme(&t())
        .with_prompt("  Follow output in real time? (Ctrl-C to stop)")
        .default(false)
        .interact()?;

    let lines_str: String = Input::<String>::with_theme(&t())
        .with_prompt("  How many lines to show?")
        .default("50".into())
        .show_default(true)
        .interact_text()?;
    let lines = lines_str.parse::<usize>().unwrap_or(50);

    Ok(Some(LogsSelection { index: idx, follow, lines }))
}
