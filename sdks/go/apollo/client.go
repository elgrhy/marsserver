// Package apollo provides a production-quality Go client for the Apollo AI Agent
// Platform v2.2 REST API.
//
// All methods accept a context.Context for cancellation and deadline propagation.
//
// # Authentication
//
// Every request to the Apollo node requires either an X-Apollo-Key header
// or an Authorization: Bearer <JWT> header. Pass the key or JWT when creating
// the client via NewClient or NewClientWithJWT.
//
// # Example
//
//	client, err := apollo.NewClient("http://localhost:8080", "your-secret-key",
//	    apollo.WithTimeout(30*time.Second),
//	)
//	if err != nil {
//	    log.Fatal(err)
//	}
//	defer client.Close()
//
//	ctx := context.Background()
//	agents, err := client.Agents.List(ctx)
//	if err != nil {
//	    log.Fatal(err)
//	}
//	for _, a := range agents {
//	    fmt.Println(a.ID, a.Spec.Version)
//	}
package apollo

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// ---------------------------------------------------------------------------
// ApolloError
// ---------------------------------------------------------------------------

// ApolloError is returned when the Apollo node responds with a non-2xx status code.
type ApolloError struct {
	StatusCode int
	Detail     string
}

func (e *ApolloError) Error() string {
	return fmt.Sprintf("ApolloError [%d]: %s", e.StatusCode, e.Detail)
}

// ---------------------------------------------------------------------------
// Client options
// ---------------------------------------------------------------------------

// Option configures the ApolloClient.
type Option func(*ApolloClient)

// WithTimeout sets the HTTP client timeout (default: 30s).
func WithTimeout(d time.Duration) Option {
	return func(c *ApolloClient) { c.httpClient.Timeout = d }
}

// WithHTTPClient replaces the underlying *http.Client.
func WithHTTPClient(hc *http.Client) Option {
	return func(c *ApolloClient) { c.httpClient = hc }
}

// ---------------------------------------------------------------------------
// ApolloClient
// ---------------------------------------------------------------------------

// ApolloClient is a full-featured Go client for the Apollo AI Agent Platform.
//
// It exposes one sub-client per API namespace (Agents, Traces, Policy, etc.)
// so callers can use named, discoverable methods.
type ApolloClient struct {
	baseURL    string
	headers    map[string]string
	httpClient *http.Client

	// Sub-clients — one per API namespace.
	Agents       *AgentsClient
	Secrets      *SecretsClient
	Usage        *UsageClient
	Traces       *TracesClient
	Policy       *PolicyClient
	Health       *HealthClient
	Memory       *MemoryClient
	Models       *ModelsClient
	Schedule     *ScheduleClient
	Blueprints   *BlueprintsClient
	Groups       *GroupsClient
	Workflows    *WorkflowsClient
	Architecture *ArchitectureClient
	Alerts       *AlertsClient
	Messages     *MessagesClient
}

// NewClient creates a new ApolloClient authenticated with an API key.
//
// The key is sent as the X-Apollo-Key header on every request.
func NewClient(baseURL, key string, opts ...Option) (*ApolloClient, error) {
	return newClient(baseURL, map[string]string{
		"X-Apollo-Key":  key,
		"Content-Type": "application/json",
	}, opts...)
}

// NewClientWithJWT creates a new ApolloClient authenticated with a JWT Bearer token.
func NewClientWithJWT(baseURL, jwt string, opts ...Option) (*ApolloClient, error) {
	return newClient(baseURL, map[string]string{
		"Authorization": "Bearer " + jwt,
		"Content-Type": "application/json",
	}, opts...)
}

