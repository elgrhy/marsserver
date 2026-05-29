/**
 * TypeScript interfaces for the Apollo AI Agent Platform v2.2 REST API.
 *
 * All field names use snake_case matching the Apollo server's serde defaults.
 */

// ---------------------------------------------------------------------------
// Core / Agent types
// ---------------------------------------------------------------------------

export interface AgentRuntimeConfig {
  /** Runtime type: python3 | node | go | deno | bun | ruby | gx | rust | shell | custom */
  type: string;
  /** Entry-point file relative to package root. */
  entry: string;
  env?: Record<string, string>;
  /** Override launch command template. */
  command?: string;
}

export interface AgentLLMConfig {
  required: boolean;
  provider: string;
  fallback: boolean;
}

export interface AgentResourceLimits {
  cpu: number;
  memory: string;
  timeout: number;
}

export interface AgentPermissionConfig {
  network: string;
  filesystem: string;
  processes: string;
}

export interface AgentCompatibility {
  os: string[];
  arch: string[];
}

export interface RestartPolicy {
  max_restarts: number;
  window_secs: number;
}

export interface VolumeSpec {
  name: string;
  size?: string;
}

/** Full agent specification as defined in agent.yaml. */
export interface AgentSpec {
  name: string;
  version: string;
  runtime: AgentRuntimeConfig;
  llm: AgentLLMConfig;
  capabilities: string[];
  triggers: string[];
  resources: AgentResourceLimits;
  permissions: AgentPermissionConfig;
  compatibility: AgentCompatibility;
  restart_policy?: RestartPolicy;
  volumes: VolumeSpec[];
}

/** Registered agent record returned by /agents/list. */
export interface AgentRecord {
  id: string;
  spec: AgentSpec;
  checksum: string;
  created_at: number;
  prev_version?: string;
}

export interface ExecutionStats {
  cpu_usage_pct: number;
  memory_mb: number;
  uptime_secs: number;
  restart_count: number;
  last_restart: number;
  is_failed: boolean;
}

/** A running agent instance. */
export interface AgentInstance {
  id: string;
  agent_id: string;
  tenant_id: string;
  status: string;
  pid?: number;
  port?: number;
  stats: ExecutionStats;
  created_at: number;
}

// ---------------------------------------------------------------------------
// Policy / Governance
// ---------------------------------------------------------------------------

export interface ModelPolicy {
  allowed_models: string[];
  require_local: boolean;
}

/** Per-tenant governance policy. */
export interface TenantPolicy {
  tenant_id?: string;
  max_instances?: number;
  allowed_agents?: string[];
  blocked_agents?: string[];
  allowed_tools?: string[];
  blocked_tools?: string[];
  data_residency?: string;
  max_tokens_per_day?: number;
  model_policy?: ModelPolicy;
  require_audit?: boolean;
  blocked_env_keys?: string[];
  forced_env?: Record<string, string>;
  updated_at?: number;
}

export interface ComplianceReport {
  tenant_id: string;
  score: number;
  checks: Record<string, unknown>;
  violations: string[];
  generated_at: number;
}

// ---------------------------------------------------------------------------
// Tracing / Observability
// ---------------------------------------------------------------------------

export interface TokenUsage {
  model: string;
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
  provider: string;
}

/** A single execution span posted by an agent. */
export interface TraceSpan {
  span_id?: string;
  trace_id?: string;
  tenant_id: string;
  agent_id: string;
  name: string;
  status?: string;
  start_ts_ms: number;
  end_ts_ms: number;
  token_usage?: TokenUsage;
  metadata?: Record<string, unknown>;
  parent_span_id?: string;
}

export interface SpanPostResponse {
  span_id: string;
  trace_id: string;
}

export interface TraceSummary {
  trace_id: string;
  tenant_id: string;
  agent_id: string;
  span_count: number;
  total_tokens: number;
  total_cost_usd: number;
  started_at: number;
  ended_at: number;
  status: string;
}

