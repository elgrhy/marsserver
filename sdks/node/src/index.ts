/**
 * Apollo AI Agent Platform — TypeScript/Node.js SDK v2.2.0
 *
 * Uses native fetch (Node 18+). No additional HTTP library required.
 *
 * @example
 * ```typescript
 * import { ApolloClient } from '@apollo-platform/sdk';
 *
 * const client = new ApolloClient('http://localhost:8080', { key: 'your-secret' });
 * const agents = await client.agents.list();
 * const instance = await client.agents.run('openclaw', 'alice');
 * await client.close();
 * ```
 */

export * from './types';

import type {
  AgentRecord,
  AgentInstance,
  AlertEvent,
  AlertRule,
  ArchitectureDecision,
  Blueprint,
  BlueprintDeployResponse,
  AgentGroup,
  BusMessage,
  ComplianceReport,
  FleetHealthSummary,
  GroupRunResponse,
  GroupStopResponse,
  HealthRecord,
  HealthEndpointResponse,
  JobRunRecord,
  MemoryEntry,
  MemoryListResponse,
  MemoryPutBody,
  MemoryQuery,
  MemoryStats,
  ModelFeedback,
  ModelRecord,
  ModelUsageRecord,
  NodeMetrics,
  PolicyListResponse,
  QuickClassifyRequest,
  RoutingDecision,
  RoutingRequest,
  ScheduledJob,
  SpanPostResponse,
  StatusResponse,
  TenantPolicy,
  TenantTokenStats,
  TraceSummary,
  TraceSpan,
  UsageRecord,
  WorkflowDef,
  WorkflowRun,
} from './types';

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/** Thrown when the Apollo node returns a non-2xx HTTP response. */
export class ApolloError extends Error {
  constructor(
    public readonly statusCode: number,
    public readonly detail: string,
  ) {
    super(`ApolloError [${statusCode}]: ${detail}`);
    this.name = 'ApolloError';
  }
}

// ---------------------------------------------------------------------------
// Internal HTTP helpers
// ---------------------------------------------------------------------------

interface ClientOptions {
  /** API key sent as X-Apollo-Key header. Mutually exclusive with jwt. */
  key?: string;
  /** JWT Bearer token. Mutually exclusive with key. */
  jwt?: string;
  /** Request timeout in milliseconds (default: 30000). */
  timeout?: number;
}

async function raiseForStatus(response: Response): Promise<Response> {
  if (!response.ok) {
    let detail: string;
    try {
      const body = await response.json() as Record<string, unknown>;
      detail = String(body['error'] ?? body['message'] ?? response.statusText);
    } catch {
      detail = response.statusText;
    }
    throw new ApolloError(response.status, detail);
  }
  return response;
}

// ---------------------------------------------------------------------------
// Resource API classes
// ---------------------------------------------------------------------------

/**
 * Agent lifecycle management: register, start, stop, rollback, remove.
 */
export class AgentsAPI {
  constructor(private readonly request: RequestFn) {}

  /** List all registered agent records. */
  async list(): Promise<AgentRecord[]> {
    return this.request('GET', '/agents/list');
  }

  /**
   * Register an agent from a local path, HTTPS archive, or git URL.
   * @param source - Local path, HTTPS URL, or git repository URL.
   */
  async add(source: string): Promise<AgentRecord> {
    return this.request('POST', '/agents/add', { source });
  }

  /**
   * Start an agent for a specific tenant.
   * @param agent - Registered agent name.
   * @param tenant - Tenant / user ID.
   */
  async run(agent: string, tenant: string): Promise<AgentInstance> {
    return this.request('POST', '/agents/run', { agent, tenant });
  }

  /**
   * Stop a running agent instance.
   * @param agent - Agent name.
   * @param tenant - Tenant ID.
   */
  async stop(agent: string, tenant: string): Promise<StatusResponse> {
    return this.request('DELETE', '/agents/stop', { agent, tenant });
  }

  /**
   * Roll back an agent to its previous version.
   * @param agent - Agent name.
   */
  async rollback(agent: string): Promise<StatusResponse> {
    return this.request('POST', '/agents/rollback', { agent });
  }