func newClient(baseURL string, headers map[string]string, opts ...Option) (*ApolloClient, error) {
	if baseURL == "" {
		return nil, fmt.Errorf("apollo: baseURL must not be empty")
	}
	c := &ApolloClient{
		baseURL: strings.TrimRight(baseURL, "/"),
		headers: headers,
		httpClient: &http.Client{
			Timeout: 30 * time.Second,
		},
	}
	for _, opt := range opts {
		opt(c)
	}

	// Wire sub-clients.
	c.Agents = &AgentsClient{c}
	c.Secrets = &SecretsClient{c}
	c.Usage = &UsageClient{c}
	c.Traces = &TracesClient{c}
	c.Policy = &PolicyClient{c}
	c.Health = &HealthClient{c}
	c.Memory = &MemoryClient{c}
	c.Models = &ModelsClient{c}
	c.Schedule = &ScheduleClient{c}
	c.Blueprints = &BlueprintsClient{c}
	c.Groups = &GroupsClient{c}
	c.Workflows = &WorkflowsClient{c}
	c.Architecture = &ArchitectureClient{c}
	c.Alerts = &AlertsClient{c}
	c.Messages = &MessagesClient{c}

	return c, nil
}

// Close releases resources held by the client (idle connections).
func (c *ApolloClient) Close() {
	c.httpClient.CloseIdleConnections()
}

// Ping checks node health.
func (c *ApolloClient) Ping(ctx context.Context) (*HealthEndpointResponse, error) {
	var out HealthEndpointResponse
	return &out, c.do(ctx, http.MethodGet, "/health", nil, &out)
}

// Metrics returns node capacity and identity information.
func (c *ApolloClient) Metrics(ctx context.Context) (*NodeMetrics, error) {
	var out NodeMetrics
	return &out, c.do(ctx, http.MethodGet, "/metrics", nil, &out)
}

// ---------------------------------------------------------------------------
// Internal HTTP plumbing
// ---------------------------------------------------------------------------

// do executes an HTTP request, decoding the JSON response into out (if non-nil).
// A nil body means no request body. A nil out means the response body is discarded.
func (c *ApolloClient) do(ctx context.Context, method, path string, body, out interface{}) error {
	var bodyReader io.Reader
	if body != nil {
		buf, err := json.Marshal(body)
		if err != nil {
			return fmt.Errorf("apollo: marshal request body: %w", err)
		}
		bodyReader = bytes.NewReader(buf)
	}

	req, err := http.NewRequestWithContext(ctx, method, c.baseURL+path, bodyReader)
	if err != nil {
		return fmt.Errorf("apollo: build request: %w", err)
	}
	for k, v := range c.headers {
		req.Header.Set(k, v)
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("apollo: execute request: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		var errBody struct {
			Error   string `json:"error"`
			Message string `json:"message"`
		}
		_ = json.NewDecoder(resp.Body).Decode(&errBody)
		detail := errBody.Error
		if detail == "" {
			detail = errBody.Message
		}
		if detail == "" {
			detail = resp.Status
		}
		return &ApolloError{StatusCode: resp.StatusCode, Detail: detail}
	}

	if out != nil {
		ct := resp.Header.Get("Content-Type")
		if strings.Contains(ct, "application/json") {
			if err := json.NewDecoder(resp.Body).Decode(out); err != nil {
				return fmt.Errorf("apollo: decode response: %w", err)
			}
		}
	}
	return nil
}

// ---------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------

// AgentsClient provides agent lifecycle management methods.
type AgentsClient struct{ c *ApolloClient }

// List returns all registered agent records.
func (a *AgentsClient) List(ctx context.Context) ([]AgentRecord, error) {
	var out []AgentRecord
	return out, a.c.do(ctx, http.MethodGet, "/agents/list", nil, &out)
}

// Add registers an agent from a local path, HTTPS archive, or git URL.
func (a *AgentsClient) Add(ctx context.Context, source string) (*AgentRecord, error) {
	var out AgentRecord
	return &out, a.c.do(ctx, http.MethodPost, "/agents/add",
		map[string]string{"source": source}, &out)
}

// Run starts an agent for a specific tenant.
func (a *AgentsClient) Run(ctx context.Context, agent, tenant string) (*AgentInstance, error) {
	var out AgentInstance
	return &out, a.c.do(ctx, http.MethodPost, "/agents/run",
		map[string]string{"agent": agent, "tenant": tenant}, &out)
}

