// Package apollo provides a Go client for the Apollo AI Agent Platform v2.2.
//
// All types use json struct tags matching the Apollo server's serde snake_case defaults.
package apollo

// ---------------------------------------------------------------------------
// Core / Agent types
// ---------------------------------------------------------------------------

// AgentRuntimeConfig describes how an agent is launched.
type AgentRuntimeConfig struct {
	// Kind is the runtime type: python3, node, go, deno, bun, etc.
	Kind    string            `json:"type"`
	Entry   string            `json:"entry"`
	Env     map[string]string `json:"env,omitempty"`
	Command *string           `json:"command,omitempty"`
}

// AgentLLMConfig describes the LLM requirements of an agent.
type AgentLLMConfig struct {
	Required bool   `json:"required"`
	Provider string `json:"provider"`
	Fallback bool   `json:"fallback"`
}

// AgentResourceLimits specifies CPU, memory, and timeout constraints.
type AgentResourceLimits struct {
	CPU     float64 `json:"cpu"`
	Memory  string  `json:"memory"`
	Timeout int     `json:"timeout"`
}

// AgentPermissionConfig specifies filesystem, network, and process permissions.
type AgentPermissionConfig struct {
	Network    string `json:"network"`
	Filesystem string `json:"filesystem"`
	Processes  string `json:"processes"`
}

// AgentCompatibility declares OS and architecture requirements.
type AgentCompatibility struct {
	OS   []string `json:"os"`
	Arch []string `json:"arch"`
}

// RestartPolicy controls automatic restart behaviour.
type RestartPolicy struct {
	MaxRestarts int `json:"max_restarts"`
	WindowSecs  int `json:"window_secs"`
}

// VolumeSpec declares a named persistent volume.
type VolumeSpec struct {
	Name string `json:"name"`
	Size string `json:"size,omitempty"`
}

// AgentSpec is the full agent specification as defined in agent.yaml.
type AgentSpec struct {
	Name          string                `json:"name"`
	Version       string                `json:"version"`
	Runtime       AgentRuntimeConfig    `json:"runtime"`
	LLM           AgentLLMConfig        `json:"llm"`
	Capabilities  []string              `json:"capabilities"`
	Triggers      []string              `json:"triggers"`
	Resources     AgentResourceLimits   `json:"resources"`
	Permissions   AgentPermissionConfig `json:"permissions"`
	Compatibility AgentCompatibility    `json:"compatibility"`
	RestartPolicy *RestartPolicy        `json:"restart_policy,omitempty"`
	Volumes       []VolumeSpec          `json:"volumes"`
}

// AgentRecord is a registered agent record returned by /agents/list.
type AgentRecord struct {
	ID          string    `json:"id"`
	Spec        AgentSpec `json:"spec"`
	Checksum    string    `json:"checksum"`
	CreatedAt   int64     `json:"created_at"`
	PrevVersion *string   `json:"prev_version,omitempty"`
}

// ExecutionStats holds runtime metrics for a running agent instance.
type ExecutionStats struct {
	CPUUsagePct float64 `json:"cpu_usage_pct"`
	MemoryMB    int64   `json:"memory_mb"`
	UptimeSecs  int64   `json:"uptime_secs"`
	RestartCount int    `json:"restart_count"`
	LastRestart int64   `json:"last_restart"`
	IsFailed    bool    `json:"is_failed"`
}

// AgentInstance represents a running agent process.
type AgentInstance struct {
	ID        string         `json:"id"`
	AgentID   string         `json:"agent_id"`
	TenantID  string         `json:"tenant_id"`
	Status    string         `json:"status"`
	PID       *int           `json:"pid,omitempty"`
	Port      *int           `json:"port,omitempty"`
	Stats     ExecutionStats `json:"stats"`
	CreatedAt int64          `json:"created_at"`
}

// ---------------------------------------------------------------------------
// Policy / Governance
// ---------------------------------------------------------------------------