  /**
   * Permanently remove an agent from the registry.
   * @param agent - Agent name.
   */
  async remove(agent: string): Promise<StatusResponse> {
    return this.request('POST', '/agents/remove', { agent });
  }
}

/** Per-tenant secret management. */
export class SecretsAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * Store or update secrets for a tenant.
   * @param tenantId - Tenant identifier.
   * @param secrets - Key-value mapping of secret names to values.
   */
  async put(tenantId: string, secrets: Record<string, string>): Promise<StatusResponse> {
    return this.request('PUT', `/tenants/${tenantId}/secrets`, { secrets });
  }

  /**
   * Delete all secrets for a tenant.
   * @param tenantId - Tenant identifier.
   */
  async delete(tenantId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/tenants/${tenantId}/secrets`);
  }
}

/** Compute metering and billing cycle management. */
export class UsageAPI {
  constructor(private readonly request: RequestFn) {}

  /** Get usage records for all tenants. */
  async getAll(): Promise<UsageRecord[]> {
    return this.request('GET', '/usage');
  }

  /**
   * Get usage record for a specific tenant.
   * @param tenantId - Tenant identifier.
   */
  async getTenant(tenantId: string): Promise<UsageRecord> {
    return this.request('GET', `/usage/${tenantId}`);
  }

  /**
   * Reset usage counters for a tenant.
   * @param tenantId - Tenant identifier.
   */
  async reset(tenantId: string): Promise<UsageRecord> {
    return this.request('POST', `/usage/${tenantId}/reset`);
  }
}

/** Distributed tracing, span ingestion, and token billing. */
export class TracesAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * List trace summaries for a tenant/agent pair.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   */
  async list(tenantId: string, agentId: string): Promise<TraceSummary[]> {
    return this.request('GET', `/traces/${tenantId}/${agentId}`);
  }

  /**
   * Retrieve all spans for a specific trace.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param traceId - Trace ID.
   */
  async get(tenantId: string, agentId: string, traceId: string): Promise<TraceSpan[]> {
    return this.request('GET', `/traces/${tenantId}/${agentId}/${traceId}`);
  }

  /**
   * Post a single execution span.
   * @param span - The TraceSpan to record. span_id and trace_id are auto-assigned if empty.
   */
  async postSpan(span: TraceSpan): Promise<SpanPostResponse> {
    return this.request(
      'POST',
      `/traces/${span.tenant_id}/${span.agent_id}/spans`,
      span,
    );
  }

  /**
   * Finalize a trace — builds summary and aggregates token totals.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param traceId - Trace to finalize.
   */
  async finalize(tenantId: string, agentId: string, traceId: string): Promise<TraceSummary> {
    return this.request(
      'POST',
      `/traces/${tenantId}/${agentId}/${traceId}/finalize`,
    );
  }

  /**
   * Get aggregated token usage for a tenant (for billing).
   * @param tenantId - Tenant identifier.
   */
  async tokenStats(tenantId: string): Promise<TenantTokenStats> {
    return this.request('GET', `/traces/${tenantId}/tokens`);
  }
}

/** Per-tenant governance policy management. */
export class PolicyAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * Get the current governance policy for a tenant.
   * @param tenantId - Tenant identifier.
   */
  async get(tenantId: string): Promise<TenantPolicy> {
    return this.request('GET', `/tenants/${tenantId}/policy`);
  }

  /**
   * Set or update the governance policy for a tenant.
   * @param tenantId - Tenant identifier.
   * @param policy - The new TenantPolicy.
   */
  async put(tenantId: string, policy: TenantPolicy): Promise<StatusResponse> {
    return this.request('PUT', `/tenants/${tenantId}/policy`, policy);
  }

  /**
   * Delete a tenant's policy (resets to permissive default).
   * @param tenantId - Tenant identifier.
   */
  async delete(tenantId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/tenants/${tenantId}/policy`);
  }

  /**
   * Retrieve the compliance report for a tenant.
   * @param tenantId - Tenant identifier.
   */
  async compliance(tenantId: string): Promise<ComplianceReport> {
    return this.request('GET', `/tenants/${tenantId}/compliance`);
  }

  /** List all tenant IDs that have a stored policy. */
  async listTenants(): Promise<string[]> {
    const resp = await this.request<PolicyListResponse>('GET', '/tenants/policies');
    return resp.tenants;
  }
}

