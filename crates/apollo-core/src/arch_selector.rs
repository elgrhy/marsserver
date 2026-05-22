//! Automatic Architecture Selection Engine.
//!
//! Before a workflow executes, this engine analyses its DAG topology, governance
//! constraints, error-tolerance posture, and tool diversity to choose the correct
//! execution architecture:
//!
//! | Architecture   | When selected                                                  |
//! |----------------|----------------------------------------------------------------|
//! | `Deterministic` | 1 step, 1 agent, no branching, zero error tolerance           |
//! | `SingleAgent`   | ≤5 steps, 1 agent, sequential or light branching              |
//! | `MultiAgent`    | multiple distinct agents, real parallelism, or high fan-out   |
//!
//! ## Decision pipeline
//! 1. **DAG analysis** — topological levels, critical path, max parallel width
//! 2. **Signal extraction** — step count, distinct agents, optional ratio, etc.
//! 3. **Governance check** — validate all agents against the tenant's `TenantPolicy`
//! 4. **Scoring** — each architecture gets a 0-100 score from the signals
//! 5. **Decision** — highest score wins; confidence = score_winner / score_total
//! 6. **Config recommendation** — parallel groups, retry eligibility, concurrency cap
//!
//! REST API (added to the node):
//!   `POST /architecture/select`          — analyse a provided `WorkflowDef` inline
//!   `GET  /architecture/select/:wf_id`   — analyse a saved workflow by ID

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::orchestration::WorkflowDef;
use crate::policy::{load_policy, PolicyEngine, PolicyDecision};

// ── Architecture types ────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ArchitectureType {
    /// Single sequential path, one agent, zero error tolerance.
    /// Fastest, cheapest, most predictable — best for cron scripts, transforms.
    Deterministic,
    /// One agent across multiple tool-like steps, may branch lightly.
    /// Balanced — best for research tasks, document pipelines, chatbots.
    SingleAgent,
    /// Multiple distinct agents running in parallel across DAG levels.
    /// Most powerful — best for ETL, parallel analysis, complex orchestration.
    MultiAgent,
}

impl std::fmt::Display for ArchitectureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ArchitectureType::Deterministic => write!(f, "deterministic"),
            ArchitectureType::SingleAgent   => write!(f, "single_agent"),
            ArchitectureType::MultiAgent    => write!(f, "multi_agent"),
        }
    }
}

// ── DAG metrics ───────────────────────────────────────────────────────────────

/// Structural metrics extracted from the workflow DAG.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct DagMetrics {
    /// Total number of steps in the workflow.
    pub step_count: usize,
    /// Number of unique agent IDs across all steps.
    pub distinct_agents: usize,
    /// Maximum number of steps that can execute concurrently at any point.
    pub max_parallel_width: usize,
    /// Length of the longest dependency chain (critical path in steps).
    pub critical_path_length: usize,
    /// Steps along the critical path.
    pub critical_path: Vec<String>,
    /// Groups of steps per topological level — each inner Vec can run in parallel.
    pub parallel_groups: Vec<Vec<String>>,
    /// Fraction of steps marked `optional` (0.0–1.0).
    pub optional_ratio: f32,
    /// True if any step declares a `timeout_secs`.
    pub has_timeout_constraints: bool,
    /// True if the DAG has any branching (a step with multiple dependents).
    pub has_branching: bool,
    /// True if the DAG has any fan-in (a step with multiple dependencies).
    pub has_fan_in: bool,
}

