# APOLLO Platform — Technical Report v2.2

**Date:** 2026-05-29
**Version:** 2.2.0 (Production Certified)
**Repository:** github.com/elgrhy/apollo
**Tests:** 60/60 passing · 0 failures · 0 panics

---

## Executive Summary

Apollo is a self-hosted AI agent execution engine and fleet coordination platform. It provides the full infrastructure stack required to run autonomous AI agents at enterprise scale: secure multi-tenant isolation, per-step observability, governance enforcement, health intelligence, and model-cost routing — all in a single deployable Rust binary.

**v2.2** adds real-time alerting, agent-to-agent messaging, upgraded vector search, true parallel workflow execution, multi-language SDKs, a Kubernetes operator, a Helm chart, and an embedded web dashboard — while maintaining 100% backward compatibility with all v2.0/v2.1 APIs.

---

## Architecture

Five crates in a Cargo workspace:

| Crate | Binary | Role |
|-------|--------|------|
| apollo-core | — | Shared library: all platform modules |
| apollo-runtime | — | Cross-platform agent process spawning |
| apollo-node | apollo | HTTP/HTTPS API server + CLI |
| apollo-hub | apollo-hub | Fleet coordinator |
| apollo-operator | apollo-operator | Kubernetes operator (standalone build) |

---

## Module Reference

### v1.x — Execution Engine

| Module | Purpose |
|--------|---------|
| types.rs | Core data types |
| agents.rs | Registry CRUD, URL/git sourcing, versioning + rollback |
| detect.rs | Node capability detection |
| fetch.rs | HTTP archive + git clone with URL injection guard (v2.1) |
| runtime_registry.rs | Launch dispatch, SHA-256 verified auto-install (v2.1) |
| secrets.rs | AES-256-GCM encrypted per-tenant secrets (v2.1) |
| usage.rs | CPU-seconds, memory-GB-seconds metering |
| webhook.rs | HMAC-SHA256 signed outbound lifecycle events |

### v2.0 — Platform Layer

| Module | Purpose |
|--------|---------|
| tracing.rs | Step-level distributed tracing; append-only + dedup (v2.1 race fix) |
| policy.rs | Per-tenant governance: agent/tool whitelists, residency, quotas |
| health.rs | Health scoring 0-100, crash pattern detection, memory leak monitoring |
| memory.rs | Per-agent KV store; cosine TF-IDF search (v2.2); optional Qdrant |
| model_router.rs | Cost/latency/policy-aware LLM routing with EMA feedback |
| scheduler.rs | Cron, interval, one-shot scheduling |
| orchestration.rs | Blueprints, agent groups, parallel workflow DAGs (v2.2) |
| arch_selector.rs | Automatic architecture selection (Deterministic/Single/Multi) |

### v2.2 — New Capabilities

| Module | Purpose |
|--------|---------|
| alerting.rs | Alert rules with Slack/PagerDuty/webhook delivery and cooldown |
| messaging.rs | Agent-to-agent topic pub/sub; sequence polling; TTL expiry |

---

## v2.1 Security Hardening — All Issues Resolved

### Critical (3/3 fixed)

| Issue | Fix Applied |
|-------|-------------|
| Hardcoded apollo-dev-secret in 4 places | Node and hub refuse start without explicit keys. No fallback anywhere. |
| Hub API had zero authentication | All hub routes require X-Hub-Key / Authorization Bearer. Hub requires --hub-key. |
| JWT empty keys claim bypass (OR logic) | Fixed to AND. Empty keys claim rejected. |

### High (5/5 fixed)

| Issue | Fix Applied |
|-------|-------------|
| H1: Plaintext secrets on disk | AES-256-GCM encryption. Auto-generated master key at .master.key (mode 0600). |
| H2: Runtime downloads unverified | sha256 field in RuntimeInstallConfig. Mismatch aborts install. |
| H3: upsert_span() concurrent race | Append-only writes. load_trace() deduplicates by span_id on read. |
| H4: Dead NodeNetworkPolicy fields | allow_localhost/allow_private_ranges removed (never enforced). |
| H5: Rate limiter mutex unwrap crash | unwrap_or_else(|p| p.into_inner()) — recovers from lock poison. |

### Medium (8/8 fixed)

- Non-unique node IDs → UUID v4
- Policy denials not audited → events.jsonl + tracing::warn!
- Git URL injection → validate_git_url() blocks --upload-pack and metacharacters
- Archive path traversal → canonicalize + starts_with guard
- Hub reqwest unwrap → .expect() with message
- Interactive shell split_whitespace → shell-words crate
- Hub poller no shutdown → CancellationToken for clean SIGTERM
- events.jsonl unbounded → rotates at 100 MB, 3 generations