/** Health intelligence — scoring, crash patterns, resource trends. */
export class HealthAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * Get health record for a specific agent.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   */
  async agent(tenantId: string, agentId: string): Promise<HealthRecord> {
    return this.request('GET', `/health/${tenantId}/${agentId}`);
  }

  /**
   * Get health records for all agents under a tenant.
   * @param tenantId - Tenant identifier.
   */
  async tenant(tenantId: string): Promise<HealthRecord[]> {
    return this.request('GET', `/health/${tenantId}`);
  }

  /** Get fleet-wide health summary. */
  async fleet(): Promise<FleetHealthSummary> {
    return this.request('GET', '/health/fleet/summary');
  }
}

/** Per-agent persistent key-value store with TF-IDF similarity search. */
export class MemoryAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * Retrieve a memory entry by key.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param key - Memory key.
   */
  async get(tenantId: string, agentId: string, key: string): Promise<MemoryEntry> {
    return this.request('GET', `/memory/${tenantId}/${agentId}/${key}`);
  }

  /**
   * Store a memory entry.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param key - Memory key.
   * @param body - MemoryPutBody with value, optional tags, text, and ttl_secs.
   */
  async put(
    tenantId: string,
    agentId: string,
    key: string,
    body: MemoryPutBody,
  ): Promise<MemoryEntry> {
    return this.request('PUT', `/memory/${tenantId}/${agentId}/${key}`, body);
  }

  /**
   * Delete a memory entry.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param key - Memory key.
   */
  async delete(tenantId: string, agentId: string, key: string): Promise<StatusResponse> {
    return this.request('DELETE', `/memory/${tenantId}/${agentId}/${key}`);
  }

  /**
   * List all stored memory keys.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   */
  async list(tenantId: string, agentId: string): Promise<string[]> {
    const resp = await this.request<MemoryListResponse>('GET', `/memory/${tenantId}/${agentId}`);
    return resp.keys;
  }

  /**
   * Perform a TF-IDF similarity search over the memory store.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param query - MemoryQuery with search string, optional tags, and limit.
   */
  async search(tenantId: string, agentId: string, query: MemoryQuery): Promise<MemoryEntry[]> {
    return this.request('POST', `/memory/${tenantId}/${agentId}/search`, query);
  }

  /**
   * Delete all memory entries for an agent.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   */
  async clear(tenantId: string, agentId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/memory/${tenantId}/${agentId}`);
  }

  /**
   * Get memory store statistics.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   */
  async stats(tenantId: string, agentId: string): Promise<MemoryStats> {
    return this.request('GET', `/memory/${tenantId}/${agentId}/stats`);
  }
}

/** LLM model registry and cost/latency-aware routing. */
export class ModelsAPI {
  constructor(private readonly request: RequestFn) {}

  /** List all registered LLM models. */
  async list(): Promise<ModelRecord[]> {
    return this.request('GET', '/models');
  }

  /**
   * Register or update an LLM model.
   * @param modelId - Unique model identifier.
   * @param model - ModelRecord with routing metadata.
   */
  async put(modelId: string, model: ModelRecord): Promise<ModelRecord> {
    return this.request('PUT', `/models/${modelId}`, model);
  }

  /**
   * Remove a model from the registry.
   * @param modelId - Model identifier.
   */
  async delete(modelId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/models/${modelId}`);
  }

  /**
   * Get a cost/latency/policy-aware model routing recommendation.
   * @param request - RoutingRequest with token estimates and constraints.
   */
  async route(request: RoutingRequest): Promise<RoutingDecision> {
    return this.request('POST', '/models/route', request);
  }

  /**
   * Report observed latency feedback for a model.
   * @param feedback - ModelFeedback with model_id, latency_ms, is_success.
   */
  async feedback(feedback: ModelFeedback): Promise<StatusResponse> {
    return this.request('POST', '/models/feedback', feedback);
  }

  /**
   * Get per-tenant model usage and cost breakdown.
   * @param tenantId - Tenant identifier.
   */
  async usage(tenantId: string): Promise<Record<string, unknown>> {
    return this.request('GET', `/models/usage/${tenantId}`);
  }

  /**
   * Record model usage for a tenant.
   * @param tenantId - Tenant identifier.
   * @param record - ModelUsageRecord with token counts and cost.
   */
  async recordUsage(tenantId: string, record: ModelUsageRecord): Promise<StatusResponse> {
    return this.request('POST', `/models/usage/${tenantId}/record`, record);
  }
}

