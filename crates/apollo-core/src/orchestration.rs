//! Provider-Oriented Orchestration APIs.
//!
//! Extends Apollo beyond single-agent execution with:
//!
//! 1. **Blueprints** — parameterized agent deployment templates. A blueprint
//!    captures the agent ID, version pin, default env, resource overrides, and
//!    deployment constraints. Instantiating a blueprint creates a running agent.
//!
//! 2. **Agent Groups** — named collections of agents that start/stop together,
//!    share a lifecycle, and expose a single status endpoint.
//!
//! 3. **Workflows** — lightweight DAG orchestration. A workflow is a directed
//!    acyclic graph of agent steps. Steps may run in parallel or sequentially.
//!    Each step specifies an agent, input mapping, and success condition.
//!    Workflow state is persisted so runs survive node restarts.
//!
//! Storage:
//!   `base_dir/blueprints/{id}.json`
//!   `base_dir/groups/{id}.json`
//!   `base_dir/workflows/definitions/{id}.json`
//!   `base_dir/workflows/runs/{run_id}.json`

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ══════════════════════════════════════════════════════════════════════════════
// BLUEPRINTS
// ══════════════════════════════════════════════════════════════════════════════

/// A reusable deployment template for an agent.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Blueprint {
    pub id:          String,
    pub name:        String,
    pub description: String,

    /// Agent to deploy (must already be registered on the node).
    pub agent_id:    String,
    /// If set, only this exact version may be deployed.
    pub pin_version: Option<String>,

    /// Default env vars injected at deploy time (merged with tenant secrets).
    #[serde(default)]
    pub default_env: HashMap<String, String>,

    /// Resource overrides (replaces agent.yaml values if set).
    pub cpu_override:    Option<f32>,
    pub memory_override: Option<String>,
    pub timeout_override: Option<u32>,

    /// Tags for discovery / catalog search.
    #[serde(default)]
    pub tags:        Vec<String>,

    /// Region constraint for deployments from this blueprint.
    pub region:      Option<String>,

    pub created_at:  u64,
    pub updated_at:  u64,
}

pub fn blueprints_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("blueprints")
}

fn blueprint_path(base_dir: &Path, id: &str) -> PathBuf {
    blueprints_dir(base_dir).join(format!("{}.json", sanitize(id)))
}