---

## v2.2 Feature Detail

### 1. Real-Time Alerting

Endpoints: PUT/GET/DELETE /alerts/rules, GET /alerts/history

Alert metrics:
- health_score — fires when agent health drops below threshold
- token_budget — fires when tenant daily tokens exceed threshold (millions)
- crash_count — fires when crash count exceeds threshold in sliding window
- fleet_utilization — fires when fleet stress exceeds threshold (0.0-1.0)

Delivery channels: Slack (incoming webhook), PagerDuty (Events API v2, severity levels),
Webhook (generic HTTP POST with HMAC-SHA256 signature).

Cooldown: configurable per rule (default 300 s). History stored as JSONL.
Background evaluation: every 30 s, consistent with health and scheduler loops.

### 2. Agent-to-Agent Messaging Bus

Topic-based pub/sub for runtime communication between agents.
Messages are persisted (JSONL), sequence-numbered, and TTL-aware.

Endpoints:
  POST /messages/:topic               publish message
  GET  /messages/:topic?since=N       poll (returns messages with seq > N)
  GET  /messages/:topic/latest        peek latest N messages
  DELETE /messages/:topic             clear topic
  GET  /messages                      list topics with stats

Polling model: consumer tracks last seen seq number and polls for new messages.
No long-polling or SSE required (planned for v2.3).

### 3. Vector Memory Search (Cosine TF-IDF)

Memory search upgraded from Jaccard token overlap to cosine TF-IDF similarity.

Algorithm:
  1. Compute per-term IDF across all entries in the store
  2. Compute TF-IDF weight vectors for query and each document
  3. Cosine similarity = normalized dot product of weight vectors

Improvement over Jaccard: handles partial matches, term frequency weighting,
and inverse document frequency penalisation of common terms.

Optional Qdrant backend for tenants with pre-computed embeddings:
  Start node with: --qdrant-url http://localhost:6333
  Store with: PUT /memory/.../key with embedding: [...] in body
  Search with: POST /memory/.../search with embedding: [...] in body
  Falls back to TF-IDF when no embedding provided.

### 4. Parallel Workflow Execution

Workflows now use true parallel DAG execution via futures::future::join_all.

Background executor per workflow run:
  1. Load run state
  2. Get all ready steps (pending + all deps complete/skipped)
  3. Mark all ready as Running, save state
  4. Spawn all as concurrent async futures
  5. join_all — wait for entire wave to complete
  6. Update step states from results
  7. Repeat until all steps terminal

Optional step failure → Skipped (workflow continues).
Non-optional step failure → Failed (workflow terminates).
Workflow status: Pending → Running → Completed | Failed.

### 5. Web Dashboard

GET /dashboard serves a self-contained HTML dashboard (no build step).

Sections: Fleet Overview, Agents, Health (color-coded scores), Traces,
Usage per tenant, Model registry, Alerts, Messages.

Features: auto-refresh every 10 s, dark/light mode, settings panel
(node URL + API key stored in localStorage), per-section error isolation.

### 6. Kubernetes Operator

apollo-operator watches ApolloAgent CRDs and reconciles against the Apollo
node REST API. Reconciliation loop every 30 s:
  1. Resolve API key from referenced K8s Secret
  2. Register agent if agentSource provided
  3. Count running instances for tenant
  4. Start/stop to match spec.replicas
  5. Update CRD status (phase, lastSyncAt)

Built standalone: cd crates/apollo-operator && cargo build --release

### 7. Helm Chart

Location: deploy/helm/apollo/

Components:
  apollo-node    StatefulSet     PVC per replica for .apollo/ state
  apollo-hub     Deployment      Rolling update, hub-key from Secret
  apollo-operator Deployment     Optional; RBAC for CRD watch
  ApolloAgent CRD               Pre-install hook, resource-policy: keep
  HPA                           Scale node on CPU/memory (optional)
  Ingress                       Route to node + hub (optional)

### 8. Multi-Language SDKs

  Python     sdks/python/    pip install apollo-sdk
  Node.js/TS sdks/node/      npm install @apollo-platform/sdk
  Go         sdks/go/        go get github.com/elgrhy/apollo/sdk/go

All SDKs cover the complete v2.2 API surface including alerting and messaging.

---

## Complete REST API

Node auth: X-Apollo-Key: KEY  OR  Authorization: Bearer JWT
Hub auth:  X-Hub-Key: KEY     OR  Authorization: Bearer KEY  (v2.1+)