// ModelPolicy constrains which LLM models a tenant may use.
type ModelPolicy struct {
	AllowedModels []string `json:"allowed_models"`
	RequireLocal  bool     `json:"require_local"`
}

// TenantPolicy is the per-tenant governance policy.
type TenantPolicy struct {
	TenantID         string            `json:"tenant_id,omitempty"`
	MaxInstances     *int              `json:"max_instances,omitempty"`
	AllowedAgents    []string          `json:"allowed_agents,omitempty"`
	BlockedAgents    []string          `json:"blocked_agents,omitempty"`
	AllowedTools     []string          `json:"allowed_tools,omitempty"`
	BlockedTools     []string          `json:"blocked_tools,omitempty"`
	DataResidency    *string           `json:"data_residency,omitempty"`
	MaxTokensPerDay  *int              `json:"max_tokens_per_day,omitempty"`
	ModelPolicy      *ModelPolicy      `json:"model_policy,omitempty"`
	RequireAudit     bool              `json:"require_audit,omitempty"`
	BlockedEnvKeys   []string          `json:"blocked_env_keys,omitempty"`
	ForcedEnv        map[string]string `json:"forced_env,omitempty"`
	UpdatedAt        int64             `json:"updated_at,omitempty"`
}

// ComplianceReport is the result of a tenant compliance check.
type ComplianceReport struct {
	TenantID    string                 `json:"tenant_id"`
	Score       float64                `json:"score"`
	Checks      map[string]interface{} `json:"checks"`
	Violations  []string               `json:"violations"`
	GeneratedAt int64                  `json:"generated_at"`
}

// ---------------------------------------------------------------------------
// Tracing / Observability
// ---------------------------------------------------------------------------

// TokenUsage records LLM token consumption within a span.
type TokenUsage struct {
	Model        string  `json:"model"`
	InputTokens  int64   `json:"input_tokens"`
	OutputTokens int64   `json:"output_tokens"`
	CostUSD      float64 `json:"cost_usd"`
	Provider     string  `json:"provider"`
}

// TraceSpan is a single execution span posted by an agent.
type TraceSpan struct {
	SpanID       string                 `json:"span_id,omitempty"`
	TraceID      string                 `json:"trace_id,omitempty"`
	TenantID     string                 `json:"tenant_id"`
	AgentID      string                 `json:"agent_id"`
	Name         string                 `json:"name"`
	Status       string                 `json:"status,omitempty"`
	StartTsMs    int64                  `json:"start_ts_ms"`
	EndTsMs      int64                  `json:"end_ts_ms"`
	TokenUsage   *TokenUsage            `json:"token_usage,omitempty"`
	Metadata     map[string]interface{} `json:"metadata,omitempty"`
	ParentSpanID *string                `json:"parent_span_id,omitempty"`
}

// SpanPostResponse is returned after successfully posting a span.
type SpanPostResponse struct {
	SpanID  string `json:"span_id"`
	TraceID string `json:"trace_id"`
}

// TraceSummary summarises a completed or ongoing trace.
type TraceSummary struct {
	TraceID     string  `json:"trace_id"`
	TenantID    string  `json:"tenant_id"`
	AgentID     string  `json:"agent_id"`
	SpanCount   int     `json:"span_count"`
	TotalTokens int64   `json:"total_tokens"`
	TotalCostUSD float64 `json:"total_cost_usd"`
	StartedAt   int64   `json:"started_at"`
	EndedAt     int64   `json:"ended_at"`
	Status      string  `json:"status"`
}

// TenantTokenStats aggregates token usage across all traces for a tenant.
type TenantTokenStats struct {
	TenantID          string                 `json:"tenant_id"`
	TotalInputTokens  int64                  `json:"total_input_tokens"`
	TotalOutputTokens int64                  `json:"total_output_tokens"`
	TotalCostUSD      float64                `json:"total_cost_usd"`
	ByModel           map[string]interface{} `json:"by_model"`
}

