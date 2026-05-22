# APOLLO v2.0

**Production-grade distributed infrastructure runtime for autonomous AI agents.**

Apollo is a self-hosted execution engine and fleet coordination layer that gives infrastructure providers, IT teams, and SaaS platforms a secure, observable, governance-aware foundation for running agent workloads at scale. Deploy once, operate indefinitely — no developer involvement required.

**Status:** Production Certified — v2.0  
**Tests:** 60/60 unit tests passing · 0 warnings · Zero external ML dependencies

---

## What's New in v2.0

v2.0 adds the complete enterprise platform layer on top of the v1.2 execution engine — 8 new modules, 45 new REST endpoints, and 3 background intelligence loops:

| Layer | Module | What it does |
|-------|--------|-------------|
| Observability | `tracing.rs` | Step-level distributed tracing — spans, token usage, cost accounting |
| Governance | `policy.rs` | Per-tenant rules — agent/tool whitelists, data residency, quotas, model policies |
| Health Intelligence | `health.rs` | Health scoring 0–100, crash pattern detection, memory leak monitoring |
| Agent Memory | `memory.rs` | Per-agent persistent KV store with TF-IDF similarity search and TTL |
| Model Routing | `model_router.rs` | Cost/latency/policy-aware LLM model selection with EMA feedback |
| Scheduler | `scheduler.rs` | Cron, interval, and one-shot job scheduling with built-in cron parser |
| Orchestration | `orchestration.rs` | Blueprints, agent groups, workflow DAGs with dependency resolution |
| Arch Selector | `arch_selector.rs` | Automatic architecture selection — Deterministic / SingleAgent / MultiAgent |

---

## Architecture

```
┌────────────────────────────────────────────────────────────────────┐
│                       Provider Control Plane                       │
│             (your dashboard / billing / orchestration)             │
└──────────────────────────────┬─────────────────────────────────────┘
                               │ REST + webhooks (internal VPC)
              ┌────────────────▼─────────────────────┐
              │          Apollo Hub  :9191            │
              │  Region routing · Auto-scale alerts  │
              │  Agent catalog · Fleet health        │
              └──────┬───────────────┬───────────────┘
                     │ 10s poll      │ 10s poll
         ┌───────────▼──────┐  ┌─────▼────────────────┐
         │  Node :8443 (TLS)│  │  Node :8443 (TLS)    │  ...
         │  us-east-1       │  │  eu-west-1           │
         │                  │  │                      │
         │  ┌─ Observability │  │  ┌─ Policy Engine   │
         │  ├─ Health Intel  │  │  ├─ Model Router    │
         │  ├─ Scheduler     │  │  ├─ Agent Memory    │
         │  └─ Orchestration │  │  └─ Arch Selector   │
         │  tenant_1 → agent │  │  tenant_9001 → agt  │
         └──────────────────┘  └──────────────────────┘
```

| Component | Binary | Default Port | Role |
|-----------|--------|-------------|------|
| Apollo Node | `apollo` | `:8080` (HTTP) / `:8443` (TLS) | Execution engine + all platform layers |
| Apollo Hub | `apollo-hub` | `:9191` | Fleet coordinator, region routing, catalog |
| Apollo CLI | `apollo` | — | Operator: register, run, rollback, remove |
| Apollo Doctor | `apollo doctor` | — | Validates every deployment — 12 checks |

---

## Key Features

### v1.x — Execution Engine
- **Multi-tenant isolation** — per-tenant workspaces, process groups, env scrubbing
- **TLS / HTTPS** — native `rustls`; no reverse proxy needed
- **JWT + Key Auth** — multi-key rotation, scoped sub-operator JWT tokens
- **Secret injection** — mode `0600` storage; loaded only at spawn; never logged
- **Usage metering** — CPU-seconds, memory-GB-seconds per tenant; billing reset API
- **Persistent volumes** — survive restarts, rollbacks, and upgrades
- **Webhook events** — HMAC-SHA256 signed `AGENT_START` / `AGENT_STOP` / `SCALE_NEEDED`
- **Multi-region routing** — hub routes to least-loaded node per region
- **Runtime auto-provisioning** — Python, Node, Go, Deno, Bun, Ruby, PHP, Java, .NET, Rust, Shell, custom
- **Agent versioning + rollback** — one-command restore of any previous version

### v2.0 — Platform Layer