/** Cron, interval, and one-shot job scheduling. */
export class ScheduleAPI {
  constructor(private readonly request: RequestFn) {}

  /** List all scheduled jobs. */
  async list(): Promise<ScheduledJob[]> {
    return this.request('GET', '/schedule');
  }

  /**
   * Create a new scheduled job.
   * @param job - ScheduledJob definition. id is auto-assigned if omitted.
   */
  async create(job: ScheduledJob): Promise<ScheduledJob> {
    return this.request('POST', '/schedule', job);
  }

  /**
   * Get a scheduled job by ID.
   * @param jobId - Job identifier.
   */
  async get(jobId: string): Promise<ScheduledJob> {
    return this.request('GET', `/schedule/${jobId}`);
  }

  /**
   * Update a scheduled job.
   * @param jobId - Job identifier.
   * @param job - Updated ScheduledJob definition.
   */
  async update(jobId: string, job: ScheduledJob): Promise<ScheduledJob> {
    return this.request('PUT', `/schedule/${jobId}`, job);
  }

  /**
   * Delete a scheduled job.
   * @param jobId - Job identifier.
   */
  async delete(jobId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/schedule/${jobId}`);
  }

  /**
   * Manually trigger a scheduled job immediately.
   * @param jobId - Job identifier.
   */
  async run(jobId: string): Promise<AgentInstance> {
    return this.request('POST', `/schedule/${jobId}/run`);
  }

  /**
   * Get the run history for a scheduled job.
   * @param jobId - Job identifier.
   */
  async history(jobId: string): Promise<JobRunRecord[]> {
    return this.request('GET', `/schedule/${jobId}/history`);
  }
}

/** Agent deployment blueprints. */
export class BlueprintsAPI {
  constructor(private readonly request: RequestFn) {}

  /** List all blueprints. */
  async list(): Promise<Blueprint[]> {
    return this.request('GET', '/blueprints');
  }

  /**
   * Create a new blueprint.
   * @param blueprint - Blueprint definition. id is auto-assigned if omitted.
   */
  async create(blueprint: Blueprint): Promise<Blueprint> {
    return this.request('POST', '/blueprints', blueprint);
  }

  /**
   * Get a blueprint by ID.
   * @param blueprintId - Blueprint identifier.
   */
  async get(blueprintId: string): Promise<Blueprint> {
    return this.request('GET', `/blueprints/${blueprintId}`);
  }

  /**
   * Update a blueprint.
   * @param blueprintId - Blueprint identifier.
   * @param blueprint - Updated Blueprint definition.
   */
  async update(blueprintId: string, blueprint: Blueprint): Promise<Blueprint> {
    return this.request('PUT', `/blueprints/${blueprintId}`, blueprint);
  }

  /**
   * Delete a blueprint.
   * @param blueprintId - Blueprint identifier.
   */
  async delete(blueprintId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/blueprints/${blueprintId}`);
  }

  /**
   * Deploy an agent from a blueprint for a tenant.
   * @param blueprintId - Blueprint identifier.
   * @param tenantId - Tenant / user ID.
   */
  async deploy(blueprintId: string, tenantId: string): Promise<BlueprintDeployResponse> {
    return this.request('POST', `/blueprints/${blueprintId}/deploy`, { tenant_id: tenantId });
  }
}