pub fn load_blueprint(base_dir: &Path, id: &str) -> Option<Blueprint> {
    let path = blueprint_path(base_dir, id);
    if !path.exists() { return None; }
    fs::read_to_string(&path).ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn list_blueprints(base_dir: &Path) -> Vec<Blueprint> {
    let dir = blueprints_dir(base_dir);
    if !dir.exists() { return vec![]; }
    fs::read_dir(&dir).ok().map(|entries| {
        entries.flatten()
            .filter_map(|e| {
                let raw = fs::read_to_string(e.path()).ok()?;
                serde_json::from_str(&raw).ok()
            })
            .collect()
    }).unwrap_or_default()
}

pub fn save_blueprint(base_dir: &Path, mut bp: Blueprint) -> Result<Blueprint> {
    if bp.id.is_empty() { bp.id = Uuid::new_v4().to_string(); }
    let now = now_unix();
    if bp.created_at == 0 { bp.created_at = now; }
    bp.updated_at = now;

    let path = blueprint_path(base_dir, &bp.id);
    fs::create_dir_all(blueprints_dir(base_dir)).context("create blueprints dir")?;
    fs::write(&path, serde_json::to_string_pretty(&bp)?)?;
    Ok(bp)
}

pub fn delete_blueprint(base_dir: &Path, id: &str) -> Result<()> {
    let path = blueprint_path(base_dir, id);
    if !path.exists() { return Err(anyhow!("Blueprint '{}' not found", id)); }
    fs::remove_file(path)?;
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// AGENT GROUPS
// ══════════════════════════════════════════════════════════════════════════════

/// A named group of agents that share a lifecycle.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AgentGroup {
    pub id:         String,
    pub name:       String,
    pub description: String,
    pub tenant_id:  String,

    /// Ordered list of member agents (agent_id, optional blueprint_id).
    pub members:    Vec<GroupMember>,

    /// Group-level status.
    pub status:     GroupStatus,

    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GroupMember {
    pub agent_id:     String,
    pub blueprint_id: Option<String>,
    /// Override the default tenant when running this member.
    pub tenant_override: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupStatus {
    #[default]
    Idle,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

fn groups_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("groups")
}

fn group_path(base_dir: &Path, id: &str) -> PathBuf {
    groups_dir(base_dir).join(format!("{}.json", sanitize(id)))
}

pub fn load_group(base_dir: &Path, id: &str) -> Option<AgentGroup> {
    let path = group_path(base_dir, id);
    if !path.exists() { return None; }
    fs::read_to_string(&path).ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn list_groups(base_dir: &Path) -> Vec<AgentGroup> {
    let dir = groups_dir(base_dir);
    if !dir.exists() { return vec![]; }
    fs::read_dir(&dir).ok().map(|entries| {
        entries.flatten()
            .filter_map(|e| {
                let raw = fs::read_to_string(e.path()).ok()?;
                serde_json::from_str(&raw).ok()
            })
            .collect()
    }).unwrap_or_default()
}

pub fn save_group(base_dir: &Path, mut group: AgentGroup) -> Result<AgentGroup> {
    if group.id.is_empty() { group.id = Uuid::new_v4().to_string(); }
    let now = now_unix();
    if group.created_at == 0 { group.created_at = now; }
    group.updated_at = now;

    let path = group_path(base_dir, &group.id);
    fs::create_dir_all(groups_dir(base_dir)).context("create groups dir")?;
    fs::write(&path, serde_json::to_string_pretty(&group)?)?;
    Ok(group)
}

pub fn delete_group(base_dir: &Path, id: &str) -> Result<()> {
    let path = group_path(base_dir, id);
    if !path.exists() { return Err(anyhow!("Group '{}' not found", id)); }
    fs::remove_file(path)?;
    Ok(())
}

pub fn set_group_status(base_dir: &Path, id: &str, status: GroupStatus) -> Result<()> {
    let mut group = load_group(base_dir, id)
        .ok_or_else(|| anyhow!("Group '{}' not found", id))?;
    group.status = status;
    save_group(base_dir, group)?;
    Ok(())
}

// ══════════════════════════════════════════════════════════════════════════════
// WORKFLOWS  (DAG orchestration)
// ══════════════════════════════════════════════════════════════════════════════

/// A workflow definition — a DAG of agent steps.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkflowDef {
    pub id:          String,
    pub name:        String,
    pub description: String,
    pub tenant_id:   String,

    /// Ordered (topological) list of steps.
    pub steps:       Vec<WorkflowStep>,

    pub created_at:  u64,
    pub updated_at:  u64,
}

/// One step in a workflow.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkflowStep {
    pub step_id:    String,
    pub name:       String,
    /// Agent to run for this step.
    pub agent_id:   String,
    /// IDs of steps that must complete before this one starts.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Env vars passed to this step (may reference prior step outputs via `${step_id.key}`).
    #[serde(default)]
    pub env:        HashMap<String, String>,
    /// If true, step errors are non-fatal (workflow continues).
    pub optional:   bool,
    /// Maximum seconds this step may run.
    pub timeout_secs: Option<u64>,
}

// ── Workflow run (execution state) ─────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WorkflowRun {
    pub run_id:      String,
    pub workflow_id: String,
    pub tenant_id:   String,
    pub status:      WorkflowStatus,
    pub steps:       Vec<StepExecution>,
    pub started_at:  u64,
    pub finished_at: Option<u64>,
    pub error:       Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StepExecution {
    pub step_id:     String,
    pub status:      StepStatus,
    pub agent_pid:   Option<u32>,
    pub started_at:  Option<u64>,
    pub finished_at: Option<u64>,
    pub output:      Option<serde_json::Value>,
    pub error:       Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

// ── Workflow storage ──────────────────────────────────────────────────────────

fn workflow_defs_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("workflows").join("definitions")
}

fn workflow_runs_dir(base_dir: &Path) -> PathBuf {
    base_dir.join("workflows").join("runs")
}

fn workflow_def_path(base_dir: &Path, id: &str) -> PathBuf {
    workflow_defs_dir(base_dir).join(format!("{}.json", sanitize(id)))
}

fn workflow_run_path(base_dir: &Path, run_id: &str) -> PathBuf {
    workflow_runs_dir(base_dir).join(format!("{}.json", sanitize(run_id)))
}

pub fn load_workflow_def(base_dir: &Path, id: &str) -> Option<WorkflowDef> {
    let path = workflow_def_path(base_dir, id);
    if !path.exists() { return None; }
    fs::read_to_string(&path).ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn list_workflow_defs(base_dir: &Path) -> Vec<WorkflowDef> {
    let dir = workflow_defs_dir(base_dir);
    if !dir.exists() { return vec![]; }
    fs::read_dir(&dir).ok().map(|entries| {
        entries.flatten()
            .filter_map(|e| {
                let raw = fs::read_to_string(e.path()).ok()?;
                serde_json::from_str(&raw).ok()
            })
            .collect()
    }).unwrap_or_default()
}

pub fn save_workflow_def(base_dir: &Path, mut def: WorkflowDef) -> Result<WorkflowDef> {
    if def.id.is_empty() { def.id = Uuid::new_v4().to_string(); }
    let now = now_unix();
    if def.created_at == 0 { def.created_at = now; }
    def.updated_at = now;

    fs::create_dir_all(workflow_defs_dir(base_dir)).context("create workflow defs dir")?;
    fs::write(workflow_def_path(base_dir, &def.id), serde_json::to_string_pretty(&def)?)?;
    Ok(def)
}

pub fn delete_workflow_def(base_dir: &Path, id: &str) -> Result<()> {
    let path = workflow_def_path(base_dir, id);
    if !path.exists() { return Err(anyhow!("Workflow '{}' not found", id)); }
    fs::remove_file(path)?;
    Ok(())
}

pub fn create_workflow_run(base_dir: &Path, def: &WorkflowDef) -> Result<WorkflowRun> {
    let run_id = Uuid::new_v4().to_string();
    let steps  = def.steps.iter().map(|s| StepExecution {
        step_id:     s.step_id.clone(),
        status:      StepStatus::Pending,
        agent_pid:   None,
        started_at:  None,
        finished_at: None,
        output:      None,
        error:       None,
    }).collect();

    let run = WorkflowRun {
        run_id:      run_id.clone(),
        workflow_id: def.id.clone(),
        tenant_id:   def.tenant_id.clone(),
        status:      WorkflowStatus::Pending,
        steps,
        started_at:  now_unix(),
        finished_at: None,
        error:       None,
    };

    save_workflow_run(base_dir, &run)?;
    Ok(run)
}

pub fn save_workflow_run(base_dir: &Path, run: &WorkflowRun) -> Result<()> {
    fs::create_dir_all(workflow_runs_dir(base_dir)).context("create workflow runs dir")?;
    fs::write(
        workflow_run_path(base_dir, &run.run_id),
        serde_json::to_string_pretty(run)?,
    )?;
    Ok(())
}

pub fn load_workflow_run(base_dir: &Path, run_id: &str) -> Option<WorkflowRun> {
    let path = workflow_run_path(base_dir, run_id);
    if !path.exists() { return None; }
    fs::read_to_string(&path).ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn list_workflow_runs(base_dir: &Path, workflow_id: &str) -> Vec<WorkflowRun> {
    let dir = workflow_runs_dir(base_dir);
    if !dir.exists() { return vec![]; }
    fs::read_dir(&dir).ok().map(|entries| {
        entries.flatten()
            .filter_map(|e| {
                let raw = fs::read_to_string(e.path()).ok()?;
                let run: WorkflowRun = serde_json::from_str(&raw).ok()?;
                if run.workflow_id == workflow_id { Some(run) } else { None }
            })
            .collect()
    }).unwrap_or_default()
}

/// Compute which steps are ready to execute given only the run state.
/// Note: use `ready_steps_with_def` for accurate dependency checking.
pub fn ready_steps(run: &WorkflowRun) -> Vec<String> {
    run.steps.iter()
        .filter(|s| s.status == StepStatus::Pending)
        .map(|s| s.step_id.clone())
        .collect()
}

/// Compute ready steps given both the run and the definition.
pub fn ready_steps_with_def(run: &WorkflowRun, def: &WorkflowDef) -> Vec<String> {
    let completed: std::collections::HashSet<&str> = run.steps.iter()
        .filter(|s| matches!(s.status, StepStatus::Completed | StepStatus::Skipped))
        .map(|s| s.step_id.as_str())
        .collect();

    def.steps.iter()
        .filter(|def_step| {
            let exec = run.steps.iter().find(|s| s.step_id == def_step.step_id);
            matches!(exec.map(|e| &e.status), Some(StepStatus::Pending) | None)
        })
        .filter(|def_step| def_step.depends_on.iter().all(|dep| completed.contains(dep.as_str())))
        .map(|s| s.step_id.clone())
        .collect()
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