/// Compute DAG metrics for a workflow definition.
pub fn analyse_dag(def: &WorkflowDef) -> DagMetrics {
    let n = def.steps.len();
    if n == 0 {
        return DagMetrics::default();
    }

    // Index steps by step_id
    let idx: HashMap<&str, usize> = def.steps.iter().enumerate()
        .map(|(i, s)| (s.step_id.as_str(), i))
        .collect();

    // Build adjacency (predecessors per step)
    let in_degree: Vec<usize> = def.steps.iter()
        .map(|s| s.depends_on.iter().filter(|d| idx.contains_key(d.as_str())).count())
        .collect();

    // Topological sort → assign levels (BFS from roots)
    let mut level = vec![0usize; n];
    let mut queue: VecDeque<usize> = VecDeque::new();
    let mut remaining_in: Vec<usize> = in_degree.clone();

    for (i, &deg) in remaining_in.iter().enumerate() {
        if deg == 0 { queue.push_back(i); }
    }

    // Build reverse adjacency (step → steps that depend on it)
    let mut successors: Vec<Vec<usize>> = vec![vec![]; n];
    for (i, step) in def.steps.iter().enumerate() {
        for dep in &step.depends_on {
            if let Some(&j) = idx.get(dep.as_str()) {
                successors[j].push(i);
            }
        }
    }

    let mut topo_order: Vec<usize> = Vec::with_capacity(n);
    while let Some(node) = queue.pop_front() {
        topo_order.push(node);
        for &succ in &successors[node] {
            level[succ] = level[succ].max(level[node] + 1);
            remaining_in[succ] -= 1;
            if remaining_in[succ] == 0 {
                queue.push_back(succ);
            }
        }
    }

    // Group steps by topological level → parallel groups
    let max_level = level.iter().copied().max().unwrap_or(0);
    let mut groups: Vec<Vec<String>> = vec![vec![]; max_level + 1];
    for (i, &lvl) in level.iter().enumerate() {
        groups[lvl].push(def.steps[i].step_id.clone());
    }
    let max_parallel_width = groups.iter().map(|g| g.len()).max().unwrap_or(1);

    // Critical path — longest chain by step count
    // dp[i] = length of longest path ending at step i
    let mut dp = vec![1usize; n];
    let mut parent = vec![usize::MAX; n];
    for &node in &topo_order {
        for &succ in &successors[node] {
            if dp[node] + 1 > dp[succ] {
                dp[succ]  = dp[node] + 1;
                parent[succ] = node;
            }
        }
    }
    let (cp_end, &cp_len) = dp.iter().enumerate()
        .max_by_key(|(_, &v)| v)
        .unwrap_or((0, &1));

    let mut cp_path: Vec<String> = Vec::new();
    let mut cur = cp_end;
    while cur != usize::MAX {
        cp_path.push(def.steps[cur].step_id.clone());
        cur = parent[cur];
    }
    cp_path.reverse();

    // Distinct agents
    let distinct_agents: HashSet<&str> = def.steps.iter()
        .map(|s| s.agent_id.as_str())
        .collect();

    // Branching: any step that has 2+ successors
    let has_branching = successors.iter().any(|s| s.len() >= 2);

    // Fan-in: any step with 2+ dependencies
    let has_fan_in = in_degree.iter().any(|&d| d >= 2);

    // Optional + timeout
    let optional_count = def.steps.iter().filter(|s| s.optional).count();
    let has_timeouts   = def.steps.iter().any(|s| s.timeout_secs.is_some());

    DagMetrics {
        step_count:           n,
        distinct_agents:      distinct_agents.len(),
        max_parallel_width,
        critical_path_length: cp_len,
        critical_path:        cp_path,
        parallel_groups:      groups,
        optional_ratio:       optional_count as f32 / n as f32,
        has_timeout_constraints: has_timeouts,
        has_branching,
        has_fan_in,
    }
}

// ── Governance check ──────────────────────────────────────────────────────────

/// Result of checking the workflow against the tenant's policy.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GovernanceCheck {
    /// All agents in the workflow are allowed under the tenant's policy.
    pub all_agents_allowed:  bool,
    /// Agent IDs that are blocked by policy.
    pub blocked_agents:      Vec<String>,
    /// Data residency constraint matches the node's region (or is absent).
    pub data_residency_ok:   bool,
    /// Tenant policy requires full audit logging.
    pub audit_required:      bool,
    /// Degree of governance strictness: 0.0 = open, 1.0 = maximum locked-down.
    pub strictness_score:    f32,
    /// Human-readable policy warnings (not errors — just advisories).
    pub policy_warnings:     Vec<String>,
}

