//! CLI commands for orchestration: blueprints, agent groups, and workflows.

use anyhow::Result;
use clap::Subcommand;
use crate::client::NodeClient;
use crate::fmt;

// ── Blueprints ────────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum BlueprintCmd {
    /// List all blueprints
    List,
    /// Create a new blueprint
    Create {
        #[arg(long)] name:        String,
        #[arg(long)] agent:       String,
        /// Short description of this blueprint
        #[arg(long, default_value = "")]
        description: String,
        #[arg(long)] pin_version: Option<String>,
        #[arg(long)] region:      Option<String>,
        /// Comma-separated tags
        #[arg(long)] tags:        Option<String>,
    },
    /// Show a blueprint
    Get { id: String },
    /// Delete a blueprint
    Delete { id: String },
    /// Deploy a blueprint for a tenant
    Deploy { id: String, #[arg(long)] tenant: String },
}

pub fn run_blueprint(cmd: BlueprintCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        BlueprintCmd::List => {
            let bps = client.blueprints_list()?;
            if bps.is_empty() {
                fmt::info("No blueprints. Use 'apollo blueprint create' to add one.");
                return Ok(());
            }
            fmt::section("Blueprints");
            fmt::print_table(
                &["ID", "NAME", "AGENT", "VERSION", "REGION", "TAGS"],
                &bps.iter().map(|b| vec![
                    b.id[..8].to_string(),
                    b.name.clone(),
                    b.agent_id.clone(),
                    b.pin_version.clone().unwrap_or_else(|| "latest".into()),
                    b.region.clone().unwrap_or_else(|| "any".into()),
                    b.tags.join(", "),
                ]).collect::<Vec<_>>(),
            );
        }

        BlueprintCmd::Create { name, agent, pin_version, region, tags, description } => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            // Generate a deterministic ID from name + timestamp
            let id = format!("{:x}", now ^ (name.len() as u64).wrapping_mul(0x9e3779b97f4a7c15));
            let tag_list: Vec<String> = tags.unwrap_or_default()
                .split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
            let body = serde_json::json!({
                "id":          id,
                "name":        name,
                "description": description,
                "agent_id":    agent,
                "pin_version": pin_version,
                "region":      region,
                "tags":        tag_list,
                "default_env": {},
                "created_at":  now,
                "updated_at":  now,
            });
            let bp = client.blueprint_create(&body)?;
            fmt::ok(&format!("Created blueprint '{}' (id: {})", bp.name, &bp.id[..8.min(bp.id.len())]));
        }

        BlueprintCmd::Get { id } => {
            let b = client.blueprint_get(&id)?;
            fmt::section(&format!("Blueprint — {}", b.name));
            fmt::kv("ID",          &b.id);
            fmt::kv("Description", if b.description.is_empty() { "—" } else { &b.description });
            fmt::kv("Agent",       &b.agent_id);
            fmt::kv("Version",     &b.pin_version.clone().unwrap_or_else(|| "latest".into()));
            fmt::kv("Region",      &b.region.clone().unwrap_or_else(|| "any".into()));
            fmt::kv("Tags",        &b.tags.join(", "));
        }

        BlueprintCmd::Delete { id } => {
            client.blueprint_delete(&id)?;
            fmt::ok("Blueprint deleted.");
        }

        BlueprintCmd::Deploy { id, tenant } => {
            let result = client.blueprint_deploy(&id, &tenant)?;
            fmt::ok(&format!("Deployed blueprint for tenant '{}': {}",
                tenant, serde_json::to_string(&result).unwrap_or_default()));
        }
    }
    Ok(())
}

// ── Agent Groups ──────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum GroupCmd {
    List,
    Create {
        #[arg(long)] name:   String,
        #[arg(long)] tenant: String,
        /// Comma-separated agent IDs
        #[arg(long)] agents: String,
    },
    Get    { id: String },
    Delete { id: String },
    Run    { id: String },
    Stop   { id: String },
}

pub fn run_group(cmd: GroupCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        GroupCmd::List => {
            let groups = client.groups_list()?;
            if groups.is_empty() {
                fmt::info("No agent groups. Use 'apollo group create' to add one.");
                return Ok(());
            }
            fmt::section("Agent Groups");
            fmt::print_table(
                &["ID", "NAME", "TENANT", "MEMBERS", "STATUS"],
                &groups.iter().map(|g| vec![
                    g.id[..8].to_string(),
                    g.name.clone(),
                    g.tenant_id.clone(),
                    g.members.len().to_string(),
                    format!("{:?}", g.status),
                ]).collect::<Vec<_>>(),
            );
        }

        GroupCmd::Create { name, tenant, agents } => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let id = format!("{:x}", now ^ (name.len() as u64).wrapping_mul(0x9e3779b97f4a7c15));
            let members: Vec<serde_json::Value> = agents
                .split(',')
                .filter(|a| !a.trim().is_empty())
                .map(|a| serde_json::json!({"agent_id": a.trim(), "blueprint_id": null, "tenant_override": null}))
                .collect();
            let body = serde_json::json!({
                "id":          id,
                "name":        name,
                "description": "",
                "tenant_id":   tenant,
                "members":     members,
                "status":      "stopped",
                "created_at":  now,
                "updated_at":  now,
            });
            let g = client.group_create(&body)?;
            fmt::ok(&format!("Created group '{}' (id: {})", g.name, &g.id[..8.min(g.id.len())]));
        }

        GroupCmd::Get { id } => {
            let g = client.group_get(&id)?;
            fmt::section(&format!("Group — {}", g.name));
            fmt::kv("ID",      &g.id);
            fmt::kv("Tenant",  &g.tenant_id);
            fmt::kv("Status",  &format!("{:?}", g.status));
            fmt::kv("Members", &g.members.iter().map(|m| m.agent_id.clone()).collect::<Vec<_>>().join(", "));
        }

        GroupCmd::Delete { id } => {
            client.group_delete(&id)?;
            fmt::ok("Group deleted.");
        }

        GroupCmd::Run { id } => {
            client.group_run(&id)?;
            fmt::ok("Started all agents in group.");
        }

        GroupCmd::Stop { id } => {
            client.group_stop(&id)?;
            fmt::ok("Stopped all agents in group.");
        }
    }
    Ok(())
}

