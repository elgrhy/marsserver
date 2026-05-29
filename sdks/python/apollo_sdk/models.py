"""
Pydantic models for the Apollo AI Agent Platform v2.2 REST API.

These models mirror the server-side Rust structs serialized to/from JSON.
All fields use snake_case, matching the Apollo node's serde defaults.
"""

from __future__ import annotations

from typing import Any, Dict, List, Optional, Union
from pydantic import BaseModel, Field


# ---------------------------------------------------------------------------
# Core / Agent types
# ---------------------------------------------------------------------------

class AgentRuntimeConfig(BaseModel):
    """Runtime configuration for an agent package."""
    kind: str = Field(..., alias="type", description="Runtime type: python3, node, go, deno, etc.")
    entry: str = Field(..., description="Entry-point file relative to package root.")
    env: Optional[Dict[str, str]] = None
    command: Optional[str] = Field(None, description="Override launch command template.")

    model_config = {"populate_by_name": True}


class AgentLLMConfig(BaseModel):
    required: bool
    provider: str
    fallback: bool


class AgentResourceLimits(BaseModel):
    cpu: float
    memory: str
    timeout: int


class AgentPermissionConfig(BaseModel):
    network: str
    filesystem: str
    processes: str


class AgentCompatibility(BaseModel):
    os: List[str]
    arch: List[str]


class RestartPolicy(BaseModel):
    max_restarts: int
    window_secs: int


class VolumeSpec(BaseModel):
    name: str
    size: str = ""


class AgentSpec(BaseModel):
    """Full agent specification as defined in agent.yaml."""
    name: str
    version: str
    runtime: AgentRuntimeConfig
    llm: AgentLLMConfig
    capabilities: List[str] = Field(default_factory=list)
    triggers: List[str] = Field(default_factory=list)
    resources: AgentResourceLimits
    permissions: AgentPermissionConfig
    compatibility: AgentCompatibility
    restart_policy: Optional[RestartPolicy] = None
    volumes: List[VolumeSpec] = Field(default_factory=list)


class AgentRecord(BaseModel):
    """Registered agent record returned by /agents/list."""
    id: str
    spec: AgentSpec
    checksum: str
    created_at: int
    prev_version: Optional[str] = None


class ExecutionStats(BaseModel):
    cpu_usage_pct: float = 0.0
    memory_mb: int = 0
    uptime_secs: int = 0
    restart_count: int = 0
    last_restart: int = 0
    is_failed: bool = False


class AgentInstance(BaseModel):
    """A running agent instance."""
    id: str
    agent_id: str
    tenant_id: str
    status: str
    pid: Optional[int] = None
    port: Optional[int] = None
    stats: ExecutionStats = Field(default_factory=ExecutionStats)
    created_at: int = 0


# ---------------------------------------------------------------------------
# Policy / Governance
# ---------------------------------------------------------------------------

class ModelPolicy(BaseModel):
    allowed_models: List[str] = Field(default_factory=list)
    require_local: bool = False


class TenantPolicy(BaseModel):
    """Per-tenant governance policy."""
    tenant_id: str = ""
    max_instances: Optional[int] = None
    allowed_agents: Optional[List[str]] = None
    blocked_agents: Optional[List[str]] = None
    allowed_tools: Optional[List[str]] = None
    blocked_tools: Optional[List[str]] = None
    data_residency: Optional[str] = None
    max_tokens_per_day: Optional[int] = None
    model_policy: Optional[ModelPolicy] = None
    require_audit: bool = False
    blocked_env_keys: List[str] = Field(default_factory=list)
    forced_env: Dict[str, str] = Field(default_factory=dict)
    updated_at: int = 0


class ComplianceReport(BaseModel):
    tenant_id: str
    score: float
    checks: Dict[str, Any] = Field(default_factory=dict)
    violations: List[str] = Field(default_factory=list)
    generated_at: int = 0


# ---------------------------------------------------------------------------
# Tracing / Observability
# ---------------------------------------------------------------------------

class TokenUsage(BaseModel):
    model: str
    input_tokens: int = 0
    output_tokens: int = 0
    cost_usd: float = 0.0
    provider: str = ""


class TraceSpan(BaseModel):
    """A single execution span posted by an agent."""
    span_id: str = ""
    trace_id: str = ""
    tenant_id: str = ""
    agent_id: str = ""
    name: str
    status: str = "ok"
    start_ts_ms: int
    end_ts_ms: int
    token_usage: Optional[TokenUsage] = None
    metadata: Optional[Dict[str, Any]] = None
    parent_span_id: Optional[str] = None