pub fn check_governance(
    base_dir:    &Path,
    tenant_id:   &str,
    def:         &WorkflowDef,
    node_region: &str,
) -> GovernanceCheck {
    let policy  = load_policy(base_dir, tenant_id);
    let engine  = PolicyEngine::new(base_dir.to_path_buf());

    let mut blocked: Vec<String>  = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for step in &def.steps {
        match engine.check_run(tenant_id, &step.agent_id, node_region, 0) {
            PolicyDecision::Deny { reason, .. } => {
                blocked.push(step.agent_id.clone());
                warnings.push(format!("Step '{}': {}", step.step_id, reason));
            }
            PolicyDecision::Allow => {}
        }
    }

    let data_residency_ok = policy.data_residency.as_deref()
        .map_or(true, |r| r == node_region || r == "any");

    if !data_residency_ok {
        warnings.push(format!(
            "Data residency '{:?}' does not match node region '{}' — workflow may be rejected",
            policy.data_residency, node_region
        ));
    }

    // Strictness score: sum of constraint factors, normalised to 0-1
    let mut strictness = 0.0f32;
    if policy.require_audit              { strictness += 0.25; }
    if policy.data_residency.is_some()   { strictness += 0.25; }
    if policy.allowed_agents.is_some()   { strictness += 0.20; }
    if policy.max_instances.is_some()    { strictness += 0.15; }
    if policy.max_tokens_per_day.is_some() { strictness += 0.15; }

    GovernanceCheck {
        all_agents_allowed: blocked.is_empty(),
        blocked_agents:     blocked,
        data_residency_ok,
        audit_required:     policy.require_audit,
        strictness_score:   strictness.min(1.0),
        policy_warnings:    warnings,
    }
}

// ── Scoring ───────────────────────────────────────────────────────────────────

/// Raw 0-100 scores for each architecture type.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ArchitectureScores {
    pub deterministic: f32,
    pub single_agent:  f32,
    pub multi_agent:   f32,
}

fn score(dag: &DagMetrics, gov: &GovernanceCheck) -> ArchitectureScores {
    let mut det = 0.0f32;
    let mut sin = 0.0f32;
    let mut mul = 0.0f32;

    // ── Deterministic signals ─────────────────────────────────────────────────
    // Single step → overwhelmingly deterministic
    if dag.step_count == 1                  { det += 45.0; }
    // All same agent
    if dag.distinct_agents == 1             { det += 15.0; sin += 10.0; }
    // No parallelism at all
    if dag.max_parallel_width == 1          { det += 15.0; sin +=  5.0; }
    // Zero error tolerance (no optional steps)
    if dag.optional_ratio == 0.0            { det += 10.0; }
    // Explicit timeouts — deterministic contracts
    if dag.has_timeout_constraints          { det +=  8.0; sin += 4.0; }
    // Strict governance → prefer predictable
    det += gov.strictness_score * 12.0;

    // ── SingleAgent signals ───────────────────────────────────────────────────
    if dag.step_count >= 2 && dag.step_count <= 5 { sin += 25.0; }
    if dag.distinct_agents == 1             { sin += 20.0; }
    if dag.max_parallel_width <= 2          { sin += 15.0; }
    if dag.optional_ratio < 0.3             { sin += 10.0; }
    if dag.critical_path_length <= 4        { sin +=  8.0; }
    // Light branching = single agent with routing logic
    if dag.has_branching && dag.distinct_agents == 1 { sin += 12.0; }

    // ── MultiAgent signals ────────────────────────────────────────────────────
    if dag.step_count >= 3                  { mul += 15.0; }
    if dag.step_count >= 5                  { mul += 10.0; }
    // Multiple distinct agents is the strongest multiagent signal
    if dag.distinct_agents >= 2             { mul += 30.0; }
    if dag.distinct_agents >= 4             { mul += 15.0; }
    // Real parallelism available
    if dag.max_parallel_width >= 2          { mul += 20.0; }
    if dag.max_parallel_width >= 4          { mul += 10.0; }
    // Error tolerance → safe to distribute
    if dag.optional_ratio >= 0.3            { mul += 12.0; }
    // Fan-in/branching = coordination patterns
    if dag.has_fan_in                       { mul +=  8.0; }
    if dag.has_branching                    { mul +=  5.0; }
    // High step count drives parallelism value
    if dag.critical_path_length >= 3 && dag.max_parallel_width >= 2 {
        mul += 10.0;  // wide DAG is multi-agent territory
    }

    // Governance hard blocks: if agents are blocked → don't allow multi-agent
    if !gov.all_agents_allowed {
        mul  = (mul  * 0.2).max(0.0);  // heavily penalise
        sin  = (sin  * 0.5).max(0.0);
        det  += 20.0;                    // steer toward deterministic (lowest risk)
    }

    ArchitectureScores {
        deterministic: det.min(100.0).max(0.0),
        single_agent:  sin.min(100.0).max(0.0),
        multi_agent:   mul.min(100.0).max(0.0),
    }
}

// ── Recommended configuration ─────────────────────────────────────────────────