// ── Workflows ─────────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum WorkflowCmd {
    List,
    /// Create a workflow from a JSON file
    Create {
        #[arg(long)] file: std::path::PathBuf,
    },
    Get    { id: String },
    Delete { id: String },
    /// Execute a workflow and show step progress
    Run    { id: String },
    /// List all runs for a workflow
    Runs   { id: String },
    /// Show step-by-step status of a workflow run
    Status { run_id: String },
    /// Run architecture selection on a saved workflow
    Arch   { id: String },
}

pub fn run_workflow(cmd: WorkflowCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        WorkflowCmd::List => {
            let defs = client.workflows_list()?;
            if defs.is_empty() {
                fmt::info("No workflow definitions. Use 'apollo workflow create --file wf.json'.");
                return Ok(());
            }
            fmt::section("Workflow Definitions");
            fmt::print_table(
                &["ID", "NAME", "TENANT", "STEPS"],
                &defs.iter().map(|d| vec![
                    d.id[..8].to_string(),
                    d.name.clone(),
                    d.tenant_id.clone(),
                    d.steps.len().to_string(),
                ]).collect::<Vec<_>>(),
            );
        }

        WorkflowCmd::Create { file } => {
            let raw = std::fs::read_to_string(&file)?;
            let mut def: apollo_core::orchestration::WorkflowDef = serde_json::from_str(&raw)?;
            def.id = String::new(); // let server assign
            let saved = client.workflow_create(&def)?;
            fmt::ok(&format!("Created workflow '{}' (id: {})", saved.name, &saved.id[..8]));
        }

        WorkflowCmd::Get { id } => {
            let d = client.workflow_get(&id)?;
            fmt::section(&format!("Workflow — {}", d.name));
            fmt::kv("ID",     &d.id);
            fmt::kv("Tenant", &d.tenant_id);
            println!();
            fmt::print_table(
                &["STEP ID", "NAME", "AGENT", "DEPENDS ON", "OPTIONAL"],
                &d.steps.iter().map(|s| vec![
                    s.step_id.clone(),
                    s.name.clone(),
                    s.agent_id.clone(),
                    if s.depends_on.is_empty() { "—".into() } else { s.depends_on.join(", ") },
                    s.optional.to_string(),
                ]).collect::<Vec<_>>(),
            );
        }

        WorkflowCmd::Delete { id } => {
            client.workflow_delete(&id)?;
            fmt::ok("Workflow deleted.");
        }

        WorkflowCmd::Run { id } => {
            let run = client.workflow_run(&id)?;
            fmt::section(&format!("Workflow Run — {}", &run.run_id[..8]));
            fmt::kv("Run ID",  &run.run_id);
            fmt::kv("Status",  &format!("{:?}", run.status));
            println!();
            fmt::print_table(
                &["STEP ID", "STATUS", "AGENT PID", "STARTED", "FINISHED"],
                &run.steps.iter().map(|s| vec![
                    s.step_id.clone(),
                    fmt::status_str(&format!("{:?}", s.status)),
                    s.agent_pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
                    s.started_at.map(|t| t.to_string()).unwrap_or_else(|| "—".into()),
                    s.finished_at.map(|t| t.to_string()).unwrap_or_else(|| "—".into()),
                ]).collect::<Vec<_>>(),
            );
        }

        WorkflowCmd::Runs { id } => {
            let runs = client.workflow_runs(&id)?;
            if runs.is_empty() {
                fmt::info("No runs for this workflow yet.");
                return Ok(());
            }
            fmt::section("Workflow Runs");
            fmt::print_table(
                &["RUN ID", "STATUS", "STEPS DONE", "STARTED"],
                &runs.iter().map(|r| {
                    let done = r.steps.iter().filter(|s| {
                        matches!(s.status, apollo_core::orchestration::StepStatus::Completed)
                    }).count();
                    vec![
                        r.run_id[..8].to_string(),
                        format!("{:?}", r.status),
                        format!("{}/{}", done, r.steps.len()),
                        r.started_at.to_string(),
                    ]
                }).collect::<Vec<_>>(),
            );
        }

        WorkflowCmd::Status { run_id } => {
            let run = client.workflow_run_get(&run_id)?;
            fmt::section(&format!("Run {} — {:?}", &run_id[..8], run.status));
            fmt::print_table(
                &["STEP", "STATUS", "PID", "ERROR"],
                &run.steps.iter().map(|s| vec![
                    s.step_id.clone(),
                    fmt::status_str(&format!("{:?}", s.status)),
                    s.agent_pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
                    s.error.clone().unwrap_or_else(|| "—".into()),
                ]).collect::<Vec<_>>(),
            );
        }

        WorkflowCmd::Arch { id } => {
            let decision = client.workflow_arch(&id)?;
            super::arch_cmd::print_decision(&decision);
        }
    }
    Ok(())
}