// Stop stops a running agent instance.
func (a *AgentsClient) Stop(ctx context.Context, agent, tenant string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, a.c.do(ctx, http.MethodDelete, "/agents/stop",
		map[string]string{"agent": agent, "tenant": tenant}, &out)
}

// Rollback rolls an agent back to its previous version.
func (a *AgentsClient) Rollback(ctx context.Context, agent string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, a.c.do(ctx, http.MethodPost, "/agents/rollback",
		map[string]string{"agent": agent}, &out)
}

// Remove permanently removes an agent from the registry.
func (a *AgentsClient) Remove(ctx context.Context, agent string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, a.c.do(ctx, http.MethodPost, "/agents/remove",
		map[string]string{"agent": agent}, &out)
}

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

// SecretsClient manages per-tenant secrets.
type SecretsClient struct{ c *ApolloClient }

// Put stores or updates secrets for a tenant.
func (s *SecretsClient) Put(ctx context.Context, tenantID string, secrets map[string]string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, s.c.do(ctx, http.MethodPut,
		"/tenants/"+tenantID+"/secrets",
		map[string]interface{}{"secrets": secrets}, &out)
}

// Delete removes all secrets for a tenant.
func (s *SecretsClient) Delete(ctx context.Context, tenantID string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, s.c.do(ctx, http.MethodDelete,
		"/tenants/"+tenantID+"/secrets", nil, &out)
}

// ---------------------------------------------------------------------------
// Usage
// ---------------------------------------------------------------------------

// UsageClient manages compute metering and billing resets.
type UsageClient struct{ c *ApolloClient }

// GetAll returns usage records for all tenants.
func (u *UsageClient) GetAll(ctx context.Context) ([]UsageRecord, error) {
	var out []UsageRecord
	return out, u.c.do(ctx, http.MethodGet, "/usage", nil, &out)
}

// GetTenant returns the usage record for a specific tenant.
func (u *UsageClient) GetTenant(ctx context.Context, tenantID string) (*UsageRecord, error) {
	var out UsageRecord
	return &out, u.c.do(ctx, http.MethodGet, "/usage/"+tenantID, nil, &out)
}

// Reset resets the usage counters for a tenant.
func (u *UsageClient) Reset(ctx context.Context, tenantID string) (*UsageRecord, error) {
	var out UsageRecord
	return &out, u.c.do(ctx, http.MethodPost, "/usage/"+tenantID+"/reset", nil, &out)
}

// ---------------------------------------------------------------------------
// Traces
// ---------------------------------------------------------------------------

// TracesClient manages distributed tracing and token accounting.
type TracesClient struct{ c *ApolloClient }

// List returns trace summaries for a tenant/agent pair.
func (t *TracesClient) List(ctx context.Context, tenantID, agentID string) ([]TraceSummary, error) {
	var out []TraceSummary
	return out, t.c.do(ctx, http.MethodGet,
		"/traces/"+tenantID+"/"+agentID, nil, &out)
}

// Get retrieves all spans for a specific trace.
func (t *TracesClient) Get(ctx context.Context, tenantID, agentID, traceID string) ([]TraceSpan, error) {
	var out []TraceSpan
	return out, t.c.do(ctx, http.MethodGet,
		"/traces/"+tenantID+"/"+agentID+"/"+traceID, nil, &out)
}

// PostSpan posts a single execution span.
// span_id and trace_id are auto-assigned by the server if left empty.
func (t *TracesClient) PostSpan(ctx context.Context, span *TraceSpan) (*SpanPostResponse, error) {
	var out SpanPostResponse
	return &out, t.c.do(ctx, http.MethodPost,
		"/traces/"+span.TenantID+"/"+span.AgentID+"/spans", span, &out)
}

// Finalize finalises a trace — builds summary and aggregates token totals.
func (t *TracesClient) Finalize(ctx context.Context, tenantID, agentID, traceID string) (*TraceSummary, error) {
	var out TraceSummary
	return &out, t.c.do(ctx, http.MethodPost,
		"/traces/"+tenantID+"/"+agentID+"/"+traceID+"/finalize", nil, &out)
}