/// Actionable configuration derived from the architecture decision.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RecommendedConfig {
    /// Steps that are eligible for automatic retry on failure.
    pub retry_eligible_steps: Vec<String>,
    /// Maximum concurrent agents that should run simultaneously.
    pub max_concurrency: usize,
    /// Whether to abort the whole workflow on first step failure.
    pub fail_fast: bool,
    /// Parallel execution groups — each inner Vec can start simultaneously.
    pub parallel_groups: Vec<Vec<String>>,
    /// The critical path steps (optimise these for latency).
    pub critical_path: Vec<String>,
    /// Estimated execution phases: number of sequential "rounds" needed.
    pub phase_count: usize,
    /// Steps the engine recommends skipping if governance denies them.
    pub governance_skip_candidates: Vec<String>,
    /// Suggested timeout for the full workflow (sum of critical path step timeouts).
    pub suggested_total_timeout_secs: Option<u64>,
}

fn build_config(
    arch:  &ArchitectureType,
    dag:   &DagMetrics,
    def:   &WorkflowDef,
    gov:   &GovernanceCheck,
) -> RecommendedConfig {
    // Retry-eligible: optional steps + steps not on the critical path
    let cp_set: HashSet<&str> = dag.critical_path.iter().map(|s| s.as_str()).collect();
    let retry_eligible: Vec<String> = def.steps.iter()
        .filter(|s| s.optional || !cp_set.contains(s.step_id.as_str()))
        .map(|s| s.step_id.clone())
        .collect();

    // Max concurrency by architecture
    let max_concurrency = match arch {
        ArchitectureType::Deterministic => 1,
        ArchitectureType::SingleAgent   => 1,
        ArchitectureType::MultiAgent    => dag.max_parallel_width.min(8),
    };

    // fail_fast: deterministic always; single-agent if zero optional; multi-agent never
    let fail_fast = match arch {
        ArchitectureType::Deterministic => true,
        ArchitectureType::SingleAgent   => dag.optional_ratio == 0.0,
        ArchitectureType::MultiAgent    => false,
    };

    // Governance skip candidates: blocked agents that are optional
    let blocked_set: HashSet<&str> = gov.blocked_agents.iter().map(|s| s.as_str()).collect();
    let governance_skip_candidates: Vec<String> = def.steps.iter()
        .filter(|s| s.optional && blocked_set.contains(s.agent_id.as_str()))
        .map(|s| s.step_id.clone())
        .collect();

    // Suggested total timeout: sum of critical-path step timeouts
    let suggested_total_timeout_secs: Option<u64> = {
        let cp_timeouts: Vec<u64> = dag.critical_path.iter()
            .filter_map(|sid| def.steps.iter().find(|s| &s.step_id == sid))
            .filter_map(|s| s.timeout_secs)
            .collect();
        if cp_timeouts.is_empty() { None } else { Some(cp_timeouts.iter().sum()) }
    };

    RecommendedConfig {
        retry_eligible_steps:          retry_eligible,
        max_concurrency,
        fail_fast,
        parallel_groups:               dag.parallel_groups.clone(),
        critical_path:                 dag.critical_path.clone(),
        phase_count:                   dag.parallel_groups.len(),
        governance_skip_candidates,
        suggested_total_timeout_secs,
    }
}

// ── Decision ──────────────────────────────────────────────────────────────────

/// The complete architecture decision with full audit trail.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ArchitectureDecision {
    /// The selected architecture.
    pub architecture: ArchitectureType,
    /// Confidence in the selection (0.0–1.0).
    pub confidence: f32,
    /// Raw scores for all three architectures.
    pub scores: ArchitectureScores,
    /// DAG structure metrics.
    pub dag: DagMetrics,
    /// Governance analysis for the tenant.
    pub governance: GovernanceCheck,
    /// Human-readable reasoning chain (ordered, most-decisive first).
    pub reasoning: Vec<String>,
    /// Actionable configuration for the selected architecture.
    pub config: RecommendedConfig,
    /// Unix timestamp of this decision.
    pub decided_at: u64,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// The stateless architecture selector.
pub struct ArchitectureSelector {
    pub base_dir: PathBuf,
}

impl ArchitectureSelector {
    pub fn new(base_dir: PathBuf) -> Self { Self { base_dir } }