**Observability**  
Every agent execution step can POST a span to `/traces/{tenant}/{agent}/spans`. Spans carry token usage (model, input/output tokens, cost in USD), status, and timing. Finalized traces aggregate into billing-ready token stats per tenant.

**Governance & Policy**  
Per-tenant policy rules (`PUT /tenants/{id}/policy`) enforce agent whitelists, tool blocklists, data residency regions, daily token quotas, model restrictions, and mandatory audit logging. Policy is checked before every agent start — violators get `403` with a human-readable reason.

**Health Intelligence**  
A 30-second background loop samples every running process via `sysinfo`. Health scores drop on crashes (−15 each), high CPU (−10), memory growth trends (−20 for >50 MB/min), and latency spikes. Crash patterns are classified as `oom_loop`, `startup_crash`, or `periodic`.

**Agent Memory**  
Agents persist structured data across sessions: `PUT /memory/{tenant}/{agent}/{key}`. Keys carry JSON values, tags, an optional text field for search, and a TTL. Similarity search uses TF-IDF token overlap — no ML runtime required.

**Model Routing**  
Register LLM models with cost-per-million-tokens and latency benchmarks. `POST /models/route` returns the optimal model for a request given cost budget, required capabilities, local-only preference, and tenant policy. Latency estimates update via exponential moving average from real feedback.

**Scheduler**  
Create cron (full 5-field POSIX), interval, or one-shot jobs. A 30-second background loop fires due jobs as isolated agent spawns. Run history is persisted per job.

**Orchestration**  
- **Blueprints** — parameterized agent deploy templates with pinned versions, env defaults, and resource overrides
- **Agent Groups** — shared lifecycle management (start/stop all members at once)
- **Workflow DAGs** — steps with `depends_on` edges; `ready_steps_with_def()` drives topological execution

**Architecture Selection**  
Before running a workflow, `POST /architecture/select` analyses the DAG topology + tenant governance to score and pick the optimal execution model:

| Architecture | When selected |
|---|---|
| `deterministic` | 1 step, 1 agent, zero error tolerance, strict governance |
| `single_agent` | 2–5 steps, same agent, sequential or light branching |
| `multi_agent` | Multiple distinct agents, real parallelism, high fan-out |

Returns confidence score, per-architecture raw scores, top-6 reasoning chain, and an actionable config (max_concurrency, fail_fast, retry_eligible_steps, parallel_groups, governance_skip_candidates, suggested timeout).

---

## Installation

### Build from Source

```bash
git clone https://github.com/elgrhy/apollo.git
cd apollo
cargo build --release
sudo cp target/release/apollo /usr/local/bin/
sudo cp target/release/apollo-hub /usr/local/bin/
apollo doctor
```

Requires: Rust 1.75+, Git 2.30+

### Verify Installation

```
$ apollo doctor
[OK] Node Engine Initialized
[OK] Hub Connectivity Ready
[OK] Event Spine Active
[OK] Security Sandbox Enabled
[OK] Observability Layer Active
[OK] Policy Engine Active
[OK] Health Intelligence Active
[OK] Memory Layer Active
[OK] Model Router Active
[OK] Scheduler Active
[OK] Orchestration APIs Active
[OK] Architecture Selector Active
[OK] Runtimes Detected: python3, node, rustc, deno, ruby, ...
STATUS: PRODUCTION READY  [Apollo v2.0]
```

---

## CLI — Full Platform Coverage

The `apollo` binary is a complete operator CLI for the entire v2.0 platform — not just agent management. Every REST endpoint has a typed subcommand.

```
apollo [--node URL] [--key KEY] <command>

  doctor                     12-point system validation
  demo [--quick]             Offline guided tour of all platform modules
  guide [topic]              Built-in docs (quick-start | concepts | observability |
                             governance | health | memory | routing | scheduler |
                             orchestration | arch-selection | api)
  dashboard [--refresh N]    Live auto-refresh terminal fleet view (htop-style)

  ── Agent Management (v1.x) ──────────────────────────────────────────
  agent add / run / stop / list / rollback / remove

  ── v2.0 Platform ────────────────────────────────────────────────────
  traces   list / get / tokens
  policy   get / set / delete / compliance
  health   agent / tenant / fleet
  memory   get / put / delete / list / search / clear / stats
  models   list / add / remove / route / usage
  schedule list / create / get / delete / run / history
  blueprint list / create / get / delete / deploy
  group    list / create / get / delete / run / stop
  workflow  list / create / get / delete / run / runs / status / arch
  arch     select / classify
  usage    list / get / reset
```