/** Agent groups — bulk lifecycle management. */
export class GroupsAPI {
  constructor(private readonly request: RequestFn) {}

  /** List all agent groups. */
  async list(): Promise<AgentGroup[]> {
    return this.request('GET', '/groups');
  }

  /**
   * Create a new agent group.
   * @param group - AgentGroup definition.
   */
  async create(group: AgentGroup): Promise<AgentGroup> {
    return this.request('POST', '/groups', group);
  }

  /**
   * Get an agent group by ID.
   * @param groupId - Group identifier.
   */
  async get(groupId: string): Promise<AgentGroup> {
    return this.request('GET', `/groups/${groupId}`);
  }

  /**
   * Update a group definition.
   * @param groupId - Group identifier.
   * @param group - Updated AgentGroup.
   */
  async update(groupId: string, group: AgentGroup): Promise<AgentGroup> {
    return this.request('PUT', `/groups/${groupId}`, group);
  }

  /**
   * Delete an agent group.
   * @param groupId - Group identifier.
   */
  async delete(groupId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/groups/${groupId}`);
  }

  /**
   * Start all agents in a group.
   * @param groupId - Group identifier.
   */
  async run(groupId: string): Promise<GroupRunResponse> {
    return this.request('POST', `/groups/${groupId}/run`);
  }

  /**
   * Stop all agents in a group.
   * @param groupId - Group identifier.
   */
  async stop(groupId: string): Promise<GroupStopResponse> {
    return this.request('POST', `/groups/${groupId}/stop`);
  }
}

/** Workflow DAG definitions and run management. */
export class WorkflowsAPI {
  constructor(private readonly request: RequestFn) {}

  /** List all workflow definitions. */
  async list(): Promise<WorkflowDef[]> {
    return this.request('GET', '/workflows');
  }

  /**
   * Create a new workflow DAG.
   * @param workflow - WorkflowDef with steps and dependency graph.
   */
  async create(workflow: WorkflowDef): Promise<WorkflowDef> {
    return this.request('POST', '/workflows', workflow);
  }

  /**
   * Get a workflow definition.
   * @param workflowId - Workflow identifier.
   */
  async get(workflowId: string): Promise<WorkflowDef> {
    return this.request('GET', `/workflows/${workflowId}`);
  }

  /**
   * Delete a workflow definition.
   * @param workflowId - Workflow identifier.
   */
  async delete(workflowId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/workflows/${workflowId}`);
  }

  /**
   * Execute a workflow.
   * @param workflowId - Workflow identifier.
   */
  async run(workflowId: string): Promise<WorkflowRun> {
    return this.request('POST', `/workflows/${workflowId}/run`);
  }

  /**
   * List all runs for a workflow.
   * @param workflowId - Workflow identifier.
   */
  async runsList(workflowId: string): Promise<WorkflowRun[]> {
    return this.request('GET', `/workflows/${workflowId}/runs`);
  }

  /**
   * Get the current state of a workflow run.
   * @param runId - Run identifier.
   */
  async runGet(runId: string): Promise<WorkflowRun> {
    return this.request('GET', `/workflows/runs/${runId}`);
  }
}

/** Automatic architecture selection. */
export class ArchitectureAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * Analyse a WorkflowDef and select the optimal execution architecture.
   * @param workflow - WorkflowDef object (id, name, tenant_id, steps, etc.).
   */
  async select(workflow: WorkflowDef): Promise<ArchitectureDecision> {
    return this.request('POST', '/architecture/select', workflow);
  }

  /**
   * Analyse a previously saved workflow by ID.
   * @param workflowId - Workflow identifier stored in the node.
   */
  async selectSaved(workflowId: string): Promise<ArchitectureDecision> {
    return this.request('GET', `/architecture/select/${workflowId}`);
  }

  /**
   * Quick heuristic classification without a full WorkflowDef.
   * @param request - QuickClassifyRequest with tool/branch/governance metrics.
   */
  async classify(request: QuickClassifyRequest): Promise<ArchitectureDecision> {
    return this.request('POST', '/architecture/classify', request);
  }
}