// ---------------------------------------------------------------------------
// Health Intelligence
// ---------------------------------------------------------------------------

// ResourceSample is a single CPU/memory reading.
type ResourceSample struct {
	Ts       int64   `json:"ts"`
	CPUPct   float64 `json:"cpu_pct"`
	MemoryMB int64   `json:"memory_mb"`
}

// CrashEvent records a single agent crash.
type CrashEvent struct {
	Ts       int64  `json:"ts"`
	ExitCode *int   `json:"exit_code,omitempty"`
	Reason   string `json:"reason"`
}

// HealthRecord holds the health state for a single agent.
type HealthRecord struct {
	TenantID      string           `json:"tenant_id"`
	AgentID       string           `json:"agent_id"`
	Score         float64          `json:"score"`
	CrashCount    int              `json:"crash_count"`
	CrashPattern  *string          `json:"crash_pattern,omitempty"`
	LastCrash     *CrashEvent      `json:"last_crash,omitempty"`
	ResourceTrend string           `json:"resource_trend"`
	Samples       []ResourceSample `json:"samples"`
	UpdatedAt     int64            `json:"updated_at"`
}

// FleetHealthSummary aggregates health across the entire fleet.
type FleetHealthSummary struct {
	HealthyCount  int                    `json:"healthy_count"`
	DegradedCount int                    `json:"degraded_count"`
	CriticalCount int                    `json:"critical_count"`
	AvgScore      float64                `json:"avg_score"`
	ByTenant      map[string]interface{} `json:"by_tenant"`
}

// ---------------------------------------------------------------------------
// Memory Layer
// ---------------------------------------------------------------------------

// MemoryEntry is a single stored memory record.
type MemoryEntry struct {
	Key       string      `json:"key"`
	Value     interface{} `json:"value"`
	Tags      []string    `json:"tags"`
	Text      *string     `json:"text,omitempty"`
	CreatedAt int64       `json:"created_at"`
	UpdatedAt int64       `json:"updated_at"`
	TTLSecs   *int64      `json:"ttl_secs,omitempty"`
}

// MemoryQuery is the request body for a similarity search.
type MemoryQuery struct {
	Query string   `json:"query"`
	Tags  []string `json:"tags,omitempty"`
	Limit int      `json:"limit,omitempty"`
}

// MemoryPutBody is the request body for storing a memory entry.
type MemoryPutBody struct {
	Value   interface{} `json:"value"`
	Tags    []string    `json:"tags,omitempty"`
	Text    *string     `json:"text,omitempty"`
	TTLSecs *int64      `json:"ttl_secs,omitempty"`
}

// MemoryStats reports the size of an agent's memory store.
type MemoryStats struct {
	TenantID   string `json:"tenant_id"`
	AgentID    string `json:"agent_id"`
	EntryCount int    `json:"entry_count"`
	TotalBytes int64  `json:"total_bytes"`
}

// MemoryListResponse wraps the list of memory keys.
type MemoryListResponse struct {
	Keys []string `json:"keys"`
}

// ---------------------------------------------------------------------------
// Model Routing
// ---------------------------------------------------------------------------

// ModelRecord is a registered LLM model with routing metadata.
type ModelRecord struct {
	ModelID        string   `json:"model_id"`
	Provider       string   `json:"provider"`
	CostPerMInput  float64  `json:"cost_per_m_input"`
	CostPerMOutput float64  `json:"cost_per_m_output"`
	LatencyP50Ms   int      `json:"latency_p50_ms"`
	LatencyP99Ms   int      `json:"latency_p99_ms"`
	ThroughputTokS int      `json:"throughput_tok_s"`
	Capabilities   []string `json:"capabilities"`
	ContextWindow  int      `json:"context_window"`
	IsLocal        bool     `json:"is_local"`
	IsAvailable    bool     `json:"is_available"`
	Priority       int      `json:"priority"`
}