Global flags apply to all v2.0 commands:
```
--node   http://localhost:8080   (env: APOLLO_NODE)
--key    apollo-dev-secret       (env: APOLLO_KEY)
```

**Run offline with no node:**
```bash
apollo demo --quick                   # 60-second highlights
apollo demo                           # full 10-module guided walkthrough
apollo guide quick-start              # installation walkthrough
apollo guide api                      # complete REST reference
```

**Live terminal dashboard:**
```bash
apollo dashboard --key my-secret      # redraws every 5s; [q] quit, [r] refresh
```

**Interactive shell** (no arguments):
```bash
apollo                                # shows banner + help menu; type commands directly
```

---

## Quick Start

```bash
# Start a node (dev — plain HTTP)
apollo node start --secret-keys "your-secret-key"

# Start a node (production — TLS)
apollo node start \
  --listen 0.0.0.0:8443 \
  --tls-cert /etc/apollo/node.crt \
  --tls-key  /etc/apollo/node.key \
  --secret-keys "key-1,key-2" \
  --jwt-secret "your-jwt-secret" \
  --webhook-url https://control.example.com/apollo-events \
  --region us-east-1

# Start the hub
apollo-hub start \
  --webhook-url https://control.example.com/apollo-scale \
  --scale-threshold 0.80

# Register an agent
apollo agent --base-dir .apollo add ./examples/openclaw
apollo agent --base-dir .apollo add https://github.com/org/agent.git

# Run an agent for a tenant
apollo agent --base-dir .apollo run openclaw --tenant user_123
```

---

## REST API

All node endpoints require `X-Apollo-Key: <key>` OR `Authorization: Bearer <HS256-JWT>`.

### v1.x — Node & Agents

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health` | Liveness check |
| `GET` | `/metrics` | Capacity, region, active agent count |
| `GET` | `/agents/list` | All registered agents |
| `POST` | `/agents/add` | Register from local path, URL, or git |
| `POST` | `/agents/run` | Start agent for a tenant (policy checked) |
| `DELETE` | `/agents/stop` | Stop and release resources |
| `POST` | `/agents/rollback` | Restore previous version |
| `POST` | `/agents/remove` | Permanently remove agent |
| `PUT` | `/tenants/:id/secrets` | Store secrets (0600) |
| `DELETE` | `/tenants/:id/secrets` | Delete secrets |
| `GET` | `/usage` | All-tenant usage |
| `GET` | `/usage/:id` | Per-tenant usage |
| `POST` | `/usage/:id/reset` | Billing cycle reset |

### v2.0 — Observability

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/traces/:tenant/:agent/spans` | Record a trace span |
| `GET` | `/traces/:tenant/:agent` | List trace summaries |
| `GET` | `/traces/:tenant/:agent/:trace_id` | Full trace with all spans |
| `POST` | `/traces/:tenant/:agent/:trace_id/finalize` | Build summary + token totals |
| `GET` | `/traces/:tenant/tokens` | Tenant token usage (billing) |

### v2.0 — Governance

| Method | Endpoint | Description |
|--------|----------|-------------|
| `PUT` | `/tenants/:id/policy` | Set governance rules |
| `GET` | `/tenants/:id/policy` | Retrieve policy |
| `DELETE` | `/tenants/:id/policy` | Reset to permissive default |
| `GET` | `/tenants/:id/compliance` | Compliance report |

### v2.0 — Health Intelligence

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/health/:tenant/:agent` | Score, crash pattern, resource trends |
| `GET` | `/health/:tenant` | All agents for a tenant |
| `GET` | `/health/fleet/summary` | Fleet-wide health overview |

### v2.0 — Agent Memory

| Method | Endpoint | Description |
|--------|----------|-------------|
| `PUT` | `/memory/:tenant/:agent/:key` | Store / update memory entry |
| `GET` | `/memory/:tenant/:agent/:key` | Retrieve entry |
| `DELETE` | `/memory/:tenant/:agent/:key` | Delete entry |
| `POST` | `/memory/:tenant/:agent/search` | TF-IDF similarity search |
| `GET` | `/memory/:tenant/:agent` | List all live keys |
| `DELETE` | `/memory/:tenant/:agent` | Clear all memory |

### v2.0 — Model Routing

| Method | Endpoint | Description |
|--------|----------|-------------|
| `PUT` | `/models/:model_id` | Register a model |
| `GET` | `/models` | List all models |
| `POST` | `/models/route` | Get routing recommendation |
| `POST` | `/models/feedback` | Report latency observation |
| `GET` | `/models/usage/:tenant` | Per-tenant model cost tracking |

### v2.0 — Scheduler

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/schedule` | Create job (cron / interval / once) |
| `GET` | `/schedule` | List all jobs |
| `GET` | `/schedule/:job_id` | Get job detail |
| `PUT` | `/schedule/:job_id` | Update job |
| `DELETE` | `/schedule/:job_id` | Delete job |
| `POST` | `/schedule/:job_id/run` | Manual trigger |
| `GET` | `/schedule/:job_id/history` | Run history |

