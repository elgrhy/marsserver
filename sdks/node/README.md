# @apollo-platform/sdk

TypeScript/Node.js client for the [Apollo AI Agent Platform](https://github.com/elgrhy/apollo) v2.2.

Uses native `fetch` (Node 18+) — zero production dependencies.

## Installation

```bash
npm install @apollo-platform/sdk
```

## Quick Start

```typescript
import { ApolloClient } from '@apollo-platform/sdk';

const client = new ApolloClient('http://localhost:8080', { key: 'your-secret' });

// Check node health
const health = await client.ping();
console.log(health); // { status: 'ok', version: '2.0' }

// List and run agents
const agents = await client.agents.list();
const instance = await client.agents.run('openclaw', 'alice');
console.log(`Running: PID=${instance.pid}`);

// Check fleet health
const fleet = await client.health.fleet();
console.log(`Healthy: ${fleet.healthy_count}`);

await client.close();
```

### JWT Authentication

```typescript
const client = new ApolloClient('http://localhost:8080', { jwt: 'eyJ...' });
```

## API Reference

### Agents

```typescript
const agents = await client.agents.list();
const record  = await client.agents.add('./path/or/git-url');
const instance = await client.agents.run('agent-name', 'tenant-id');
await client.agents.stop('agent-name', 'tenant-id');
await client.agents.rollback('agent-name');
await client.agents.remove('agent-name');
```

### Secrets

```typescript
await client.secrets.put('tenant-id', { OPENAI_KEY: 'sk-...', TOKEN: 'abc' });
await client.secrets.delete('tenant-id');
```

### Usage

```typescript
const all = await client.usage.getAll();
const tenant = await client.usage.getTenant('tenant-id');
const reset = await client.usage.reset('tenant-id');
```

### Observability / Traces

```typescript
import type { TraceSpan } from '@apollo-platform/sdk';

const span: TraceSpan = {
  tenant_id: 'alice',
  agent_id: 'openclaw',
  name: 'llm_inference',
  status: 'ok',
  start_ts_ms: Date.now() - 1500,
  end_ts_ms: Date.now(),
  token_usage: {
    model: 'claude-sonnet-4-6',
    input_tokens: 500,
    output_tokens: 200,
    cost_usd: 0.002,
    provider: 'anthropic',
  },
};

const { trace_id } = await client.traces.postSpan(span);
const summary = await client.traces.finalize('alice', 'openclaw', trace_id);
const tokenStats = await client.traces.tokenStats('alice');
console.log(`Total cost: $${tokenStats.total_cost_usd.toFixed(4)}`);
```

### Policy / Governance

```typescript
import type { TenantPolicy } from '@apollo-platform/sdk';

const policy: TenantPolicy = {
  max_instances: 5,
  allowed_agents: ['openclaw', 'databot'],
  blocked_tools: ['bash', 'file_write'],
  data_residency: 'eu-west-1',
  max_tokens_per_day: 1_000_000,
  require_audit: true,
};

await client.policy.put('tenant-id', policy);
const current = await client.policy.get('tenant-id');
const report = await client.policy.compliance('tenant-id');
await client.policy.delete('tenant-id');
```

### Health Intelligence

```typescript
const agentHealth = await client.health.agent('tenant-id', 'agent-name');
console.log(`Score: ${agentHealth.score}`);

const tenantHealth = await client.health.tenant('tenant-id');
const fleet = await client.health.fleet();
```

### Memory

```typescript
await client.memory.put('alice', 'openclaw', 'prefs', {
  value: { theme: 'dark' },
  tags: ['profile'],
  text: 'user prefers dark mode',
});

const entry   = await client.memory.get('alice', 'openclaw', 'prefs');
const keys    = await client.memory.list('alice', 'openclaw');
const results = await client.memory.search('alice', 'openclaw', {
  query: 'dark theme',
  limit: 5,
});
const stats = await client.memory.stats('alice', 'openclaw');
await client.memory.delete('alice', 'openclaw', 'prefs');
await client.memory.clear('alice', 'openclaw');
```

### Model Routing

```typescript
import type { ModelRecord, RoutingRequest } from '@apollo-platform/sdk';

const model: ModelRecord = {
  model_id: 'claude-sonnet-4-6',
  provider: 'anthropic',
  cost_per_m_input: 3.0,
  cost_per_m_output: 15.0,
  latency_p50_ms: 800,
  latency_p99_ms: 2000,
  throughput_tok_s: 80,
  capabilities: ['text', 'code', 'function_calling'],
  context_window: 200_000,
  is_local: false,
  is_available: true,
  priority: 1,
};
await client.models.put('claude-sonnet-4-6', model);

const decision = await client.models.route({
  tenant_id: 'alice',
  input_tokens: 1000,
  output_tokens: 500,
  max_cost_usd: 0.05,
});
console.log(`Selected: ${decision.selected_model}`);
```

### Scheduler

```typescript
import type { ScheduledJob } from '@apollo-platform/sdk';

const job = await client.schedule.create({
  name: 'hourly-report',
  tenant_id: 'alice',
  agent_id: 'reporter',
  schedule: { type: 'cron', expression: '0 * * * *' },
  enabled: true,
});
const history = await client.schedule.history(job.id!);
await client.schedule.run(job.id!);      // manual trigger
await client.schedule.delete(job.id!);
```

### Blueprints

```typescript
const bp = await client.blueprints.create({
  name: 'Production Crawler',
  agent_id: 'openclaw',
  pin_version: '2.1.0',
  tags: ['prod'],
  region: 'us-east-1',
  default_env: { LOG_LEVEL: 'warn' },
});
await client.blueprints.deploy(bp.id!, 'alice');
```

### Groups

```typescript
const group = await client.groups.create({
  name: 'ETL Suite',
  tenant_id: 'alice',
  members: [
    { agent_id: 'extractor' },
    { agent_id: 'transformer' },
    { agent_id: 'loader' },
  ],
});
await client.groups.run(group.id!);
await client.groups.stop(group.id!);
```

### Workflows

```typescript
import type { WorkflowDef } from '@apollo-platform/sdk';

const wf = await client.workflows.create({
  name: 'ETL Pipeline',
  tenant_id: 'alice',
  steps: [
    { step_id: 'extract', name: 'Extract', agent_id: 'extractor', depends_on: [] },
    { step_id: 'transform', name: 'Transform', agent_id: 'transformer', depends_on: ['extract'] },
    { step_id: 'load', name: 'Load', agent_id: 'loader', depends_on: ['transform'] },
  ],
});
const run = await client.workflows.run(wf.id!);
const state = await client.workflows.runGet(run.run_id);
```

### Architecture Selection

```typescript
const decision = await client.architecture.classify({
  tenant_id: 'alice',
  tool_count: 4,
  parallel_branches: 2,
  error_tolerance: 1,
  governance_strict: false,
});
console.log(`Architecture: ${decision.architecture} (${(decision.confidence * 100).toFixed(1)}%)`);
```

## Error Handling

```typescript
import { ApolloError } from '@apollo-platform/sdk';

try {
  await client.agents.run('unknown', 'alice');
} catch (err) {
  if (err instanceof ApolloError) {
    console.error(`HTTP ${err.statusCode}: ${err.detail}`);
  }
}
```

## Building

```bash
npm run build   # compiles to dist/
npm run clean   # removes dist/
```

## Requirements

- Node.js 18+ (for native `fetch`)
- TypeScript 5.x (for development)

## License

MIT