/** Alert rules and event history (v2.2 extension). */
export class AlertsAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * List all alert rules for a tenant.
   * @param tenantId - Tenant identifier.
   */
  async listRules(tenantId: string): Promise<AlertRule[]> {
    return this.request('GET', `/alerts/${tenantId}/rules`);
  }

  /**
   * Create an alert rule for a tenant.
   * @param tenantId - Tenant identifier.
   * @param rule - AlertRule definition.
   */
  async createRule(tenantId: string, rule: AlertRule): Promise<AlertRule> {
    return this.request('POST', `/alerts/${tenantId}/rules`, rule);
  }

  /**
   * Delete an alert rule.
   * @param tenantId - Tenant identifier.
   * @param ruleId - Rule identifier.
   */
  async deleteRule(tenantId: string, ruleId: string): Promise<StatusResponse> {
    return this.request('DELETE', `/alerts/${tenantId}/rules/${ruleId}`);
  }

  /**
   * Get alert event history for a rule.
   * @param tenantId - Tenant identifier.
   * @param ruleId - Rule identifier.
   */
  async getHistory(tenantId: string, ruleId: string): Promise<AlertEvent[]> {
    return this.request('GET', `/alerts/${tenantId}/rules/${ruleId}/history`);
  }
}

/** Per-agent message bus for event-driven coordination (v2.2 extension). */
export class MessagesAPI {
  constructor(private readonly request: RequestFn) {}

  /**
   * Publish a message to an agent's channel.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param channel - Named message channel.
   * @param payload - JSON-serialisable message payload.
   * @param ttlSecs - Optional message TTL in seconds.
   */
  async publish(
    tenantId: string,
    agentId: string,
    channel: string,
    payload: unknown,
    ttlSecs?: number,
  ): Promise<BusMessage> {
    const body: Record<string, unknown> = { channel, payload };
    if (ttlSecs !== undefined) body['ttl_secs'] = ttlSecs;
    return this.request('POST', `/messages/${tenantId}/${agentId}`, body);
  }

  /**
   * Poll messages from a channel (dequeues messages).
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param channel - Named message channel.
   * @param limit - Maximum messages to return.
   */
  async poll(
    tenantId: string,
    agentId: string,
    channel: string,
    limit = 10,
  ): Promise<BusMessage[]> {
    return this.request(
      'GET',
      `/messages/${tenantId}/${agentId}/${channel}?limit=${limit}`,
    );
  }

  /**
   * Clear all messages from a channel.
   * @param tenantId - Tenant identifier.
   * @param agentId - Agent name.
   * @param channel - Named message channel.
   */
  async clear(tenantId: string, agentId: string, channel: string): Promise<StatusResponse> {
    return this.request('DELETE', `/messages/${tenantId}/${agentId}/${channel}`);
  }
}

// ---------------------------------------------------------------------------
// RequestFn type
// ---------------------------------------------------------------------------

type RequestFn = <T = unknown>(
  method: string,
  path: string,
  body?: unknown,
) => Promise<T>;

// ---------------------------------------------------------------------------
// Main client
// ---------------------------------------------------------------------------

/**
 * Apollo AI Agent Platform TypeScript client.
 *
 * Uses native fetch (Node 18+). No dependencies required.
 *
 * @example
 * ```typescript
 * const client = new ApolloClient('http://localhost:8080', { key: 'your-secret' });
 *
 * // List agents
 * const agents = await client.agents.list();
 *
 * // Run an agent
 * const instance = await client.agents.run('openclaw', 'alice');
 *
 * // Check health
 * const health = await client.health.agent('alice', 'openclaw');
 * console.log(`Health score: ${health.score}`);
 *
 * await client.close();
 * ```
 */