### v2.0 — Orchestration

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/blueprints` | Create blueprint |
| `GET` | `/blueprints` | List blueprints |
| `GET` | `/blueprints/:id` | Get blueprint |
| `PUT` | `/blueprints/:id` | Update blueprint |
| `DELETE` | `/blueprints/:id` | Delete blueprint |
| `POST` | `/blueprints/:id/deploy` | Deploy blueprint for a tenant |
| `POST` | `/groups` | Create agent group |
| `GET` | `/groups/:id` | Get group |
| `POST` | `/groups/:id/run` | Start all group members |
| `POST` | `/groups/:id/stop` | Stop all group members |
| `POST` | `/workflows` | Define workflow DAG |
| `GET` | `/workflows/:id` | Get workflow definition |
| `POST` | `/workflows/:id/run` | Execute workflow |
| `GET` | `/workflows/runs/:run_id` | Get run status |

### v2.0 — Architecture Selection

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/architecture/select` | Analyse inline `WorkflowDef` → full decision |
| `GET` | `/architecture/select/:wf_id` | Analyse saved workflow by ID |
| `POST` | `/architecture/classify` | Lightweight heuristic (no WorkflowDef needed) |

```bash
# Analyse a workflow inline
curl -X POST http://localhost:8080/architecture/select \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{
    "id":"wf1","name":"ETL","description":"","tenant_id":"user_1",
    "steps":[
      {"step_id":"fetch","name":"Fetch","agent_id":"scraper","depends_on":[],"env":{},"optional":false,"timeout_secs":60},
      {"step_id":"analyze","name":"Analyze","agent_id":"llm","depends_on":[],"env":{},"optional":true,"timeout_secs":null},
      {"step_id":"store","name":"Store","agent_id":"db-agent","depends_on":["fetch","analyze"],"env":{},"optional":false,"timeout_secs":null}
    ],"created_at":0,"updated_at":0
  }'

# Response:
# {
#   "architecture": "multi_agent",
#   "confidence": 0.61,
#   "scores": { "deterministic": 10.0, "single_agent": 20.0, "multi_agent": 50.0 },
#   "reasoning": ["3 distinct agents required — multi-agent coordination needed", ...],
#   "config": { "max_concurrency": 2, "fail_fast": false, "retry_eligible_steps": ["analyze"], ... }
# }

# Quick heuristic classify
curl -X POST http://localhost:8080/architecture/classify \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"tenant_id":"user_1","tool_count":4,"parallel_branches":2,"error_tolerance":2,"governance_strict":false}'
```

### Hub API (`:9191`) — Internal Network Only

| Method | Endpoint | Description |
|--------|----------|-------------|
| `GET` | `/summary` | Fleet-wide nodes, agents, capacity |
| `GET` | `/nodes/status` | Per-node health |
| `GET` | `/nodes/best` | Least-loaded node (`?region=X`) |
| `GET` | `/catalog` | Aggregated agent catalog |
| `GET` | `/regions` | Per-region capacity breakdown |

---

## State Files

All state lives under `base_dir` (default `.apollo/`):