// RoutingRequest specifies constraints for model selection.
type RoutingRequest struct {
	TenantID             string   `json:"tenant_id"`
	InputTokens          int64    `json:"input_tokens,omitempty"`
	OutputTokens         int64    `json:"output_tokens,omitempty"`
	MaxCostUSD           *float64 `json:"max_cost_usd,omitempty"`
	RequiredCapabilities []string `json:"required_capabilities,omitempty"`
	PreferLocal          bool     `json:"prefer_local,omitempty"`
}

// RoutingDecision is the result of a routing request.
type RoutingDecision struct {
	SelectedModel      string  `json:"selected_model"`
	Reason             string  `json:"reason"`
	EstimatedCostUSD   float64 `json:"estimated_cost_usd"`
	EstimatedLatencyMs int     `json:"estimated_latency_ms"`
}

// ModelFeedback reports observed latency for a model invocation.
type ModelFeedback struct {
	ModelID   string `json:"model_id"`
	LatencyMs int64  `json:"latency_ms"`
	IsSuccess bool   `json:"is_success"`
}

// ModelUsageRecord records token usage for billing.
type ModelUsageRecord struct {
	ModelID      string  `json:"model_id"`
	InputTokens  int64   `json:"input_tokens"`
	OutputTokens int64   `json:"output_tokens"`
	CostUSD      float64 `json:"cost_usd"`
	LatencyMs    int64   `json:"latency_ms"`
}

// ---------------------------------------------------------------------------
// Scheduler
// ---------------------------------------------------------------------------

// Schedule is the JSON-tagged schedule variant.
// Use map[string]interface{} or define a concrete type per variant.
// For convenience, helper constructors are provided below.
type Schedule map[string]interface{}

// CronSchedule creates a 5-field cron schedule.
func CronSchedule(expression string) Schedule {
	return Schedule{"type": "cron", "expression": expression}
}

// IntervalSchedule creates a recurring interval schedule.
func IntervalSchedule(secs int64) Schedule {
	return Schedule{"type": "interval", "secs": secs}
}

// OnceSchedule creates a one-shot schedule at a Unix timestamp.
func OnceSchedule(at int64) Schedule {
	return Schedule{"type": "once", "at": at}
}

// ScheduledJob is a scheduled agent invocation.
type ScheduledJob struct {
	ID        string            `json:"id,omitempty"`
	Name      string            `json:"name"`
	TenantID  string            `json:"tenant_id"`
	AgentID   string            `json:"agent_id"`
	ExtraEnv  map[string]string `json:"extra_env,omitempty"`
	Schedule  Schedule          `json:"schedule"`
	Enabled   bool              `json:"enabled"`
	CreatedAt int64             `json:"created_at,omitempty"`
	NextRun   *int64            `json:"next_run,omitempty"`
	LastRun   *int64            `json:"last_run,omitempty"`
}