// TokenStats returns aggregated token usage for a tenant (for billing).
func (t *TracesClient) TokenStats(ctx context.Context, tenantID string) (*TenantTokenStats, error) {
	var out TenantTokenStats
	return &out, t.c.do(ctx, http.MethodGet,
		"/traces/"+tenantID+"/tokens", nil, &out)
}

// ---------------------------------------------------------------------------
// Policy
// ---------------------------------------------------------------------------

// PolicyClient manages per-tenant governance policies.
type PolicyClient struct{ c *ApolloClient }

// Get returns the current governance policy for a tenant.
func (p *PolicyClient) Get(ctx context.Context, tenantID string) (*TenantPolicy, error) {
	var out TenantPolicy
	return &out, p.c.do(ctx, http.MethodGet,
		"/tenants/"+tenantID+"/policy", nil, &out)
}

// Put sets or updates the governance policy for a tenant.
func (p *PolicyClient) Put(ctx context.Context, tenantID string, policy *TenantPolicy) (*StatusResponse, error) {
	var out StatusResponse
	return &out, p.c.do(ctx, http.MethodPut,
		"/tenants/"+tenantID+"/policy", policy, &out)
}

// Delete removes a tenant's policy (resets to permissive default).
func (p *PolicyClient) Delete(ctx context.Context, tenantID string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, p.c.do(ctx, http.MethodDelete,
		"/tenants/"+tenantID+"/policy", nil, &out)
}

// Compliance retrieves the compliance report for a tenant.
func (p *PolicyClient) Compliance(ctx context.Context, tenantID string) (*ComplianceReport, error) {
	var out ComplianceReport
	return &out, p.c.do(ctx, http.MethodGet,
		"/tenants/"+tenantID+"/compliance", nil, &out)
}