export class ApolloClient {
  /** Agent lifecycle management. */
  readonly agents: AgentsAPI;
  /** Per-tenant secret management. */
  readonly secrets: SecretsAPI;
  /** Usage metering and billing. */
  readonly usage: UsageAPI;
  /** Distributed tracing and span ingestion. */
  readonly traces: TracesAPI;
  /** Per-tenant governance policy. */
  readonly policy: PolicyAPI;
  /** Health intelligence and crash patterns. */
  readonly health: HealthAPI;
  /** Persistent agent memory store. */
  readonly memory: MemoryAPI;
  /** LLM model registry and routing. */
  readonly models: ModelsAPI;
  /** Cron / interval / one-shot job scheduler. */
  readonly schedule: ScheduleAPI;
  /** Agent deployment blueprints. */
  readonly blueprints: BlueprintsAPI;
  /** Agent groups — bulk lifecycle. */
  readonly groups: GroupsAPI;
  /** Workflow DAG management. */
  readonly workflows: WorkflowsAPI;
  /** Architecture selection engine. */
  readonly architecture: ArchitectureAPI;
  /** Alert rules and history. */
  readonly alerts: AlertsAPI;
  /** Per-agent message bus. */
  readonly messages: MessagesAPI;

  private readonly baseUrl: string;
  private readonly headers: Record<string, string>;
  private readonly timeoutMs: number;
  private _abortControllers: Set<AbortController> = new Set();

  /**
   * @param baseUrl - Apollo node base URL, e.g. "http://localhost:8080".
   * @param options - Authentication and timeout options.
   */
  constructor(baseUrl: string, options: ClientOptions) {
    if (!options.key && !options.jwt) {
      throw new Error("Either 'key' or 'jwt' must be provided in ClientOptions.");
    }

    this.baseUrl = baseUrl.replace(/\/$/, '');
    this.timeoutMs = options.timeout ?? 30_000;
    this.headers = { 'Content-Type': 'application/json' };

    if (options.key) {
      this.headers['X-Apollo-Key'] = options.key;
    } else if (options.jwt) {
      this.headers['Authorization'] = `Bearer ${options.jwt}`;
    }

    const req = this.request.bind(this) as RequestFn;
    this.agents = new AgentsAPI(req);
    this.secrets = new SecretsAPI(req);
    this.usage = new UsageAPI(req);
    this.traces = new TracesAPI(req);
    this.policy = new PolicyAPI(req);
    this.health = new HealthAPI(req);
    this.memory = new MemoryAPI(req);
    this.models = new ModelsAPI(req);
    this.schedule = new ScheduleAPI(req);
    this.blueprints = new BlueprintsAPI(req);
    this.groups = new GroupsAPI(req);
    this.workflows = new WorkflowsAPI(req);
    this.architecture = new ArchitectureAPI(req);
    this.alerts = new AlertsAPI(req);
    this.messages = new MessagesAPI(req);
  }

  /**
   * Check node health.
   * @returns Object with status and version.
   */
  async ping(): Promise<HealthEndpointResponse> {
    return this.request('GET', '/health');
  }

  /**
   * Get node capacity and identity metrics.
   * @returns NodeMetrics with active_agents, max_agents, node_id, region, version.
   */
  async metrics(): Promise<NodeMetrics> {
    return this.request('GET', '/metrics');
  }

  /**
   * Close all pending requests. Cancels in-flight requests via AbortController.
   */
  async close(): Promise<void> {
    for (const controller of this._abortControllers) {
      controller.abort();
    }
    this._abortControllers.clear();
  }

  private async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const url = `${this.baseUrl}${path}`;
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeoutMs);
    this._abortControllers.add(controller);

    try {
      const init: RequestInit = {
        method,
        headers: this.headers,
        signal: controller.signal,
      };

      if (body !== undefined) {
        init.body = JSON.stringify(body);
      }

      const response = await fetch(url, init);
      await raiseForStatus(response);

      // Handle empty bodies (204 No Content etc.)
      const contentType = response.headers.get('content-type') ?? '';
      if (!contentType.includes('application/json')) {
        return {} as T;
      }
      return (await response.json()) as T;
    } catch (error) {
      if (error instanceof ApolloError) throw error;
      if (error instanceof Error && error.name === 'AbortError') {
        throw new ApolloError(0, `Request timed out after ${this.timeoutMs}ms: ${method} ${path}`);
      }
      throw error;
    } finally {
      clearTimeout(timeoutId);
      this._abortControllers.delete(controller);
    }
  }
}