// JobRunRecord records a single job execution.
type JobRunRecord struct {
	JobID   string  `json:"job_id"`
	FiredAt int64   `json:"fired_at"`
	Status  string  `json:"status"`
	Error   *string `json:"error,omitempty"`
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

// Blueprint is an agent deployment template.
type Blueprint struct {
	ID             string            `json:"id,omitempty"`
	Name           string            `json:"name"`
	Description    string            `json:"description,omitempty"`
	AgentID        string            `json:"agent_id"`
	PinVersion     *string           `json:"pin_version,omitempty"`
	DefaultEnv     map[string]string `json:"default_env,omitempty"`
	CPUOverride    *float64          `json:"cpu_override,omitempty"`
	MemoryOverride *string           `json:"memory_override,omitempty"`
	TimeoutOverride *int             `json:"timeout_override,omitempty"`
	Tags           []string          `json:"tags,omitempty"`
	Region         *string           `json:"region,omitempty"`
	CreatedAt      int64             `json:"created_at,omitempty"`
	UpdatedAt      int64             `json:"updated_at,omitempty"`
}

// GroupMember is a single agent in an AgentGroup.
type GroupMember struct {
	AgentID        string  `json:"agent_id"`
	TenantOverride *string `json:"tenant_override,omitempty"`
}

// AgentGroup is a named collection of agents.
type AgentGroup struct {
	ID        string        `json:"id,omitempty"`
	Name      string        `json:"name"`
	TenantID  string        `json:"tenant_id"`
	Members   []GroupMember `json:"members"`
	Status    string        `json:"status,omitempty"`
	CreatedAt int64         `json:"created_at,omitempty"`
	UpdatedAt int64         `json:"updated_at,omitempty"`
}

// WorkflowStep is a single node in a workflow DAG.
type WorkflowStep struct {
	StepID     string            `json:"step_id"`
	Name       string            `json:"name"`
	AgentID    string            `json:"agent_id"`
	DependsOn  []string          `json:"depends_on"`
	Env        map[string]string `json:"env,omitempty"`
	Optional   bool              `json:"optional,omitempty"`
	TimeoutSecs *int             `json:"timeout_secs,omitempty"`
}

// WorkflowDef is a workflow DAG definition.
type WorkflowDef struct {
	ID          string         `json:"id,omitempty"`
	Name        string         `json:"name"`
	Description string         `json:"description,omitempty"`
	TenantID    string         `json:"tenant_id"`
	Steps       []WorkflowStep `json:"steps"`
	CreatedAt   int64          `json:"created_at,omitempty"`
	UpdatedAt   int64          `json:"updated_at,omitempty"`
}

// WorkflowStepRun records the state of a single step in a run.
type WorkflowStepRun struct {
	StepID    string  `json:"step_id"`
	Status    string  `json:"status"`
	StartedAt *int64  `json:"started_at,omitempty"`
	EndedAt   *int64  `json:"ended_at,omitempty"`
	Error     *string `json:"error,omitempty"`
}

// WorkflowRun is the live state of a workflow execution.
type WorkflowRun struct {
	RunID      string            `json:"run_id"`
	WorkflowID string            `json:"workflow_id"`
	TenantID   string            `json:"tenant_id"`
	Status     string            `json:"status"`
	Steps      []WorkflowStepRun `json:"steps"`
	StartedAt  int64             `json:"started_at"`
	EndedAt    *int64            `json:"ended_at,omitempty"`
}

// ---------------------------------------------------------------------------
// Architecture Selection
// ---------------------------------------------------------------------------

// QuickClassifyRequest is the input for a heuristic architecture classification.
type QuickClassifyRequest struct {
	TenantID         string `json:"tenant_id"`
	ToolCount        int    `json:"tool_count,omitempty"`
	ParallelBranches int    `json:"parallel_branches,omitempty"`
	ErrorTolerance   int    `json:"error_tolerance,omitempty"`
	GovernanceStrict bool   `json:"governance_strict,omitempty"`
}

// ArchitectureDecision is the output of the architecture selection engine.
type ArchitectureDecision struct {
	// Architecture is "deterministic", "single_agent", or "multi_agent".
	Architecture string                 `json:"architecture"`
	Confidence   float64                `json:"confidence"`
	Scores       map[string]float64     `json:"scores"`
	Reasoning    []string               `json:"reasoning"`
	DAG          map[string]interface{} `json:"dag,omitempty"`
	Governance   map[string]interface{} `json:"governance,omitempty"`
	Config       map[string]interface{} `json:"config,omitempty"`
}

// ---------------------------------------------------------------------------
// Alerts (v2.2 extension)
// ---------------------------------------------------------------------------

// AlertCondition defines when an alert fires.
type AlertCondition struct {
	Metric    string  `json:"metric"`
	// Operator: "gt" | "lt" | "eq" | "gte" | "lte"
	Operator  string  `json:"operator"`
	Threshold float64 `json:"threshold"`
}

// AlertAction describes what happens when an alert fires.
type AlertAction struct {
	// Type: "webhook" | "log" | "restart"
	Type   string  `json:"type"`
	Target *string `json:"target,omitempty"`
}

// AlertRule is an alert rule definition.
type AlertRule struct {
	ID          string          `json:"id,omitempty"`
	Name        string          `json:"name"`
	TenantID    string          `json:"tenant_id"`
	AgentID     *string         `json:"agent_id,omitempty"`
	Condition   AlertCondition  `json:"condition"`
	Actions     []AlertAction   `json:"actions"`
	Enabled     bool            `json:"enabled"`
	CooldownSecs *int           `json:"cooldown_secs,omitempty"`
	CreatedAt   int64           `json:"created_at,omitempty"`
	LastFired   *int64          `json:"last_fired,omitempty"`
}

// AlertEvent records a single alert firing.
type AlertEvent struct {
	RuleID      string  `json:"rule_id"`
	FiredAt     int64   `json:"fired_at"`
	MetricValue float64 `json:"metric_value"`
	Message     string  `json:"message"`
}

// ---------------------------------------------------------------------------
// Message Bus (v2.2 extension)
// ---------------------------------------------------------------------------

// BusMessage is a message on the per-agent event bus.
type BusMessage struct {
	ID          string      `json:"id,omitempty"`
	Channel     string      `json:"channel"`
	TenantID    string      `json:"tenant_id"`
	AgentID     string      `json:"agent_id"`
	Payload     interface{} `json:"payload"`
	PublishedAt int64       `json:"published_at,omitempty"`
	ExpiresAt   *int64      `json:"expires_at,omitempty"`
}

// ---------------------------------------------------------------------------
// Usage metering
// ---------------------------------------------------------------------------

// UsageRecord accumulates compute usage for a tenant.
type UsageRecord struct {
	TenantID        string  `json:"tenant_id"`
	CPUSeconds      float64 `json:"cpu_seconds"`
	MemoryGBSeconds float64 `json:"memory_gb_seconds"`
	AgentStarts     int     `json:"agent_starts"`
	UpdatedAt       int64   `json:"updated_at"`
}

// ---------------------------------------------------------------------------
// Common response shapes
// ---------------------------------------------------------------------------

// StatusResponse is returned by create/delete/update endpoints.
type StatusResponse struct {
	Status string `json:"status"`
}

// NodeMetrics is the payload from GET /metrics.
type NodeMetrics struct {
	ActiveAgents int    `json:"active_agents"`
	MaxAgents    int    `json:"max_agents"`
	NodeID       string `json:"node_id"`
	Region       string `json:"region"`
	Version      string `json:"version"`
}

// HealthEndpointResponse is the payload from GET /health.
type HealthEndpointResponse struct {
	Status  string `json:"status"`
	Version string `json:"version"`
}

// PolicyListResponse wraps the list of tenant IDs from GET /tenants/policies.
type PolicyListResponse struct {
	Tenants []string `json:"tenants"`
}

// MemoryListKeys wraps the keys list from GET /memory/{tenant}/{agent}.
type MemoryListKeys = MemoryListResponse

// GroupRunResponse is returned when a group starts.
type GroupRunResponse struct {
	GroupID string   `json:"group_id"`
	Started []string `json:"started"`
	Errors  []string `json:"errors"`
}

// GroupStopResponse is returned when a group stops.
type GroupStopResponse struct {
	GroupID string   `json:"group_id"`
	Stopped []string `json:"stopped"`
}

// BlueprintDeployResponse is returned when deploying a blueprint.
type BlueprintDeployResponse struct {
	Status      string        `json:"status"`
	BlueprintID string        `json:"blueprint_id"`
	Instance    AgentInstance `json:"instance"`
}