// ListTenants returns the IDs of all tenants that have a stored policy.
func (p *PolicyClient) ListTenants(ctx context.Context) ([]string, error) {
	var out PolicyListResponse
	if err := p.c.do(ctx, http.MethodGet, "/tenants/policies", nil, &out); err != nil {
		return nil, err
	}
	return out.Tenants, nil
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

// HealthClient provides health intelligence methods.
type HealthClient struct{ c *ApolloClient }

// Agent returns the health record for a specific agent.
func (h *HealthClient) Agent(ctx context.Context, tenantID, agentID string) (*HealthRecord, error) {
	var out HealthRecord
	return &out, h.c.do(ctx, http.MethodGet,
		"/health/"+tenantID+"/"+agentID, nil, &out)
}

// Tenant returns health records for all agents under a tenant.
func (h *HealthClient) Tenant(ctx context.Context, tenantID string) ([]HealthRecord, error) {
	var out []HealthRecord
	return out, h.c.do(ctx, http.MethodGet,
		"/health/"+tenantID, nil, &out)
}

// Fleet returns the fleet-wide health summary.
func (h *HealthClient) Fleet(ctx context.Context) (*FleetHealthSummary, error) {
	var out FleetHealthSummary
	return &out, h.c.do(ctx, http.MethodGet,
		"/health/fleet/summary", nil, &out)
}

// ---------------------------------------------------------------------------
// Memory
// ---------------------------------------------------------------------------

// MemoryClient manages the per-agent persistent key-value store.
type MemoryClient struct{ c *ApolloClient }

// Get retrieves a memory entry by key.
func (m *MemoryClient) Get(ctx context.Context, tenantID, agentID, key string) (*MemoryEntry, error) {
	var out MemoryEntry
	return &out, m.c.do(ctx, http.MethodGet,
		"/memory/"+tenantID+"/"+agentID+"/"+key, nil, &out)
}

// Put stores a memory entry.
func (m *MemoryClient) Put(ctx context.Context, tenantID, agentID, key string, body *MemoryPutBody) (*MemoryEntry, error) {
	var out MemoryEntry
	return &out, m.c.do(ctx, http.MethodPut,
		"/memory/"+tenantID+"/"+agentID+"/"+key, body, &out)
}

// Delete removes a memory entry.
func (m *MemoryClient) Delete(ctx context.Context, tenantID, agentID, key string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, m.c.do(ctx, http.MethodDelete,
		"/memory/"+tenantID+"/"+agentID+"/"+key, nil, &out)
}

// List returns all stored memory keys for an agent.
func (m *MemoryClient) List(ctx context.Context, tenantID, agentID string) ([]string, error) {
	var out MemoryListResponse
	if err := m.c.do(ctx, http.MethodGet,
		"/memory/"+tenantID+"/"+agentID, nil, &out); err != nil {
		return nil, err
	}
	return out.Keys, nil
}

// Search performs a TF-IDF similarity search over the memory store.
func (m *MemoryClient) Search(ctx context.Context, tenantID, agentID string, query *MemoryQuery) ([]MemoryEntry, error) {
	var out []MemoryEntry
	return out, m.c.do(ctx, http.MethodPost,
		"/memory/"+tenantID+"/"+agentID+"/search", query, &out)
}

// Clear deletes all memory entries for an agent.
func (m *MemoryClient) Clear(ctx context.Context, tenantID, agentID string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, m.c.do(ctx, http.MethodDelete,
		"/memory/"+tenantID+"/"+agentID, nil, &out)
}

// Stats returns memory store statistics.
func (m *MemoryClient) Stats(ctx context.Context, tenantID, agentID string) (*MemoryStats, error) {
	var out MemoryStats
	return &out, m.c.do(ctx, http.MethodGet,
		"/memory/"+tenantID+"/"+agentID+"/stats", nil, &out)
}

// ---------------------------------------------------------------------------
// Models
// ---------------------------------------------------------------------------

// ModelsClient manages the LLM model registry and routing.
type ModelsClient struct{ c *ApolloClient }

// List returns all registered LLM models.
func (m *ModelsClient) List(ctx context.Context) ([]ModelRecord, error) {
	var out []ModelRecord
	return out, m.c.do(ctx, http.MethodGet, "/models", nil, &out)
}

// Put registers or updates an LLM model.
func (m *ModelsClient) Put(ctx context.Context, modelID string, model *ModelRecord) (*ModelRecord, error) {
	var out ModelRecord
	return &out, m.c.do(ctx, http.MethodPut, "/models/"+modelID, model, &out)
}

// Delete removes a model from the registry.
func (m *ModelsClient) Delete(ctx context.Context, modelID string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, m.c.do(ctx, http.MethodDelete, "/models/"+modelID, nil, &out)
}

// Route returns a cost/latency/policy-aware model routing recommendation.
func (m *ModelsClient) Route(ctx context.Context, req *RoutingRequest) (*RoutingDecision, error) {
	var out RoutingDecision
	return &out, m.c.do(ctx, http.MethodPost, "/models/route", req, &out)
}

// Feedback reports observed latency for a model invocation.
func (m *ModelsClient) Feedback(ctx context.Context, fb *ModelFeedback) (*StatusResponse, error) {
	var out StatusResponse
	return &out, m.c.do(ctx, http.MethodPost, "/models/feedback", fb, &out)
}

// Usage returns per-tenant model usage and cost breakdown.
func (m *ModelsClient) Usage(ctx context.Context, tenantID string) (map[string]interface{}, error) {
	var out map[string]interface{}
	return out, m.c.do(ctx, http.MethodGet, "/models/usage/"+tenantID, nil, &out)
}

// RecordUsage records model usage for a tenant.
func (m *ModelsClient) RecordUsage(ctx context.Context, tenantID string, rec *ModelUsageRecord) (*StatusResponse, error) {
	var out StatusResponse
	return &out, m.c.do(ctx, http.MethodPost,
		"/models/usage/"+tenantID+"/record", rec, &out)
}

// ---------------------------------------------------------------------------
// Schedule
// ---------------------------------------------------------------------------

// ScheduleClient manages cron, interval, and one-shot job scheduling.
type ScheduleClient struct{ c *ApolloClient }

// List returns all scheduled jobs.
func (s *ScheduleClient) List(ctx context.Context) ([]ScheduledJob, error) {
	var out []ScheduledJob
	return out, s.c.do(ctx, http.MethodGet, "/schedule", nil, &out)
}

// Create creates a new scheduled job.
func (s *ScheduleClient) Create(ctx context.Context, job *ScheduledJob) (*ScheduledJob, error) {
	var out ScheduledJob
	return &out, s.c.do(ctx, http.MethodPost, "/schedule", job, &out)
}

// Get retrieves a scheduled job by ID.
func (s *ScheduleClient) Get(ctx context.Context, jobID string) (*ScheduledJob, error) {
	var out ScheduledJob
	return &out, s.c.do(ctx, http.MethodGet, "/schedule/"+jobID, nil, &out)
}

// Update updates a scheduled job.
func (s *ScheduleClient) Update(ctx context.Context, jobID string, job *ScheduledJob) (*ScheduledJob, error) {
	var out ScheduledJob
	return &out, s.c.do(ctx, http.MethodPut, "/schedule/"+jobID, job, &out)
}

// Delete deletes a scheduled job.
func (s *ScheduleClient) Delete(ctx context.Context, jobID string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, s.c.do(ctx, http.MethodDelete, "/schedule/"+jobID, nil, &out)
}

// Run manually triggers a scheduled job immediately.
func (s *ScheduleClient) Run(ctx context.Context, jobID string) (*AgentInstance, error) {
	var out AgentInstance
	return &out, s.c.do(ctx, http.MethodPost, "/schedule/"+jobID+"/run", nil, &out)
}

// History returns the run history for a scheduled job.
func (s *ScheduleClient) History(ctx context.Context, jobID string) ([]JobRunRecord, error) {
	var out []JobRunRecord
	return out, s.c.do(ctx, http.MethodGet, "/schedule/"+jobID+"/history", nil, &out)
}

// ---------------------------------------------------------------------------
// Blueprints
// ---------------------------------------------------------------------------

// BlueprintsClient manages agent deployment blueprints.
type BlueprintsClient struct{ c *ApolloClient }

// List returns all blueprints.
func (b *BlueprintsClient) List(ctx context.Context) ([]Blueprint, error) {
	var out []Blueprint
	return out, b.c.do(ctx, http.MethodGet, "/blueprints", nil, &out)
}

// Create creates a new blueprint.
func (b *BlueprintsClient) Create(ctx context.Context, bp *Blueprint) (*Blueprint, error) {
	var out Blueprint
	return &out, b.c.do(ctx, http.MethodPost, "/blueprints", bp, &out)
}

// Get retrieves a blueprint by ID.
func (b *BlueprintsClient) Get(ctx context.Context, id string) (*Blueprint, error) {
	var out Blueprint
	return &out, b.c.do(ctx, http.MethodGet, "/blueprints/"+id, nil, &out)
}

// Update updates a blueprint.
func (b *BlueprintsClient) Update(ctx context.Context, id string, bp *Blueprint) (*Blueprint, error) {
	var out Blueprint
	return &out, b.c.do(ctx, http.MethodPut, "/blueprints/"+id, bp, &out)
}

// Delete deletes a blueprint.
func (b *BlueprintsClient) Delete(ctx context.Context, id string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, b.c.do(ctx, http.MethodDelete, "/blueprints/"+id, nil, &out)
}

// Deploy deploys an agent from a blueprint for a tenant.
func (b *BlueprintsClient) Deploy(ctx context.Context, id, tenantID string) (*BlueprintDeployResponse, error) {
	var out BlueprintDeployResponse
	return &out, b.c.do(ctx, http.MethodPost, "/blueprints/"+id+"/deploy",
		map[string]string{"tenant_id": tenantID}, &out)
}

// ---------------------------------------------------------------------------
// Groups
// ---------------------------------------------------------------------------

// GroupsClient manages agent groups.
type GroupsClient struct{ c *ApolloClient }

// List returns all agent groups.
func (g *GroupsClient) List(ctx context.Context) ([]AgentGroup, error) {
	var out []AgentGroup
	return out, g.c.do(ctx, http.MethodGet, "/groups", nil, &out)
}

// Create creates a new agent group.
func (g *GroupsClient) Create(ctx context.Context, group *AgentGroup) (*AgentGroup, error) {
	var out AgentGroup
	return &out, g.c.do(ctx, http.MethodPost, "/groups", group, &out)
}

// Get retrieves an agent group by ID.
func (g *GroupsClient) Get(ctx context.Context, id string) (*AgentGroup, error) {
	var out AgentGroup
	return &out, g.c.do(ctx, http.MethodGet, "/groups/"+id, nil, &out)
}

// Update updates a group definition.
func (g *GroupsClient) Update(ctx context.Context, id string, group *AgentGroup) (*AgentGroup, error) {
	var out AgentGroup
	return &out, g.c.do(ctx, http.MethodPut, "/groups/"+id, group, &out)
}

// Delete deletes an agent group.
func (g *GroupsClient) Delete(ctx context.Context, id string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, g.c.do(ctx, http.MethodDelete, "/groups/"+id, nil, &out)
}

// Run starts all agents in a group.
func (g *GroupsClient) Run(ctx context.Context, id string) (*GroupRunResponse, error) {
	var out GroupRunResponse
	return &out, g.c.do(ctx, http.MethodPost, "/groups/"+id+"/run", nil, &out)
}

// Stop stops all agents in a group.
func (g *GroupsClient) Stop(ctx context.Context, id string) (*GroupStopResponse, error) {
	var out GroupStopResponse
	return &out, g.c.do(ctx, http.MethodPost, "/groups/"+id+"/stop", nil, &out)
}

// ---------------------------------------------------------------------------
// Workflows
// ---------------------------------------------------------------------------

// WorkflowsClient manages workflow DAGs and runs.
type WorkflowsClient struct{ c *ApolloClient }

// List returns all workflow definitions.
func (w *WorkflowsClient) List(ctx context.Context) ([]WorkflowDef, error) {
	var out []WorkflowDef
	return out, w.c.do(ctx, http.MethodGet, "/workflows", nil, &out)
}

// Create creates a new workflow DAG.
func (w *WorkflowsClient) Create(ctx context.Context, def *WorkflowDef) (*WorkflowDef, error) {
	var out WorkflowDef
	return &out, w.c.do(ctx, http.MethodPost, "/workflows", def, &out)
}

// Get retrieves a workflow definition.
func (w *WorkflowsClient) Get(ctx context.Context, id string) (*WorkflowDef, error) {
	var out WorkflowDef
	return &out, w.c.do(ctx, http.MethodGet, "/workflows/"+id, nil, &out)
}

// Delete deletes a workflow definition.
func (w *WorkflowsClient) Delete(ctx context.Context, id string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, w.c.do(ctx, http.MethodDelete, "/workflows/"+id, nil, &out)
}

// Run executes a workflow.
func (w *WorkflowsClient) Run(ctx context.Context, id string) (*WorkflowRun, error) {
	var out WorkflowRun
	return &out, w.c.do(ctx, http.MethodPost, "/workflows/"+id+"/run", nil, &out)
}

// RunsList returns all runs for a workflow.
func (w *WorkflowsClient) RunsList(ctx context.Context, id string) ([]WorkflowRun, error) {
	var out []WorkflowRun
	return out, w.c.do(ctx, http.MethodGet, "/workflows/"+id+"/runs", nil, &out)
}

// RunGet returns the current state of a workflow run.
func (w *WorkflowsClient) RunGet(ctx context.Context, runID string) (*WorkflowRun, error) {
	var out WorkflowRun
	return &out, w.c.do(ctx, http.MethodGet, "/workflows/runs/"+runID, nil, &out)
}

// ---------------------------------------------------------------------------
// Architecture
// ---------------------------------------------------------------------------

// ArchitectureClient provides architecture selection methods.
type ArchitectureClient struct{ c *ApolloClient }

// Select analyses a WorkflowDef and selects the optimal execution architecture.
func (a *ArchitectureClient) Select(ctx context.Context, wf *WorkflowDef) (*ArchitectureDecision, error) {
	var out ArchitectureDecision
	return &out, a.c.do(ctx, http.MethodPost, "/architecture/select", wf, &out)
}

// SelectSaved analyses a previously saved workflow by ID.
func (a *ArchitectureClient) SelectSaved(ctx context.Context, workflowID string) (*ArchitectureDecision, error) {
	var out ArchitectureDecision
	return &out, a.c.do(ctx, http.MethodGet,
		"/architecture/select/"+workflowID, nil, &out)
}

// Classify performs a quick heuristic classification without a full WorkflowDef.
func (a *ArchitectureClient) Classify(ctx context.Context, req *QuickClassifyRequest) (*ArchitectureDecision, error) {
	var out ArchitectureDecision
	return &out, a.c.do(ctx, http.MethodPost, "/architecture/classify", req, &out)
}

// ---------------------------------------------------------------------------
// Alerts (v2.2 extension)
// ---------------------------------------------------------------------------

// AlertsClient manages alert rules and history.
type AlertsClient struct{ c *ApolloClient }

// ListRules returns all alert rules for a tenant.
func (a *AlertsClient) ListRules(ctx context.Context, tenantID string) ([]AlertRule, error) {
	var out []AlertRule
	return out, a.c.do(ctx, http.MethodGet,
		"/alerts/"+tenantID+"/rules", nil, &out)
}

// CreateRule creates an alert rule for a tenant.
func (a *AlertsClient) CreateRule(ctx context.Context, tenantID string, rule *AlertRule) (*AlertRule, error) {
	var out AlertRule
	return &out, a.c.do(ctx, http.MethodPost,
		"/alerts/"+tenantID+"/rules", rule, &out)
}

// DeleteRule deletes an alert rule.
func (a *AlertsClient) DeleteRule(ctx context.Context, tenantID, ruleID string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, a.c.do(ctx, http.MethodDelete,
		"/alerts/"+tenantID+"/rules/"+ruleID, nil, &out)
}

// GetHistory returns alert event history for a rule.
func (a *AlertsClient) GetHistory(ctx context.Context, tenantID, ruleID string) ([]AlertEvent, error) {
	var out []AlertEvent
	return out, a.c.do(ctx, http.MethodGet,
		"/alerts/"+tenantID+"/rules/"+ruleID+"/history", nil, &out)
}

// ---------------------------------------------------------------------------
// Messages (v2.2 extension)
// ---------------------------------------------------------------------------

// MessagesClient manages the per-agent event bus.
type MessagesClient struct{ c *ApolloClient }

// Publish publishes a message to an agent's channel.
func (m *MessagesClient) Publish(ctx context.Context, tenantID, agentID, channel string, payload interface{}, ttlSecs *int64) (*BusMessage, error) {
	body := map[string]interface{}{"channel": channel, "payload": payload}
	if ttlSecs != nil {
		body["ttl_secs"] = *ttlSecs
	}
	var out BusMessage
	return &out, m.c.do(ctx, http.MethodPost,
		"/messages/"+tenantID+"/"+agentID, body, &out)
}

// Poll polls messages from a channel (dequeues messages).
func (m *MessagesClient) Poll(ctx context.Context, tenantID, agentID, channel string, limit int) ([]BusMessage, error) {
	path := fmt.Sprintf("/messages/%s/%s/%s?limit=%d", tenantID, agentID, channel, limit)
	var out []BusMessage
	return out, m.c.do(ctx, http.MethodGet, path, nil, &out)
}

// Clear removes all messages from a channel.
func (m *MessagesClient) Clear(ctx context.Context, tenantID, agentID, channel string) (*StatusResponse, error) {
	var out StatusResponse
	return &out, m.c.do(ctx, http.MethodDelete,
		"/messages/"+tenantID+"/"+agentID+"/"+channel, nil, &out)
}