| Path | Contents |
|------|----------|
| `agents.json` | Registered agent catalog |
| `instances/{tenant_id}.json` | Running instances (sharded by tenant) |
| `secrets/{tenant_id}.json` | Per-tenant secrets (mode 0600) |
| `usage/{tenant_id}.json` | CPU-seconds, memory-GB-seconds, starts/stops |
| `policies/{tenant_id}.json` | Per-tenant governance policy |
| `traces/{tenant}/{agent}/{trace_id}.jsonl` | Span records (one JSON per line) |
| `health/{tenant}/{agent}.json` | Health score, crash history, resource trend |
| `memory/{tenant}/{agent}/store.json` | Persistent agent KV memory |
| `models/registry.json` | LLM model registry |
| `models/usage/{tenant_id}.json` | Per-tenant model cost tracking |
| `scheduler/jobs.json` | Scheduled jobs |
| `scheduler/history/{job_id}.jsonl` | Per-job run history |
| `blueprints/{id}.json` | Deployment blueprints |
| `groups/{id}.json` | Agent group definitions |
| `workflows/definitions/{id}.json` | Workflow DAG definitions |
| `workflows/runs/{run_id}.json` | Workflow run state |
| `agents/{name}/` | Copied agent package |
| `agents/{name}.v{ver}/` | Version backup for rollback |
| `tenants/{id}/{name}/` | Per-tenant isolated workspace |
| `volumes/{id}/{name}/{vol}/` | Persistent volume mounts |
| `logs/{id}/{name}.log` | Agent stdout/stderr (rotated at 10 MB) |
| `events.jsonl` | Append-only audit log |

---

## Security Model

| Control | Enforcement |
|---------|-------------|
| API authentication | `X-Apollo-Key` (multi-key rotation) OR `Authorization: Bearer` HS256-JWT |
| JWT scoping | `keys` claim issues constrained tokens per sub-operator |
| Rate limiting | Per-key token bucket 100 RPS; `429` on breach |
| TLS | `rustls` via `axum-server`; native — no reverse proxy needed |
| Secrets protection | Mode `0600`; loaded only at spawn; never in API responses or logs |
| Env scrubbing | `cmd.env_clear()` before spawn; only Apollo vars + tenant secrets + safe PATH |
| Process containment | Unix: `setpgid`/`killpg`; Windows: `CREATE_NEW_PROCESS_GROUP`/`taskkill /F /T` |
| FS isolation | `harden_path()` canonicalises + `starts_with(root)` before exec |
| Governance | `PolicyEngine::check_run()` before every agent start — returns 403 on deny |
| Audit trail | Append-only `events.jsonl` for all starts, stops, recoveries |
| Webhook integrity | HMAC-SHA256 `X-Apollo-Signature` on every outbound event |

---

## Agent Package Format

```yaml
name: openclaw
version: 1.0.0
runtime:
  type: python3        # python3 | node | go | deno | bun | ruby | php | java | dotnet | rust | shell | custom
  entry: main.py
  command: "python3 {entry}"   # optional override
  install:
    linux:   https://example.com/runtime-linux
    macos:   https://example.com/runtime-macos
llm:
  required: true
  provider: any
resources:
  cpu: 0.5
  memory: 512mb
  timeout: 120
volumes:
  - name: data
    size: 1gb
```

---

## Provider Integration Flow

```
1.  Register agents      POST /agents/add          — fetch, validate, auto-install runtime
2.  Store secrets        PUT /tenants/:id/secrets   — injected at spawn, never logged
3.  Set policy           PUT /tenants/:id/policy    — governance, residency, quotas
4.  Run agents           POST /agents/run           — policy checked; isolated workspace
5.  Record traces        POST /traces/:t/:a/spans   — per-step observability
6.  Billing usage        GET /usage/:id             — CPU + memory costs
7.                       GET /traces/:id/tokens     — LLM token costs
8.  Billing reset        POST /usage/:id/reset
9.  Monitor health       GET /health/:id/:agent     — score, crash pattern, trends
10. Agent memory         PUT /memory/:t/:a/:key     — cross-session state
11. Route models         POST /models/route         — cost/latency/policy-aware
12. Schedule agents      POST /schedule             — cron, interval, one-shot
13. Orchestrate          POST /workflows/:id/run    — DAG execution
14. Auto-select arch     POST /architecture/select  — Deterministic/SingleAgent/MultiAgent
```

---

## Background Tasks (Node Daemon)

| Task | Interval | Purpose |
|------|----------|---------|
| Metering loop | 60 s | CPU/memory samples → `usage/*.json` |
| Health loop | 30 s | Health scores + crash detection → `health/**/*.json` |
| Scheduler loop | 30 s | Fire due jobs → agent spawns |

---

## Systemd Services

**apollo-node.service:**