    /// Analyse a workflow definition and return an architecture decision.
    ///
    /// `node_region` is the region this node belongs to (from `NodeConfig`).
    pub fn select(
        &self,
        def:         &WorkflowDef,
        node_region: &str,
    ) -> ArchitectureDecision {
        // 1. DAG analysis
        let dag = analyse_dag(def);

        // 2. Governance check
        let gov = check_governance(&self.base_dir, &def.tenant_id, def, node_region);

        // 3. Score
        let scores = score(&dag, &gov);

        // 4. Pick winner
        let (arch, top_score) = if scores.deterministic >= scores.single_agent
            && scores.deterministic >= scores.multi_agent
        {
            (ArchitectureType::Deterministic, scores.deterministic)
        } else if scores.single_agent >= scores.multi_agent {
            (ArchitectureType::SingleAgent, scores.single_agent)
        } else {
            (ArchitectureType::MultiAgent, scores.multi_agent)
        };

        // 5. Confidence = winner / (sum of all scores), or 1.0 if only one option
        let total = scores.deterministic + scores.single_agent + scores.multi_agent;
        let confidence = if total == 0.0 { 1.0 } else { top_score / total };

        // 6. Reasoning chain
        let reasoning = build_reasoning(&arch, &dag, &gov, &scores);

        // 7. Recommended config
        let config = build_config(&arch, &dag, def, &gov);

        ArchitectureDecision {
            architecture: arch,
            confidence,
            scores,
            dag,
            governance: gov,
            reasoning,
            config,
            decided_at: now_unix(),
        }
    }
}

// ── Reasoning chain ───────────────────────────────────────────────────────────

fn build_reasoning(
    arch:   &ArchitectureType,
    dag:    &DagMetrics,
    gov:    &GovernanceCheck,
    scores: &ArchitectureScores,
) -> Vec<String> {
    let mut reasons: Vec<(i32, String)> = Vec::new();  // (weight, reason)

    // DAG topology
    if dag.step_count == 1 {
        reasons.push((90, "Single step — deterministic execution is guaranteed".to_string()));
    } else if dag.step_count <= 3 {
        reasons.push((60, format!(
            "{} steps — lightweight workflow favors single-agent simplicity", dag.step_count
        )));
    } else {
        reasons.push((60, format!(
            "{} steps — multi-step workflow opens parallelism opportunities", dag.step_count
        )));
    }

    // Agent diversity
    if dag.distinct_agents == 1 {
        reasons.push((80, "All steps use the same agent — single-agent or deterministic model appropriate".to_string()));
    } else {
        reasons.push((85, format!(
            "{} distinct agents required — multi-agent coordination needed", dag.distinct_agents
        )));
    }

    // Parallelism
    if dag.max_parallel_width >= 3 {
        reasons.push((75, format!(
            "Up to {} steps can execute simultaneously — multi-agent parallelism yields major speedup",
            dag.max_parallel_width
        )));
    } else if dag.max_parallel_width == 2 {
        reasons.push((50, "Limited parallelism (2 steps) — marginal benefit from multi-agent".to_string()));
    } else {
        reasons.push((40, "Purely sequential DAG — no parallelism benefit".to_string()));
    }

    // Error tolerance
    if dag.optional_ratio == 0.0 {
        reasons.push((70, "Zero error tolerance (no optional steps) — deterministic or single-agent preferred".to_string()));
    } else if dag.optional_ratio >= 0.5 {
        reasons.push((65, format!(
            "{:.0}% of steps are optional — high error tolerance suits multi-agent distribution",
            dag.optional_ratio * 100.0
        )));
    }

    // Governance
    if gov.strictness_score >= 0.6 {
        reasons.push((72, format!(
            "High governance strictness ({:.0}%) — deterministic execution reduces compliance surface",
            gov.strictness_score * 100.0
        )));
    } else if gov.strictness_score < 0.2 {
        reasons.push((45, "Open governance — all architecture types viable".to_string()));
    }

    if !gov.all_agents_allowed {
        reasons.push((95, format!(
            "Policy blocks {} agent(s): {} — architecture constrained by governance",
            gov.blocked_agents.len(),
            gov.blocked_agents.join(", ")
        )));
    }

    // Timeout contracts
    if dag.has_timeout_constraints {
        reasons.push((55, "Explicit step timeouts — deterministic execution budget enforced".to_string()));
    }

    // Critical path length
    if dag.critical_path_length >= 5 {
        reasons.push((50, format!(
            "Critical path spans {} steps — multi-agent parallelism can reduce wall-clock time",
            dag.critical_path_length
        )));
    }

    // Score margin
    let margin = top_two_margin(scores);
    if margin > 30.0 {
        reasons.push((35, format!(
            "Clear winner: {} scores {:.0} points ahead of next option",
            arch, margin
        )));
    } else if margin < 10.0 {
        reasons.push((30, "Decision is close — consider upgrading to a more complex architecture as load grows".to_string()));
    }

    // Sort by weight descending, take top 6
    reasons.sort_by(|a, b| b.0.cmp(&a.0));
    reasons.into_iter().take(6).map(|(_, r)| r).collect()
}