export interface TenantTokenStats {
  tenant_id: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_usd: number;
  by_model: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Health Intelligence
// ---------------------------------------------------------------------------

export interface ResourceSample {
  ts: number;
  cpu_pct: number;
  memory_mb: number;
}

export interface CrashEvent {
  ts: number;
  exit_code?: number;
  reason: string;
}

export interface HealthRecord {
  tenant_id: string;
  agent_id: string;
  score: number;
  crash_count: number;
  crash_pattern?: string;
  last_crash?: CrashEvent;
  resource_trend: string;
  samples: ResourceSample[];
  updated_at: number;
}

export interface FleetHealthSummary {
  healthy_count: number;
  degraded_count: number;
  critical_count: number;
  avg_score: number;
  by_tenant: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Memory Layer
// ---------------------------------------------------------------------------

export interface MemoryEntry {
  key: string;
  value: unknown;
  tags: string[];
  text?: string;
  created_at: number;
  updated_at: number;
  ttl_secs?: number;
}

export interface MemoryQuery {
  query: string;
  tags?: string[];
  limit?: number;
}

export interface MemoryStats {
  tenant_id: string;
  agent_id: string;
  entry_count: number;
  total_bytes: number;
}

export interface MemoryListResponse {
  keys: string[];
}

export interface MemoryPutBody {
  value: unknown;
  tags?: string[];
  text?: string;
  ttl_secs?: number;
}

// ---------------------------------------------------------------------------
// Model Routing
// ---------------------------------------------------------------------------

/** A registered LLM model with routing metadata. */
export interface ModelRecord {
  model_id: string;
  provider: string;
  cost_per_m_input: number;
  cost_per_m_output: number;
  latency_p50_ms: number;
  latency_p99_ms: number;
  throughput_tok_s: number;
  capabilities: string[];
  context_window: number;
  is_local: boolean;
  is_available: boolean;
  priority: number;
}

export interface RoutingRequest {
  tenant_id: string;
  input_tokens?: number;
  output_tokens?: number;
  max_cost_usd?: number;
  required_capabilities?: string[];
  prefer_local?: boolean;
}

export interface RoutingDecision {
  selected_model: string;
  reason: string;
  estimated_cost_usd: number;
  estimated_latency_ms: number;
}

export interface ModelFeedback {
  model_id: string;
  latency_ms: number;
  is_success: boolean;
}

export interface ModelUsageRecord {
  model_id: string;
  input_tokens: number;
  output_tokens: number;
  cost_usd: number;
  latency_ms: number;
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

export interface ScheduleCron {
  type: 'cron';
  expression: string;
}

export interface ScheduleInterval {
  type: 'interval';
  secs: number;
}

export interface ScheduleOnce {
  type: 'once';
  at: number;
}

export type ScheduleDefinition = ScheduleCron | ScheduleInterval | ScheduleOnce;

/** A scheduled agent job. */
export interface ScheduledJob {
  id?: string;
  name: string;
  tenant_id: string;
  agent_id: string;
  extra_env?: Record<string, string>;
  schedule: ScheduleDefinition;
  enabled: boolean;
  created_at?: number;
  next_run?: number;
  last_run?: number;
}

export interface JobRunRecord {
  job_id: string;
  fired_at: number;
  status: string;
  error?: string;
}

// ---------------------------------------------------------------------------
// Orchestration: Blueprints, Groups, Workflows
// ---------------------------------------------------------------------------

/** Agent deployment blueprint / template. */
export interface Blueprint {
  id?: string;
  name: string;
  description?: string;
  agent_id: string;
  pin_version?: string;
  default_env?: Record<string, string>;
  cpu_override?: number;
  memory_override?: string;
  timeout_override?: number;
  tags?: string[];
  region?: string;
  created_at?: number;
  updated_at?: number;
}

export interface GroupMember {
  agent_id: string;
  tenant_override?: string;
}

/** Named collection of agents managed as a unit. */
export interface AgentGroup {
  id?: string;
  name: string;
  tenant_id: string;
  members: GroupMember[];
  status?: string;
  created_at?: number;
  updated_at?: number;
}

export interface WorkflowStep {
  step_id: string;
  name: string;
  agent_id: string;
  depends_on: string[];
  env?: Record<string, string>;
  optional?: boolean;
  timeout_secs?: number;
}

/** Workflow DAG definition. */
export interface WorkflowDef {
  id?: string;
  name: string;
  description?: string;
  tenant_id: string;
  steps: WorkflowStep[];
  created_at?: number;
  updated_at?: number;
}

export interface WorkflowStepRun {
  step_id: string;
  status: string;
  started_at?: number;
  ended_at?: number;
  error?: string;
}

export interface WorkflowRun {
  run_id: string;
  workflow_id: string;
  tenant_id: string;
  status: string;
  steps: WorkflowStepRun[];
  started_at: number;
  ended_at?: number;
}

// ---------------------------------------------------------------------------
// Architecture Selection
// ---------------------------------------------------------------------------

export interface QuickClassifyRequest {
  tenant_id: string;
  tool_count?: number;
  parallel_branches?: number;
  error_tolerance?: number;
  governance_strict?: boolean;
}

export interface ArchitectureDecision {
  /** "deterministic" | "single_agent" | "multi_agent" */
  architecture: string;
  /** Confidence score 0.0–1.0 */
  confidence: number;
  scores: Record<string, number>;
  reasoning: string[];
  dag?: Record<string, unknown>;
  governance?: Record<string, unknown>;
  config?: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Alerts (v2.2 extension)
// ---------------------------------------------------------------------------

export interface AlertCondition {
  metric: string;
  /** "gt" | "lt" | "eq" | "gte" | "lte" */
  operator: string;
  threshold: number;
}

export interface AlertAction {
  /** "webhook" | "log" | "restart" */
  type: string;
  target?: string;
}

/** An alert rule that fires when a condition is met. */
export interface AlertRule {
  id?: string;
  name: string;
  tenant_id: string;
  agent_id?: string;
  condition: AlertCondition;
  actions: AlertAction[];
  enabled: boolean;
  cooldown_secs?: number;
  created_at?: number;
  last_fired?: number;
}

export interface AlertEvent {
  rule_id: string;
  fired_at: number;
  metric_value: number;
  message: string;
}

// ---------------------------------------------------------------------------
// Message Bus (v2.2 extension)
// ---------------------------------------------------------------------------

/** A message on the per-agent event bus. */
export interface BusMessage {
  id?: string;
  channel: string;
  tenant_id: string;
  agent_id: string;
  payload: unknown;
  published_at?: number;
  expires_at?: number;
}

// ---------------------------------------------------------------------------
// Usage metering
// ---------------------------------------------------------------------------

export interface UsageRecord {
  tenant_id: string;
  cpu_seconds: number;
  memory_gb_seconds: number;
  agent_starts: number;
  updated_at: number;
}

// ---------------------------------------------------------------------------
// Common response shapes
// ---------------------------------------------------------------------------

export interface StatusResponse {
  status: string;
}

export interface NodeMetrics {
  active_agents: number;
  max_agents: number;
  node_id: string;
  region: string;
  version: string;
}

export interface HealthEndpointResponse {
  status: string;
  version: string;
}

export interface PolicyListResponse {
  tenants: string[];
}

export interface GroupRunResponse {
  group_id: string;
  started: string[];
  errors: string[];
}

export interface GroupStopResponse {
  group_id: string;
  stopped: string[];
}

export interface BlueprintDeployResponse {
  status: string;
  blueprint_id: string;
  instance: AgentInstance;
}