```ini
[Unit]
Description=APOLLO Node — Agent Execution Engine v2.0
After=network.target

[Service]
Type=simple
User=apollo
WorkingDirectory=/var/lib/apollo
EnvironmentFile=/etc/apollo/env
ExecStart=/usr/local/bin/apollo node start \
    --listen 0.0.0.0:8443 \
    --tls-cert /etc/apollo/node.crt \
    --tls-key  /etc/apollo/node.key \
    --base-dir /var/lib/apollo \
    --max-agents 200 \
    --secret-keys "${APOLLO_SECRET_KEYS}" \
    --jwt-secret  "${APOLLO_JWT_SECRET}" \
    --webhook-url "${APOLLO_WEBHOOK_URL}" \
    --region      "${APOLLO_REGION}"
Restart=on-failure
RestartSec=5s
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/var/lib/apollo /etc/apollo

[Install]
WantedBy=multi-user.target
```

---

## SLA Summary

| Metric | Target |
|--------|--------|
| Node process availability | 99.5% per calendar month |
| Automatic restart after crash | < 10 seconds |
| Node API response time (p99) | < 200 ms |
| Agent startup latency | < 2 seconds |
| Agent stop latency | < 1 second |
| Health score update interval | 30 seconds |
| Scheduler check interval | 30 seconds |
| Hub health poll interval | 10 seconds |
| Hub failure detection | ≤ 20 seconds |
| Max agent density per node | 200 (configurable) |

---

## Documentation

| Document | Purpose |
|----------|---------|
| [CLAUDE.md](CLAUDE.md) | Full API reference with curl examples for all 55+ endpoints |
| [Enterprise Handoff Pack](docs/HANDOFF.md) | Master operator documentation index |
| [Quick Start Guide](docs/quick_start.md) | Step-by-step installation and first run |
| [Production Deployment](docs/production_deployment.md) | systemd, directory layout, log management |
| [Network & Security Guide](docs/network_security.md) | Ports, firewall rules, TLS, JWT, key rotation |
| [SLA](docs/sla.md) | Availability, recovery, and performance guarantees |
| [Provider Integration Guide](provider_integration_guide.md) | End-to-end integration walkthrough |
| [Enterprise Approval Pack](enterprise_approval_pack.md) | SOC2 summary, FMEA, compliance checklist |

---

## Requirements

| | Minimum |
|-|---------|
| OS | Linux x86_64/aarch64 (Ubuntu 20.04+, RHEL 8+), macOS, Windows |
| Rust | 1.75+ |
| Git | 2.30+ |
| RAM | 512 MB per node |
| Disk | 2 GB |
| Network | Internal VPC; no public internet required at runtime |

---

## Provider & UI Integration

Apollo is **API-first**. The REST API is the integration surface for every external system — no proprietary SDK required. Any provider, cloud platform, or enterprise tool can connect:

| Integration type | How |
|---|---|
| **AWS / GCP / Azure control plane** | Call the REST API from your VPC; use `X-Apollo-Key` or HS256-JWT |
| **Custom web dashboard** | Build any UI that calls the same REST endpoints the CLI uses |
| **Billing system** | `GET /usage/:tenant` + `GET /traces/:tenant/tokens` — CPU, memory, LLM costs |
| **Alerting / PagerDuty** | `GET /health/fleet/summary` → trigger on degraded/critical counts |
| **Workflow orchestrator** | `POST /workflows/:id/run` → poll `GET /workflows/runs/:run_id` |
| **Kubernetes / Helm** | Deploy `apollo node start` as a `Deployment`; expose with `Service` |
| **Service mesh** | Node speaks standard HTTPS/TLS; plug in Istio/Envoy like any service |

The Apollo CLI itself is built on top of the same REST API — there's no private interface. Anything the CLI does, any provider dashboard can do identically.

---

## Deferred Roadmap (v3.0)

- **Kubernetes operator / Helm chart** — `deploy/helm/` for enterprise K8s deployments
- **Official Python / Node / Go SDK** — `apollo.run_agent(tenant, agent)` thin client libraries
- **Parallel workflow execution** — concurrent DAG step dispatch within a single workflow run
- **Real-time alerting** — health score drops / budget exhaustion → PagerDuty / Slack / email
- **Vector storage** — embedding-based memory search (Qdrant / pgvector / HNSW)
- **Agent-to-agent messaging** — runtime pub/sub bus between running agents

---

*Apollo v2.0 — CLI · Dashboard · Demo · Guide · Observability · Governance · Health · Memory · Model Routing · Scheduler · Orchestration · Architecture Selection*