fn top_two_margin(s: &ArchitectureScores) -> f32 {
    let mut vals = [s.deterministic, s.single_agent, s.multi_agent];
    vals.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    vals[0] - vals[1]
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

// ── Convenience: classify without a saved workflow ────────────────────────────

/// Quick heuristic decision without a full `WorkflowDef` — useful for
/// lightweight clients that just want guidance before building a workflow.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QuickClassifyRequest {
    pub tenant_id:       String,
    /// Estimated number of distinct tools/agents needed.
    pub tool_count:      usize,
    /// Expected number of parallel branches (0 = fully sequential).
    pub parallel_branches: usize,
    /// Error tolerance level: 0 = none, 1 = low, 2 = medium, 3 = high.
    pub error_tolerance: u8,
    /// Whether governance is strict (audit, residency, model constraints).
    pub governance_strict: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct QuickClassifyResponse {
    pub architecture:  ArchitectureType,
    pub confidence:    f32,
    pub one_line:      String,
    pub reasoning:     Vec<String>,
}

pub fn quick_classify(req: &QuickClassifyRequest) -> QuickClassifyResponse {
    // Synthesize signals without a full DAG
    let mut det = 0.0f32;
    let mut sin = 0.0f32;
    let mut mul = 0.0f32;

    // Tool count
    if req.tool_count <= 1         { det += 40.0; }
    if req.tool_count == 1         { sin += 10.0; }
    if req.tool_count == 2         { sin += 30.0; }
    if req.tool_count >= 3         { mul += 30.0; sin += 15.0; }
    if req.tool_count >= 5         { mul += 20.0; }

    // Parallelism
    if req.parallel_branches == 0  { det += 20.0; sin += 10.0; }
    if req.parallel_branches == 1  { sin += 25.0; }
    if req.parallel_branches >= 2  { mul += 35.0; }
    if req.parallel_branches >= 4  { mul += 15.0; }

    // Error tolerance (0=none, 1=low, 2=med, 3=high)
    match req.error_tolerance {
        0 => { det += 20.0; }
        1 => { sin += 15.0; }
        2 => { sin += 8.0; mul += 12.0; }
        _ => { mul += 25.0; }
    }

    // Governance
    if req.governance_strict       { det += 15.0; }

    // Normalise
    det = det.min(100.0);
    sin = sin.min(100.0);
    mul = mul.min(100.0);

    let total = det + sin + mul;
    let (arch, top) = if det >= sin && det >= mul {
        (ArchitectureType::Deterministic, det)
    } else if sin >= mul {
        (ArchitectureType::SingleAgent, sin)
    } else {
        (ArchitectureType::MultiAgent, mul)
    };

    let confidence = if total == 0.0 { 1.0 } else { top / total };

    let one_line = match &arch {
        ArchitectureType::Deterministic =>
            "Sequential, single-agent execution — fastest and most predictable".to_string(),
        ArchitectureType::SingleAgent =>
            "One agent with multiple tool steps — balanced complexity and control".to_string(),
        ArchitectureType::MultiAgent =>
            "Parallel multi-agent coordination — maximum throughput and fault tolerance".to_string(),
    };

    let mut reasoning = Vec::new();
    if req.tool_count >= 3 {
        reasoning.push(format!("{} tools → multi-agent specialisation valuable", req.tool_count));
    }
    if req.parallel_branches >= 2 {
        reasoning.push(format!("{} parallel branches → multi-agent parallelism pays off", req.parallel_branches));
    }
    if req.error_tolerance == 0 {
        reasoning.push("Zero error tolerance → deterministic single path".to_string());
    }
    if req.governance_strict {
        reasoning.push("Strict governance → deterministic reduces compliance surface".to_string());
    }
    if req.tool_count == 1 && req.parallel_branches == 0 {
        reasoning.push("Single tool, sequential → deterministic is simplest and safest".to_string());
    }

    QuickClassifyResponse { architecture: arch, confidence, one_line, reasoning }
}