v1.x — Core
  GET    /health
  GET    /metrics
  GET    /agents/list
  POST   /agents/add
  POST   /agents/run           (policy checked)
  DELETE /agents/stop
  POST   /agents/rollback
  POST   /agents/remove
  PUT    /tenants/:id/secrets  (AES-256-GCM encrypted)
  DELETE /tenants/:id/secrets
  GET    /usage
  GET    /usage/:id
  POST   /usage/:id/reset

v2.0 — Platform
  POST   /traces/:t/:a/spans
  GET    /traces/:t/:a
  GET    /traces/:t/:a/:id
  POST   /traces/:t/:a/:id/finalize
  GET    /traces/:t/tokens
  PUT/GET/DELETE /tenants/:id/policy
  GET    /tenants/:id/compliance
  GET    /health/fleet/summary
  GET    /health/:t/:a
  GET/PUT/DELETE /memory/:t/:a/:key
  POST   /memory/:t/:a/search   (cosine TF-IDF or Qdrant)
  GET    /models
  POST   /models/route
  POST   /schedule
  POST   /schedule/:id/run
  POST   /blueprints/:id/deploy
  POST   /groups/:id/run
  POST   /workflows/:id/run     (parallel DAG, v2.2)
  POST   /architecture/select
  POST   /architecture/classify

v2.2 — New
  GET    /alerts/rules
  POST   /alerts/rules
  DELETE /alerts/rules/:id
  GET    /alerts/history
  GET    /messages
  POST   /messages/:topic
  GET    /messages/:topic       (?since=N&limit=M)
  GET    /messages/:topic/latest
  DELETE /messages/:topic
  GET    /dashboard

Hub (port 9191) — all require X-Hub-Key
  GET    /summary
  GET    /nodes/status
  GET    /nodes/best
  GET    /catalog
  GET    /regions

---

## State File Layout

{base_dir}/              (default: .apollo/)
.master.key              AES-256-GCM master key (mode 0600, auto-generated)
agents.json              Registered agent catalog
instances/               Running instances, sharded by tenant
secrets/                 AES-256-GCM encrypted tenant secrets
usage/                   CPU-seconds + memory-GB-seconds per tenant
policies/                Per-tenant governance rules
traces/                  JSONL span records (append-only; dedup on read)
health/                  Health records (score, crashes, resource trend)
memory/                  TF-IDF KV store (+ optional Qdrant sync)
models/                  LLM registry + per-tenant usage
scheduler/               Jobs + per-job run history
alerts/                  Alert rules + firing history
messages/                Topic-based message buses (JSONL, sequence-indexed)
blueprints/              Deployment templates
groups/                  Agent group definitions
workflows/               DAG definitions + run state
tenants/                 Per-tenant isolated workspaces
volumes/                 Persistent volume mounts
logs/                    Agent stdout/stderr (10 MB rotation, 5 generations)
events.jsonl             Audit log (100 MB rotation, 3 generations)

---

## Environment Variables

APOLLO_SECRET_KEYS   Node    Comma-separated API keys (required)
APOLLO_JWT_SECRET    Node    JWT HMAC signing key
APOLLO_HUB_KEY       Hub     Hub API authentication key (required)
APOLLO_NODE          CLI     Node URL (default: http://localhost:8080)
APOLLO_KEY           CLI     API key for CLI commands
APOLLO_QDRANT_URL    Node    Qdrant URL for vector memory (optional)
APOLLO_SCALE_WEBHOOK Hub     Scale event webhook URL
RUST_LOG             Both    Log level (info/debug/warn/error)

---

## Build & Operations

  cargo build --release          Build node + hub
  cargo test --workspace         Run 60 tests
  apollo node start --secret-keys "$(openssl rand -hex 32)"
  apollo-hub start --hub-key "$(openssl rand -hex 32)"
  apollo doctor                  12-point validation

Kubernetes:
  helm install apollo deploy/helm/apollo/ \
    --set node.secretKeys="$(openssl rand -hex 32)" \
    --set hub.hubKey="$(openssl rand -hex 32)"

---

## Deferred Roadmap (v2.3+)

HNSW in-process vector index     Replace file-scan TF-IDF for large stores
SSE streaming for messages        Real-time push vs polling
Dashboard live log tailing        WebSocket stream from logs/*.log
Token-window workflow pausing     Pause workflow when budget nears limit
Email alert channel               SMTP delivery for AlertChannel::Email
Python SDK async variant          httpx-based async client
Multi-node workflow routing       Route DAG steps to different nodes