class TraceSummary(BaseModel):
    trace_id: str
    tenant_id: str
    agent_id: str
    span_count: int = 0
    total_tokens: int = 0
    total_cost_usd: float = 0.0
    started_at: int = 0
    ended_at: int = 0
    status: str = "active"


class TenantTokenStats(BaseModel):
    tenant_id: str
    total_input_tokens: int = 0
    total_output_tokens: int = 0
    total_cost_usd: float = 0.0
    by_model: Dict[str, Any] = Field(default_factory=dict)


# ---------------------------------------------------------------------------
# Health Intelligence
# ---------------------------------------------------------------------------

class ResourceSample(BaseModel):
    ts: int
    cpu_pct: float
    memory_mb: int


class CrashEvent(BaseModel):
    ts: int
    exit_code: Optional[int] = None
    reason: str = ""


class HealthRecord(BaseModel):
    tenant_id: str
    agent_id: str
    score: float = 100.0
    crash_count: int = 0
    crash_pattern: Optional[str] = None
    last_crash: Optional[CrashEvent] = None
    resource_trend: str = "stable"
    samples: List[ResourceSample] = Field(default_factory=list)
    updated_at: int = 0


class FleetHealthSummary(BaseModel):
    healthy_count: int = 0
    degraded_count: int = 0
    critical_count: int = 0
    avg_score: float = 100.0
    by_tenant: Dict[str, Any] = Field(default_factory=dict)


# ---------------------------------------------------------------------------
# Memory Layer
# ---------------------------------------------------------------------------

class MemoryEntry(BaseModel):
    key: str
    value: Any
    tags: List[str] = Field(default_factory=list)
    text: Optional[str] = None
    created_at: int = 0
    updated_at: int = 0
    ttl_secs: Optional[int] = None


class MemoryQuery(BaseModel):
    query: str
    tags: Optional[List[str]] = None
    limit: int = 10


class MemoryStats(BaseModel):
    tenant_id: str
    agent_id: str
    entry_count: int = 0
    total_bytes: int = 0


# ---------------------------------------------------------------------------
# Model Routing
# ---------------------------------------------------------------------------

class ModelRecord(BaseModel):
    """A registered LLM model with routing metadata."""
    model_id: str
    provider: str
    cost_per_m_input: float = 0.0
    cost_per_m_output: float = 0.0
    latency_p50_ms: int = 0
    latency_p99_ms: int = 0
    throughput_tok_s: int = 0
    capabilities: List[str] = Field(default_factory=list)
    context_window: int = 0
    is_local: bool = False
    is_available: bool = True
    priority: int = 0


class RoutingRequest(BaseModel):
    tenant_id: str
    input_tokens: int = 0
    output_tokens: int = 0
    max_cost_usd: Optional[float] = None
    required_capabilities: List[str] = Field(default_factory=list)
    prefer_local: bool = False


class RoutingDecision(BaseModel):
    selected_model: str
    reason: str
    estimated_cost_usd: float = 0.0
    estimated_latency_ms: int = 0


class ModelFeedback(BaseModel):
    model_id: str
    latency_ms: int
    is_success: bool


class ModelUsageRecord(BaseModel):
    model_id: str
    input_tokens: int
    output_tokens: int
    cost_usd: float
    latency_ms: int


# ---------------------------------------------------------------------------
# Scheduler
# ---------------------------------------------------------------------------

class ScheduleCron(BaseModel):
    type: str = "cron"
    expression: str


class ScheduleInterval(BaseModel):
    type: str = "interval"
    secs: int


class ScheduleOnce(BaseModel):
    type: str = "once"
    at: int


ScheduleDefinition = Union[ScheduleCron, ScheduleInterval, ScheduleOnce]


class ScheduledJob(BaseModel):
    """A scheduled agent job."""
    id: str = ""
    name: str
    tenant_id: str
    agent_id: str
    extra_env: Dict[str, str] = Field(default_factory=dict)
    schedule: Dict[str, Any]  # flexible to accept any schedule variant
    enabled: bool = True
    created_at: int = 0
    next_run: Optional[int] = None
    last_run: Optional[int] = None


