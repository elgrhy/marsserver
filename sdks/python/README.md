# Apollo SDK — Python

Python client for the [Apollo AI Agent Platform](https://github.com/elgrhy/apollo) v2.2.

## Installation

```bash
pip install apollo-sdk
```

Or from source:

```bash
cd sdks/python
pip install -e .
```

**Requirements:** Python 3.9+, `httpx`, `pydantic`

## Quick Start

### Async (recommended)

```python
import asyncio
from apollo_sdk import ApolloClient

async def main():
    async with ApolloClient("http://localhost:8080", key="your-secret") as client:
        # Check node health
        health = await client.ping()
        print(health)  # {"status": "ok", "version": "2.0"}

        # Register and start an agent
        record = await client.agents.add("./examples/openclaw")
        instance = await client.agents.run("openclaw", tenant="alice")
        print(f"Running: PID={instance.pid}")

        # Check health
        h = await client.health.agent("alice", "openclaw")
        print(f"Health score: {h.score}")

asyncio.run(main())
```

### Sync

```python
from apollo_sdk import SyncApolloClient

with SyncApolloClient("http://localhost:8080", key="your-secret") as client:
    agents = client.agents.list()
    fleet = client.health.fleet()
    print(f"Healthy agents: {fleet.healthy_count}")
```

### JWT Authentication

```python
from apollo_sdk import ApolloClient

async with ApolloClient("http://localhost:8080", jwt="eyJ...") as client:
    agents = await client.agents.list()
```

## API Reference

All methods are async on `ApolloClient` and synchronous on `SyncApolloClient`.

### Agents

```python
agents = await client.agents.list()
record = await client.agents.add("./path/or/git-url")
instance = await client.agents.run("agent-name", "tenant-id")
await client.agents.stop("agent-name", "tenant-id")
await client.agents.rollback("agent-name")
await client.agents.remove("agent-name")
```

### Secrets

```python
await client.secrets.put("tenant-id", {"OPENAI_KEY": "sk-...", "TOKEN": "abc"})
await client.secrets.delete("tenant-id")
```

### Usage / Billing

```python
all_usage = await client.usage.get_all()
tenant_usage = await client.usage.get_tenant("tenant-id")
await client.usage.reset("tenant-id")  # billing cycle rollover
```

### Observability / Traces

```python
from apollo_sdk import TraceSpan, TokenUsage
import time

span = TraceSpan(
    tenant_id="alice",
    agent_id="openclaw",
    name="llm_inference",
    status="ok",
    start_ts_ms=int(time.time() * 1000) - 1500,
    end_ts_ms=int(time.time() * 1000),
    token_usage=TokenUsage(
        model="claude-sonnet-4-6",
        input_tokens=500,
        output_tokens=200,
        cost_usd=0.002,
        provider="anthropic",
    ),
)
ids = await client.traces.post_span(span)
summary = await client.traces.finalize("alice", "openclaw", ids["trace_id"])
token_stats = await client.traces.token_stats("alice")
```

### Policy / Governance

```python
from apollo_sdk import TenantPolicy

policy = TenantPolicy(
    max_instances=5,
    allowed_agents=["openclaw", "databot"],
    blocked_tools=["bash", "file_write"],
    data_residency="eu-west-1",
    max_tokens_per_day=1_000_000,
    require_audit=True,
)
await client.policy.put("tenant-id", policy)
current = await client.policy.get("tenant-id")
report = await client.policy.compliance("tenant-id")
await client.policy.delete("tenant-id")
```

### Health Intelligence

```python
agent_health = await client.health.agent("tenant-id", "agent-name")
tenant_health = await client.health.tenant("tenant-id")
fleet = await client.health.fleet()
```

### Memory

```python
await client.memory.put("alice", "openclaw", "prefs", {"theme": "dark"},
                        tags=["profile"], text="user prefers dark mode")
entry = await client.memory.get("alice", "openclaw", "prefs")
keys = await client.memory.list("alice", "openclaw")
results = await client.memory.search("alice", "openclaw", "dark theme", limit=5)
stats = await client.memory.stats("alice", "openclaw")
await client.memory.delete("alice", "openclaw", "prefs")
await client.memory.clear("alice", "openclaw")
```

### Model Routing

```python
from apollo_sdk import ModelRecord, RoutingRequest

await client.models.put("claude-sonnet-4-6", ModelRecord(
    model_id="claude-sonnet-4-6",
    provider="anthropic",
    cost_per_m_input=3.0,
    cost_per_m_output=15.0,
    latency_p50_ms=800,
    latency_p99_ms=2000,
    throughput_tok_s=80,
    capabilities=["text", "code", "function_calling"],
    context_window=200_000,
    is_available=True,
    priority=1,
))

decision = await client.models.route(RoutingRequest(
    tenant_id="alice",
    input_tokens=1000,
    output_tokens=500,
    max_cost_usd=0.05,
))
print(f"Selected: {decision.selected_model}")
```

### Scheduler

```python
from apollo_sdk import ScheduledJob

job = await client.schedule.create(ScheduledJob(
    name="hourly-report",
    tenant_id="alice",
    agent_id="reporter",
    schedule={"type": "cron", "expression": "0 * * * *"},
    enabled=True,
))
history = await client.schedule.history(job.id)
await client.schedule.run(job.id)          # manual trigger
await client.schedule.delete(job.id)
```

### Blueprints

```python
from apollo_sdk import Blueprint

bp = await client.blueprints.create(Blueprint(
    name="Production Crawler",
    agent_id="openclaw",
    pin_version="2.1.0",
    tags=["prod"],
    region="us-east-1",
    default_env={"LOG_LEVEL": "warn"},
))
await client.blueprints.deploy(bp.id, "alice")
```

### Groups

```python
from apollo_sdk import AgentGroup, GroupMember

group = await client.groups.create(AgentGroup(
    name="ETL Suite",
    tenant_id="alice",
    members=[
        GroupMember(agent_id="extractor"),
        GroupMember(agent_id="transformer"),
        GroupMember(agent_id="loader"),
    ],
))
await client.groups.run(group.id)
await client.groups.stop(group.id)
```

### Workflows

```python
from apollo_sdk import WorkflowDef, WorkflowStep

wf = await client.workflows.create(WorkflowDef(
    name="ETL Pipeline",
    tenant_id="alice",
    steps=[
        WorkflowStep(step_id="extract", name="Extract",
                     agent_id="extractor", depends_on=[]),
        WorkflowStep(step_id="transform", name="Transform",
                     agent_id="transformer", depends_on=["extract"]),
        WorkflowStep(step_id="load", name="Load",
                     agent_id="loader", depends_on=["transform"]),
    ],
))
run = await client.workflows.run(wf.id)
state = await client.workflows.run_get(run.run_id)
```

### Architecture Selection

```python
from apollo_sdk import QuickClassifyRequest

decision = await client.architecture.classify(QuickClassifyRequest(
    tenant_id="alice",
    tool_count=4,
    parallel_branches=2,
    error_tolerance=1,
    governance_strict=False,
))
print(f"Architecture: {decision.architecture}  Confidence: {decision.confidence:.2f}")
```

## Error Handling

All errors from non-2xx responses raise `ApolloError`:

```python
from apollo_sdk import ApolloError

try:
    await client.agents.run("unknown-agent", "alice")
except ApolloError as e:
    print(f"HTTP {e.status_code}: {e.detail}")
```

## End-to-End Example

See [`examples/quickstart.py`](examples/quickstart.py) for a full walkthrough.

## License

MIT