class JobRunRecord(BaseModel):
    job_id: str
    fired_at: int
    status: str = "ok"
    error: Optional[str] = None


# ---------------------------------------------------------------------------
# Orchestration: Blueprints, Groups, Workflows
# ---------------------------------------------------------------------------

class Blueprint(BaseModel):
    """Agent deployment blueprint / template."""
    id: str = ""
    name: str
    description: str = ""
    agent_id: str
    pin_version: Optional[str] = None
    default_env: Dict[str, str] = Field(default_factory=dict)
    cpu_override: Optional[float] = None
    memory_override: Optional[str] = None
    timeout_override: Optional[int] = None
    tags: List[str] = Field(default_factory=list)
    region: Optional[str] = None
    created_at: int = 0
    updated_at: int = 0


class GroupMember(BaseModel):
    agent_id: str
    tenant_override: Optional[str] = None


class AgentGroup(BaseModel):
    """Named collection of agents managed as a unit."""
    id: str = ""
    name: str
    tenant_id: str
    members: List[GroupMember] = Field(default_factory=list)
    status: str = "stopped"
    created_at: int = 0
    updated_at: int = 0


class WorkflowStep(BaseModel):
    step_id: str
    name: str
    agent_id: str
    depends_on: List[str] = Field(default_factory=list)
    env: Dict[str, str] = Field(default_factory=dict)
    optional: bool = False
    timeout_secs: Optional[int] = None


class WorkflowDef(BaseModel):
    """Workflow DAG definition."""
    id: str = ""
    name: str
    description: str = ""
    tenant_id: str
    steps: List[WorkflowStep] = Field(default_factory=list)
    created_at: int = 0
    updated_at: int = 0


class WorkflowStepRun(BaseModel):
    step_id: str
    status: str = "pending"
    started_at: Optional[int] = None
    ended_at: Optional[int] = None
    error: Optional[str] = None


class WorkflowRun(BaseModel):
    run_id: str
    workflow_id: str
    tenant_id: str
    status: str = "pending"
    steps: List[WorkflowStepRun] = Field(default_factory=list)
    started_at: int = 0
    ended_at: Optional[int] = None


# ---------------------------------------------------------------------------
# Architecture Selection
# ---------------------------------------------------------------------------

class QuickClassifyRequest(BaseModel):
    tenant_id: str
    tool_count: int = 0
    parallel_branches: int = 0
    error_tolerance: int = 0
    governance_strict: bool = False


class ArchitectureDecision(BaseModel):
    architecture: str  # "deterministic" | "single_agent" | "multi_agent"
    confidence: float
    scores: Dict[str, float] = Field(default_factory=dict)
    reasoning: List[str] = Field(default_factory=list)
    dag: Optional[Dict[str, Any]] = None
    governance: Optional[Dict[str, Any]] = None
    config: Optional[Dict[str, Any]] = None


# ---------------------------------------------------------------------------
# Alerts (v2.2 extension)
# ---------------------------------------------------------------------------

class AlertCondition(BaseModel):
    metric: str
    operator: str  # "gt" | "lt" | "eq" | "gte" | "lte"
    threshold: float


class AlertAction(BaseModel):
    type: str  # "webhook" | "log" | "restart"
    target: Optional[str] = None


class AlertRule(BaseModel):
    """An alert rule that fires when a condition is met."""
    id: str = ""
    name: str
    tenant_id: str
    agent_id: Optional[str] = None
    condition: AlertCondition
    actions: List[AlertAction] = Field(default_factory=list)
    enabled: bool = True
    cooldown_secs: int = 300
    created_at: int = 0
    last_fired: Optional[int] = None


class AlertEvent(BaseModel):
    rule_id: str
    fired_at: int
    metric_value: float
    message: str


# ---------------------------------------------------------------------------
# Message Bus (v2.2 extension)
# ---------------------------------------------------------------------------

class BusMessage(BaseModel):
    """A message on the per-agent event bus."""
    id: str = ""
    channel: str
    tenant_id: str
    agent_id: str
    payload: Any
    published_at: int = 0
    expires_at: Optional[int] = None


# ---------------------------------------------------------------------------
# Usage metering
# ---------------------------------------------------------------------------

class UsageRecord(BaseModel):
    tenant_id: str
    cpu_seconds: float = 0.0
    memory_gb_seconds: float = 0.0
    agent_starts: int = 0
    updated_at: int = 0
