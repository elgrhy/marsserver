"""
Apollo AI Agent Platform — Python client (sync + async).

Async usage (recommended):
    async with ApolloClient("http://localhost:8080", key="secret") as client:
        agents = await client.agents.list()

Sync usage:
    client = SyncApolloClient("http://localhost:8080", key="secret")
    agents = client.agents.list()
"""

from __future__ import annotations

import asyncio
import functools
from typing import Any, Dict, List, Optional

import httpx

from .models import (
    AgentRecord,
    AgentInstance,
    AlertEvent,
    AlertRule,
    ArchitectureDecision,
    Blueprint,
    AgentGroup,
    BusMessage,
    ComplianceReport,
    FleetHealthSummary,
    HealthRecord,
    JobRunRecord,
    MemoryEntry,
    MemoryQuery,
    MemoryStats,
    ModelFeedback,
    ModelRecord,
    ModelUsageRecord,
    QuickClassifyRequest,
    RoutingDecision,
    RoutingRequest,
    ScheduledJob,
    TenantPolicy,
    TenantTokenStats,
    TraceSummary,
    TraceSpan,
    UsageRecord,
    WorkflowDef,
    WorkflowRun,
)


# ---------------------------------------------------------------------------
# Internal helpers
# ---------------------------------------------------------------------------

class ApolloError(Exception):
    """Raised when the Apollo node returns a non-2xx response."""
    def __init__(self, status_code: int, detail: str) -> None:
        self.status_code = status_code
        self.detail = detail
        super().__init__(f"ApolloError [{status_code}]: {detail}")


def _raise_for_status(response: httpx.Response) -> httpx.Response:
    """Raise ApolloError for non-2xx responses with structured error body."""
    if response.is_error:
        try:
            body = response.json()
            detail = body.get("error", body.get("message", response.text))
        except Exception:
            detail = response.text
        raise ApolloError(response.status_code, detail)
    return response


# ---------------------------------------------------------------------------
# Resource groups — one class per API namespace
# ---------------------------------------------------------------------------

class AgentsAPI:
    """Agent lifecycle management: register, start, stop, rollback, remove."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list(self) -> List[AgentRecord]:
        """List all registered agent records.

        Returns:
            List of AgentRecord objects.
        """
        r = _raise_for_status(await self._http.get("/agents/list"))
        return [AgentRecord.model_validate(a) for a in r.json()]

    async def add(self, source: str) -> AgentRecord:
        """Register an agent from a local path, HTTPS archive, or git URL.

        Args:
            source: Local path, HTTPS archive URL, or git repository URL.

        Returns:
            The newly registered AgentRecord.
        """
        r = _raise_for_status(await self._http.post("/agents/add", json={"source": source}))
        return AgentRecord.model_validate(r.json())

    async def run(self, agent: str, tenant: str) -> AgentInstance:
        """Start an agent for a specific tenant.

        Args:
            agent: Registered agent name.
            tenant: Tenant / user ID.

        Returns:
            The created AgentInstance.
        """
        r = _raise_for_status(
            await self._http.post("/agents/run", json={"agent": agent, "tenant": tenant})
        )
        return AgentInstance.model_validate(r.json())

    async def stop(self, agent: str, tenant: str) -> Dict[str, str]:
        """Stop a running agent instance.

        Args:
            agent: Agent name.
            tenant: Tenant ID.

        Returns:
            Status dict e.g. {"status": "stopped"}.
        """
        r = _raise_for_status(
            await self._http.request(
                "DELETE", "/agents/stop", json={"agent": agent, "tenant": tenant}
            )
        )
        return r.json()

    async def rollback(self, agent: str) -> Dict[str, str]:
        """Roll back an agent to its previous version.

        Args:
            agent: Agent name.

        Returns:
            Status dict e.g. {"status": "rolled_back"}.
        """
        r = _raise_for_status(await self._http.post("/agents/rollback", json={"agent": agent}))
        return r.json()

    async def remove(self, agent: str) -> Dict[str, str]:
        """Permanently remove an agent from the registry.

        Args:
            agent: Agent name.

        Returns:
            Status dict e.g. {"status": "removed"}.
        """
        r = _raise_for_status(await self._http.post("/agents/remove", json={"agent": agent}))
        return r.json()


class SecretsAPI:
    """Per-tenant secret management."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def put(self, tenant_id: str, secrets: Dict[str, str]) -> Dict[str, str]:
        """Store or update secrets for a tenant.

        Secrets are persisted at mode 0600 and injected into agent env at start.

        Args:
            tenant_id: Tenant identifier.
            secrets: Key-value mapping of secret names to values.

        Returns:
            Status dict e.g. {"status": "saved"}.
        """
        r = _raise_for_status(
            await self._http.put(f"/tenants/{tenant_id}/secrets", json={"secrets": secrets})
        )
        return r.json()

    async def delete(self, tenant_id: str) -> Dict[str, str]:
        """Delete all secrets for a tenant.

        Args:
            tenant_id: Tenant identifier.

        Returns:
            Status dict e.g. {"status": "deleted"}.
        """
        r = _raise_for_status(await self._http.delete(f"/tenants/{tenant_id}/secrets"))
        return r.json()


class UsageAPI:
    """Compute metering and billing cycle management."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def get_all(self) -> List[UsageRecord]:
        """Get usage records for all tenants.

        Returns:
            List of UsageRecord objects.
        """
        r = _raise_for_status(await self._http.get("/usage"))
        return [UsageRecord.model_validate(u) for u in r.json()]

    async def get_tenant(self, tenant_id: str) -> UsageRecord:
        """Get usage record for a specific tenant.

        Args:
            tenant_id: Tenant identifier.

        Returns:
            UsageRecord for the tenant.
        """
        r = _raise_for_status(await self._http.get(f"/usage/{tenant_id}"))
        return UsageRecord.model_validate(r.json())

    async def reset(self, tenant_id: str) -> UsageRecord:
        """Reset usage counters for a tenant (e.g. at billing cycle rollover).

        Args:
            tenant_id: Tenant identifier.

        Returns:
            The reset UsageRecord.
        """
        r = _raise_for_status(await self._http.post(f"/usage/{tenant_id}/reset"))
        return UsageRecord.model_validate(r.json())


class TracesAPI:
    """Distributed tracing, span ingestion, and token billing."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list(self, tenant_id: str, agent_id: str) -> List[TraceSummary]:
        """List trace summaries for a tenant/agent pair.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.

        Returns:
            List of TraceSummary objects.
        """
        r = _raise_for_status(await self._http.get(f"/traces/{tenant_id}/{agent_id}"))
        return [TraceSummary.model_validate(s) for s in r.json()]

    async def get(self, tenant_id: str, agent_id: str, trace_id: str) -> List[TraceSpan]:
        """Retrieve all spans for a specific trace.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            trace_id: Trace ID returned when the first span was posted.

        Returns:
            List of TraceSpan objects.
        """
        r = _raise_for_status(
            await self._http.get(f"/traces/{tenant_id}/{agent_id}/{trace_id}")
        )
        return [TraceSpan.model_validate(s) for s in r.json()]

    async def post_span(self, span: TraceSpan) -> Dict[str, str]:
        """Post a single execution span.

        Args:
            span: The TraceSpan to record. span_id and trace_id are auto-assigned
                  by the server if left empty.

        Returns:
            Dict with "span_id" and "trace_id".
        """
        r = _raise_for_status(
            await self._http.post(
                f"/traces/{span.tenant_id}/{span.agent_id}/spans",
                json=span.model_dump(exclude_none=True),
            )
        )
        return r.json()

    async def finalize(self, tenant_id: str, agent_id: str, trace_id: str) -> TraceSummary:
        """Finalize a trace — builds summary and aggregates token totals.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            trace_id: Trace to finalize.

        Returns:
            TraceSummary with final token and cost totals.
        """
        r = _raise_for_status(
            await self._http.post(f"/traces/{tenant_id}/{agent_id}/{trace_id}/finalize")
        )
        return TraceSummary.model_validate(r.json())

    async def token_stats(self, tenant_id: str) -> TenantTokenStats:
        """Get aggregated token usage for a tenant (for billing).

        Args:
            tenant_id: Tenant identifier.

        Returns:
            TenantTokenStats with cost breakdown by model.
        """
        r = _raise_for_status(await self._http.get(f"/traces/{tenant_id}/tokens"))
        return TenantTokenStats.model_validate(r.json())


class PolicyAPI:
    """Per-tenant governance policy management."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def get(self, tenant_id: str) -> TenantPolicy:
        """Get the current governance policy for a tenant.

        Args:
            tenant_id: Tenant identifier.

        Returns:
            TenantPolicy, defaulting to permissive if not set.
        """
        r = _raise_for_status(await self._http.get(f"/tenants/{tenant_id}/policy"))
        return TenantPolicy.model_validate(r.json())

    async def put(self, tenant_id: str, policy: TenantPolicy) -> Dict[str, str]:
        """Set or update the governance policy for a tenant.

        Args:
            tenant_id: Tenant identifier.
            policy: The new TenantPolicy.

        Returns:
            Status dict e.g. {"status": "saved"}.
        """
        payload = policy.model_dump(exclude_none=True)
        r = _raise_for_status(
            await self._http.put(f"/tenants/{tenant_id}/policy", json=payload)
        )
        return r.json()

    async def delete(self, tenant_id: str) -> Dict[str, str]:
        """Delete a tenant's policy (resets to permissive default).

        Args:
            tenant_id: Tenant identifier.

        Returns:
            Status dict e.g. {"status": "deleted"}.
        """
        r = _raise_for_status(await self._http.delete(f"/tenants/{tenant_id}/policy"))
        return r.json()

    async def compliance(self, tenant_id: str) -> ComplianceReport:
        """Retrieve the compliance report for a tenant.

        Args:
            tenant_id: Tenant identifier.

        Returns:
            ComplianceReport with score and violation details.
        """
        r = _raise_for_status(await self._http.get(f"/tenants/{tenant_id}/compliance"))
        return ComplianceReport.model_validate(r.json())

    async def list_tenants(self) -> List[str]:
        """List all tenant IDs that have a stored policy.

        Returns:
            List of tenant ID strings.
        """
        r = _raise_for_status(await self._http.get("/tenants/policies"))
        return r.json().get("tenants", [])


class HealthAPI:
    """Health intelligence — scoring, crash patterns, resource trends."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def agent(self, tenant_id: str, agent_id: str) -> HealthRecord:
        """Get health record for a specific agent.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.

        Returns:
            HealthRecord with score, crash history, and resource trend.
        """
        r = _raise_for_status(await self._http.get(f"/health/{tenant_id}/{agent_id}"))
        return HealthRecord.model_validate(r.json())

    async def tenant(self, tenant_id: str) -> List[HealthRecord]:
        """Get health records for all agents under a tenant.

        Args:
            tenant_id: Tenant identifier.

        Returns:
            List of HealthRecord objects.
        """
        r = _raise_for_status(await self._http.get(f"/health/{tenant_id}"))
        return [HealthRecord.model_validate(h) for h in r.json()]

    async def fleet(self) -> FleetHealthSummary:
        """Get fleet-wide health summary.

        Returns:
            FleetHealthSummary with healthy/degraded/critical counts.
        """
        r = _raise_for_status(await self._http.get("/health/fleet/summary"))
        return FleetHealthSummary.model_validate(r.json())


class MemoryAPI:
    """Per-agent persistent key-value store with TF-IDF similarity search."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def get(self, tenant_id: str, agent_id: str, key: str) -> MemoryEntry:
        """Retrieve a memory entry by key.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            key: Memory key.

        Returns:
            MemoryEntry.

        Raises:
            ApolloError: 404 if the key does not exist.
        """
        r = _raise_for_status(
            await self._http.get(f"/memory/{tenant_id}/{agent_id}/{key}")
        )
        return MemoryEntry.model_validate(r.json())

    async def put(
        self,
        tenant_id: str,
        agent_id: str,
        key: str,
        value: Any,
        *,
        tags: Optional[List[str]] = None,
        text: Optional[str] = None,
        ttl_secs: Optional[int] = None,
    ) -> MemoryEntry:
        """Store a memory entry.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            key: Memory key (used for retrieval and similarity indexing).
            value: JSON-serialisable value.
            tags: Optional list of tags for filtering.
            text: Optional plain-text representation used for TF-IDF indexing.
            ttl_secs: Optional time-to-live in seconds.

        Returns:
            The stored MemoryEntry.
        """
        body: Dict[str, Any] = {"value": value}
        if tags is not None:
            body["tags"] = tags
        if text is not None:
            body["text"] = text
        if ttl_secs is not None:
            body["ttl_secs"] = ttl_secs
        r = _raise_for_status(
            await self._http.put(f"/memory/{tenant_id}/{agent_id}/{key}", json=body)
        )
        return MemoryEntry.model_validate(r.json())

    async def delete(self, tenant_id: str, agent_id: str, key: str) -> Dict[str, str]:
        """Delete a memory entry.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            key: Memory key.

        Returns:
            Status dict e.g. {"status": "deleted"}.
        """
        r = _raise_for_status(
            await self._http.delete(f"/memory/{tenant_id}/{agent_id}/{key}")
        )
        return r.json()

    async def list(self, tenant_id: str, agent_id: str) -> List[str]:
        """List all stored memory keys.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.

        Returns:
            List of key strings.
        """
        r = _raise_for_status(await self._http.get(f"/memory/{tenant_id}/{agent_id}"))
        return r.json().get("keys", [])

    async def search(
        self,
        tenant_id: str,
        agent_id: str,
        query: str,
        *,
        tags: Optional[List[str]] = None,
        limit: int = 10,
    ) -> List[MemoryEntry]:
        """Perform a TF-IDF similarity search over the memory store.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            query: Natural-language search string.
            tags: Optional list of tags to restrict the search.
            limit: Maximum number of results to return.

        Returns:
            List of matching MemoryEntry objects in relevance order.
        """
        body: Dict[str, Any] = {"query": query, "limit": limit}
        if tags is not None:
            body["tags"] = tags
        r = _raise_for_status(
            await self._http.post(f"/memory/{tenant_id}/{agent_id}/search", json=body)
        )
        return [MemoryEntry.model_validate(e) for e in r.json()]

    async def clear(self, tenant_id: str, agent_id: str) -> Dict[str, str]:
        """Delete all memory entries for an agent.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.

        Returns:
            Status dict e.g. {"status": "cleared"}.
        """
        r = _raise_for_status(
            await self._http.delete(f"/memory/{tenant_id}/{agent_id}")
        )
        return r.json()

    async def stats(self, tenant_id: str, agent_id: str) -> MemoryStats:
        """Get memory store statistics (entry count, size).

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.

        Returns:
            MemoryStats.
        """
        r = _raise_for_status(
            await self._http.get(f"/memory/{tenant_id}/{agent_id}/stats")
        )
        return MemoryStats.model_validate(r.json())


class ModelsAPI:
    """LLM model registry and cost/latency-aware routing."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list(self) -> List[ModelRecord]:
        """List all registered LLM models.

        Returns:
            List of ModelRecord objects.
        """
        r = _raise_for_status(await self._http.get("/models"))
        return [ModelRecord.model_validate(m) for m in r.json()]

    async def put(self, model_id: str, model: ModelRecord) -> ModelRecord:
        """Register or update an LLM model.

        Args:
            model_id: Unique model identifier (e.g. "claude-sonnet-4-6").
            model: ModelRecord with routing metadata.

        Returns:
            The stored ModelRecord.
        """
        payload = model.model_dump(exclude_none=True)
        r = _raise_for_status(
            await self._http.put(f"/models/{model_id}", json=payload)
        )
        return ModelRecord.model_validate(r.json())

    async def delete(self, model_id: str) -> Dict[str, str]:
        """Remove a model from the registry.

        Args:
            model_id: Model identifier.

        Returns:
            Status dict e.g. {"status": "deleted"}.
        """
        r = _raise_for_status(await self._http.delete(f"/models/{model_id}"))
        return r.json()

    async def route(self, request: RoutingRequest) -> RoutingDecision:
        """Get a cost/latency/policy-aware model routing recommendation.

        Args:
            request: RoutingRequest with token estimates and constraints.

        Returns:
            RoutingDecision with selected model and reasoning.
        """
        r = _raise_for_status(
            await self._http.post("/models/route", json=request.model_dump(exclude_none=True))
        )
        return RoutingDecision.model_validate(r.json())

    async def feedback(self, feedback: ModelFeedback) -> Dict[str, str]:
        """Report observed latency feedback for a model.

        Args:
            feedback: ModelFeedback with model_id, latency_ms, is_success.

        Returns:
            Status dict e.g. {"status": "recorded"}.
        """
        r = _raise_for_status(
            await self._http.post("/models/feedback", json=feedback.model_dump())
        )
        return r.json()

    async def usage(self, tenant_id: str) -> Dict[str, Any]:
        """Get per-tenant model usage and cost breakdown.

        Args:
            tenant_id: Tenant identifier.

        Returns:
            Raw JSON dict with model usage stats.
        """
        r = _raise_for_status(await self._http.get(f"/models/usage/{tenant_id}"))
        return r.json()

    async def record_usage(self, tenant_id: str, usage: ModelUsageRecord) -> Dict[str, str]:
        """Record model usage for a tenant (called by agents or harnesses).

        Args:
            tenant_id: Tenant identifier.
            usage: ModelUsageRecord with token counts and cost.

        Returns:
            Status dict e.g. {"status": "recorded"}.
        """
        r = _raise_for_status(
            await self._http.post(
                f"/models/usage/{tenant_id}/record", json=usage.model_dump()
            )
        )
        return r.json()


class ScheduleAPI:
    """Cron, interval, and one-shot job scheduling."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list(self) -> List[ScheduledJob]:
        """List all scheduled jobs.

        Returns:
            List of ScheduledJob objects.
        """
        r = _raise_for_status(await self._http.get("/schedule"))
        return [ScheduledJob.model_validate(j) for j in r.json()]

    async def create(self, job: ScheduledJob) -> ScheduledJob:
        """Create a new scheduled job.

        Args:
            job: ScheduledJob definition. id is auto-assigned if empty.

        Returns:
            The created ScheduledJob with server-assigned id.
        """
        r = _raise_for_status(
            await self._http.post("/schedule", json=job.model_dump(exclude_none=True))
        )
        return ScheduledJob.model_validate(r.json())

    async def get(self, job_id: str) -> ScheduledJob:
        """Get a scheduled job by ID.

        Args:
            job_id: Job identifier.

        Returns:
            ScheduledJob.

        Raises:
            ApolloError: 404 if not found.
        """
        r = _raise_for_status(await self._http.get(f"/schedule/{job_id}"))
        return ScheduledJob.model_validate(r.json())

    async def update(self, job_id: str, job: ScheduledJob) -> ScheduledJob:
        """Update a scheduled job.

        Args:
            job_id: Job identifier.
            job: Updated ScheduledJob definition.

        Returns:
            The updated ScheduledJob.
        """
        r = _raise_for_status(
            await self._http.put(
                f"/schedule/{job_id}", json=job.model_dump(exclude_none=True)
            )
        )
        return ScheduledJob.model_validate(r.json())

    async def delete(self, job_id: str) -> Dict[str, str]:
        """Delete a scheduled job.

        Args:
            job_id: Job identifier.

        Returns:
            Status dict e.g. {"status": "deleted"}.
        """
        r = _raise_for_status(await self._http.delete(f"/schedule/{job_id}"))
        return r.json()

    async def run(self, job_id: str) -> AgentInstance:
        """Manually trigger a scheduled job immediately.

        Args:
            job_id: Job identifier.

        Returns:
            The spawned AgentInstance.
        """
        r = _raise_for_status(await self._http.post(f"/schedule/{job_id}/run"))
        return AgentInstance.model_validate(r.json())

    async def history(self, job_id: str) -> List[JobRunRecord]:
        """Get the run history for a scheduled job (last 100 runs).

        Args:
            job_id: Job identifier.

        Returns:
            List of JobRunRecord objects, most recent first.
        """
        r = _raise_for_status(await self._http.get(f"/schedule/{job_id}/history"))
        return [JobRunRecord.model_validate(rec) for rec in r.json()]


class BlueprintsAPI:
    """Agent deployment blueprints."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list(self) -> List[Blueprint]:
        """List all blueprints.

        Returns:
            List of Blueprint objects.
        """
        r = _raise_for_status(await self._http.get("/blueprints"))
        return [Blueprint.model_validate(b) for b in r.json()]

    async def create(self, blueprint: Blueprint) -> Blueprint:
        """Create a new blueprint.

        Args:
            blueprint: Blueprint definition. id is auto-assigned if empty.

        Returns:
            The created Blueprint.
        """
        r = _raise_for_status(
            await self._http.post(
                "/blueprints", json=blueprint.model_dump(exclude_none=True)
            )
        )
        return Blueprint.model_validate(r.json())

    async def get(self, blueprint_id: str) -> Blueprint:
        """Get a blueprint by ID.

        Args:
            blueprint_id: Blueprint identifier.

        Returns:
            Blueprint.

        Raises:
            ApolloError: 404 if not found.
        """
        r = _raise_for_status(await self._http.get(f"/blueprints/{blueprint_id}"))
        return Blueprint.model_validate(r.json())

    async def update(self, blueprint_id: str, blueprint: Blueprint) -> Blueprint:
        """Update a blueprint.

        Args:
            blueprint_id: Blueprint identifier.
            blueprint: Updated Blueprint definition.

        Returns:
            The updated Blueprint.
        """
        r = _raise_for_status(
            await self._http.put(
                f"/blueprints/{blueprint_id}",
                json=blueprint.model_dump(exclude_none=True),
            )
        )
        return Blueprint.model_validate(r.json())

    async def delete(self, blueprint_id: str) -> Dict[str, str]:
        """Delete a blueprint.

        Args:
            blueprint_id: Blueprint identifier.

        Returns:
            Status dict.
        """
        r = _raise_for_status(await self._http.delete(f"/blueprints/{blueprint_id}"))
        return r.json()

    async def deploy(self, blueprint_id: str, tenant_id: str) -> Dict[str, Any]:
        """Deploy an agent from a blueprint for a tenant.

        Args:
            blueprint_id: Blueprint identifier.
            tenant_id: Tenant / user ID.

        Returns:
            Dict with "status", "blueprint_id", and "instance".
        """
        r = _raise_for_status(
            await self._http.post(
                f"/blueprints/{blueprint_id}/deploy", json={"tenant_id": tenant_id}
            )
        )
        return r.json()


class GroupsAPI:
    """Agent groups — bulk lifecycle management."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list(self) -> List[AgentGroup]:
        """List all agent groups.

        Returns:
            List of AgentGroup objects.
        """
        r = _raise_for_status(await self._http.get("/groups"))
        return [AgentGroup.model_validate(g) for g in r.json()]

    async def create(self, group: AgentGroup) -> AgentGroup:
        """Create a new agent group.

        Args:
            group: AgentGroup definition.

        Returns:
            The created AgentGroup.
        """
        r = _raise_for_status(
            await self._http.post("/groups", json=group.model_dump(exclude_none=True))
        )
        return AgentGroup.model_validate(r.json())

    async def get(self, group_id: str) -> AgentGroup:
        """Get an agent group by ID.

        Args:
            group_id: Group identifier.

        Returns:
            AgentGroup.

        Raises:
            ApolloError: 404 if not found.
        """
        r = _raise_for_status(await self._http.get(f"/groups/{group_id}"))
        return AgentGroup.model_validate(r.json())

    async def update(self, group_id: str, group: AgentGroup) -> AgentGroup:
        """Update a group definition.

        Args:
            group_id: Group identifier.
            group: Updated AgentGroup.

        Returns:
            The updated AgentGroup.
        """
        r = _raise_for_status(
            await self._http.put(
                f"/groups/{group_id}", json=group.model_dump(exclude_none=True)
            )
        )
        return AgentGroup.model_validate(r.json())

    async def delete(self, group_id: str) -> Dict[str, str]:
        """Delete an agent group.

        Args:
            group_id: Group identifier.

        Returns:
            Status dict.
        """
        r = _raise_for_status(await self._http.delete(f"/groups/{group_id}"))
        return r.json()

    async def run(self, group_id: str) -> Dict[str, Any]:
        """Start all agents in a group.

        Args:
            group_id: Group identifier.

        Returns:
            Dict with "group_id", "started", and "errors".
        """
        r = _raise_for_status(await self._http.post(f"/groups/{group_id}/run"))
        return r.json()

    async def stop(self, group_id: str) -> Dict[str, Any]:
        """Stop all agents in a group.

        Args:
            group_id: Group identifier.

        Returns:
            Dict with "group_id" and "stopped".
        """
        r = _raise_for_status(await self._http.post(f"/groups/{group_id}/stop"))
        return r.json()


class WorkflowsAPI:
    """Workflow DAG definitions and run management."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list(self) -> List[WorkflowDef]:
        """List all workflow definitions.

        Returns:
            List of WorkflowDef objects.
        """
        r = _raise_for_status(await self._http.get("/workflows"))
        return [WorkflowDef.model_validate(d) for d in r.json()]

    async def create(self, workflow: WorkflowDef) -> WorkflowDef:
        """Create a new workflow DAG.

        Args:
            workflow: WorkflowDef with steps and dependency graph.

        Returns:
            The created WorkflowDef with server-assigned id.
        """
        r = _raise_for_status(
            await self._http.post(
                "/workflows", json=workflow.model_dump(exclude_none=True)
            )
        )
        return WorkflowDef.model_validate(r.json())

    async def get(self, workflow_id: str) -> WorkflowDef:
        """Get a workflow definition.

        Args:
            workflow_id: Workflow identifier.

        Returns:
            WorkflowDef.

        Raises:
            ApolloError: 404 if not found.
        """
        r = _raise_for_status(await self._http.get(f"/workflows/{workflow_id}"))
        return WorkflowDef.model_validate(r.json())

    async def delete(self, workflow_id: str) -> Dict[str, str]:
        """Delete a workflow definition.

        Args:
            workflow_id: Workflow identifier.

        Returns:
            Status dict.
        """
        r = _raise_for_status(await self._http.delete(f"/workflows/{workflow_id}"))
        return r.json()

    async def run(self, workflow_id: str) -> WorkflowRun:
        """Execute a workflow, starting from the first ready steps.

        Args:
            workflow_id: Workflow identifier.

        Returns:
            WorkflowRun with initial step states.
        """
        r = _raise_for_status(await self._http.post(f"/workflows/{workflow_id}/run"))
        return WorkflowRun.model_validate(r.json())

    async def runs_list(self, workflow_id: str) -> List[WorkflowRun]:
        """List all runs for a workflow.

        Args:
            workflow_id: Workflow identifier.

        Returns:
            List of WorkflowRun objects.
        """
        r = _raise_for_status(await self._http.get(f"/workflows/{workflow_id}/runs"))
        return [WorkflowRun.model_validate(run) for run in r.json()]

    async def run_get(self, run_id: str) -> WorkflowRun:
        """Get the current state of a workflow run.

        Args:
            run_id: Run identifier.

        Returns:
            WorkflowRun with live step states.
        """
        r = _raise_for_status(await self._http.get(f"/workflows/runs/{run_id}"))
        return WorkflowRun.model_validate(r.json())


class ArchitectureAPI:
    """Automatic architecture selection (Deterministic / SingleAgent / MultiAgent)."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def select(self, workflow: Dict[str, Any]) -> ArchitectureDecision:
        """Analyse a WorkflowDef and select the optimal execution architecture.

        Args:
            workflow: Dict representation of a WorkflowDef (use model_dump()).

        Returns:
            ArchitectureDecision with architecture, confidence, scores, and reasoning.
        """
        r = _raise_for_status(
            await self._http.post("/architecture/select", json=workflow)
        )
        return ArchitectureDecision.model_validate(r.json())

    async def select_saved(self, workflow_id: str) -> ArchitectureDecision:
        """Analyse a previously saved workflow by ID.

        Args:
            workflow_id: Workflow identifier stored in the node.

        Returns:
            ArchitectureDecision.
        """
        r = _raise_for_status(
            await self._http.get(f"/architecture/select/{workflow_id}")
        )
        return ArchitectureDecision.model_validate(r.json())

    async def classify(self, request: QuickClassifyRequest) -> ArchitectureDecision:
        """Quick heuristic classification without a full WorkflowDef.

        Args:
            request: QuickClassifyRequest with tool/branch/governance metrics.

        Returns:
            ArchitectureDecision.
        """
        r = _raise_for_status(
            await self._http.post(
                "/architecture/classify", json=request.model_dump()
            )
        )
        return ArchitectureDecision.model_validate(r.json())


class AlertsAPI:
    """Alert rules and event history (v2.2 extension)."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def list_rules(self, tenant_id: str) -> List[AlertRule]:
        """List all alert rules for a tenant.

        Args:
            tenant_id: Tenant identifier.

        Returns:
            List of AlertRule objects.
        """
        r = _raise_for_status(await self._http.get(f"/alerts/{tenant_id}/rules"))
        return [AlertRule.model_validate(rule) for rule in r.json()]

    async def create_rule(self, tenant_id: str, rule: AlertRule) -> AlertRule:
        """Create an alert rule for a tenant.

        Args:
            tenant_id: Tenant identifier.
            rule: AlertRule definition.

        Returns:
            The created AlertRule with server-assigned id.
        """
        r = _raise_for_status(
            await self._http.post(
                f"/alerts/{tenant_id}/rules", json=rule.model_dump(exclude_none=True)
            )
        )
        return AlertRule.model_validate(r.json())

    async def delete_rule(self, tenant_id: str, rule_id: str) -> Dict[str, str]:
        """Delete an alert rule.

        Args:
            tenant_id: Tenant identifier.
            rule_id: Rule identifier.

        Returns:
            Status dict.
        """
        r = _raise_for_status(
            await self._http.delete(f"/alerts/{tenant_id}/rules/{rule_id}")
        )
        return r.json()

    async def get_history(self, tenant_id: str, rule_id: str) -> List[AlertEvent]:
        """Get alert event history for a rule.

        Args:
            tenant_id: Tenant identifier.
            rule_id: Rule identifier.

        Returns:
            List of AlertEvent objects.
        """
        r = _raise_for_status(
            await self._http.get(f"/alerts/{tenant_id}/rules/{rule_id}/history")
        )
        return [AlertEvent.model_validate(e) for e in r.json()]


class MessagesAPI:
    """Per-agent message bus for event-driven coordination (v2.2 extension)."""

    def __init__(self, http: httpx.AsyncClient) -> None:
        self._http = http

    async def publish(
        self,
        tenant_id: str,
        agent_id: str,
        channel: str,
        payload: Any,
        *,
        ttl_secs: Optional[int] = None,
    ) -> BusMessage:
        """Publish a message to an agent's channel.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            channel: Named message channel.
            payload: JSON-serialisable message payload.
            ttl_secs: Optional message TTL.

        Returns:
            The published BusMessage with server-assigned id.
        """
        body: Dict[str, Any] = {"channel": channel, "payload": payload}
        if ttl_secs is not None:
            body["ttl_secs"] = ttl_secs
        r = _raise_for_status(
            await self._http.post(f"/messages/{tenant_id}/{agent_id}", json=body)
        )
        return BusMessage.model_validate(r.json())

    async def poll(
        self,
        tenant_id: str,
        agent_id: str,
        channel: str,
        *,
        limit: int = 10,
    ) -> List[BusMessage]:
        """Poll messages from a channel (dequeues messages).

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            channel: Named message channel.
            limit: Maximum messages to return.

        Returns:
            List of BusMessage objects.
        """
        r = _raise_for_status(
            await self._http.get(
                f"/messages/{tenant_id}/{agent_id}/{channel}",
                params={"limit": limit},
            )
        )
        return [BusMessage.model_validate(m) for m in r.json()]

    async def clear(self, tenant_id: str, agent_id: str, channel: str) -> Dict[str, str]:
        """Clear all messages from a channel.

        Args:
            tenant_id: Tenant identifier.
            agent_id: Agent name.
            channel: Named message channel.

        Returns:
            Status dict e.g. {"status": "cleared"}.
        """
        r = _raise_for_status(
            await self._http.delete(f"/messages/{tenant_id}/{agent_id}/{channel}")
        )
        return r.json()


# ---------------------------------------------------------------------------
# Main async client
# ---------------------------------------------------------------------------

class ApolloClient:
    """Async Apollo AI Agent Platform client.

    Supports both context-manager and explicit lifecycle patterns.

    Args:
        base_url: Node base URL, e.g. "http://localhost:8080".
        key: API key (X-Apollo-Key header). Mutually exclusive with ``jwt``.
        jwt: JWT Bearer token. Mutually exclusive with ``key``.
        timeout: Default request timeout in seconds (default 30).
        verify_ssl: Whether to verify TLS certificates (default True).

    Example:
        async with ApolloClient("http://localhost:8080", key="my-secret") as client:
            records = await client.agents.list()
            instance = await client.agents.run("openclaw", "alice")
    """

    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        *,
        key: Optional[str] = None,
        jwt: Optional[str] = None,
        timeout: float = 30.0,
        verify_ssl: bool = True,
    ) -> None:
        if not key and not jwt:
            raise ValueError("Either 'key' or 'jwt' must be provided.")

        headers: Dict[str, str] = {"Content-Type": "application/json"}
        if key:
            headers["X-Apollo-Key"] = key
        elif jwt:
            headers["Authorization"] = f"Bearer {jwt}"

        self._http = httpx.AsyncClient(
            base_url=base_url.rstrip("/"),
            headers=headers,
            timeout=timeout,
            verify=verify_ssl,
        )

        # Resource APIs
        self.agents = AgentsAPI(self._http)
        self.secrets = SecretsAPI(self._http)
        self.usage = UsageAPI(self._http)
        self.traces = TracesAPI(self._http)
        self.policy = PolicyAPI(self._http)
        self.health = HealthAPI(self._http)
        self.memory = MemoryAPI(self._http)
        self.models = ModelsAPI(self._http)
        self.schedule = ScheduleAPI(self._http)
        self.blueprints = BlueprintsAPI(self._http)
        self.groups = GroupsAPI(self._http)
        self.workflows = WorkflowsAPI(self._http)
        self.architecture = ArchitectureAPI(self._http)
        self.alerts = AlertsAPI(self._http)
        self.messages = MessagesAPI(self._http)

    async def ping(self) -> Dict[str, Any]:
        """Check node health.

        Returns:
            Dict with "status" and "version".
        """
        r = _raise_for_status(await self._http.get("/health"))
        return r.json()

    async def metrics(self) -> Dict[str, Any]:
        """Get node capacity and identity metrics.

        Returns:
            Dict with active_agents, max_agents, node_id, region, version.
        """
        r = _raise_for_status(await self._http.get("/metrics"))
        return r.json()

    async def close(self) -> None:
        """Close the underlying HTTP connection pool."""
        await self._http.aclose()

    async def __aenter__(self) -> "ApolloClient":
        return self

    async def __aexit__(self, *_: Any) -> None:
        await self.close()


# ---------------------------------------------------------------------------
# Sync wrapper
# ---------------------------------------------------------------------------

def _run_sync(coro: Any) -> Any:
    """Run a coroutine synchronously, creating an event loop if needed."""
    try:
        loop = asyncio.get_event_loop()
        if loop.is_running():
            # Already inside an event loop (e.g. Jupyter); use a new thread pool
            import concurrent.futures
            with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
                future = executor.submit(asyncio.run, coro)
                return future.result()
        return loop.run_until_complete(coro)
    except RuntimeError:
        return asyncio.run(coro)


def _make_sync(method_name: str):
    """Decorator factory: wraps an async API method with a sync runner."""
    def wrapper(self, *args, **kwargs):
        coro = getattr(self._async_api, method_name)(*args, **kwargs)
        return _run_sync(coro)
    wrapper.__name__ = method_name
    return wrapper


class _SyncAPIProxy:
    """Thin synchronous proxy over any async API resource class."""

    def __init__(self, async_api: Any) -> None:
        self._async_api = async_api

    def __getattr__(self, name: str):
        async_method = getattr(self._async_api, name)
        if asyncio.iscoroutinefunction(async_method):
            @functools.wraps(async_method)
            def sync_method(*args, **kwargs):
                return _run_sync(async_method(*args, **kwargs))
            return sync_method
        return async_method


class SyncApolloClient:
    """Synchronous Apollo AI Agent Platform client.

    Wraps ApolloClient for use in non-async contexts.

    Args:
        base_url: Node base URL.
        key: API key.
        jwt: JWT Bearer token (alternative to key).
        timeout: Request timeout in seconds.
        verify_ssl: Whether to verify TLS certificates.

    Example:
        client = SyncApolloClient("http://localhost:8080", key="my-secret")
        try:
            records = client.agents.list()
            instance = client.agents.run("openclaw", "alice")
        finally:
            client.close()
    """

    def __init__(
        self,
        base_url: str = "http://localhost:8080",
        *,
        key: Optional[str] = None,
        jwt: Optional[str] = None,
        timeout: float = 30.0,
        verify_ssl: bool = True,
    ) -> None:
        self._async_client = ApolloClient(
            base_url, key=key, jwt=jwt, timeout=timeout, verify_ssl=verify_ssl
        )
        self.agents = _SyncAPIProxy(self._async_client.agents)
        self.secrets = _SyncAPIProxy(self._async_client.secrets)
        self.usage = _SyncAPIProxy(self._async_client.usage)
        self.traces = _SyncAPIProxy(self._async_client.traces)
        self.policy = _SyncAPIProxy(self._async_client.policy)
        self.health = _SyncAPIProxy(self._async_client.health)
        self.memory = _SyncAPIProxy(self._async_client.memory)
        self.models = _SyncAPIProxy(self._async_client.models)
        self.schedule = _SyncAPIProxy(self._async_client.schedule)
        self.blueprints = _SyncAPIProxy(self._async_client.blueprints)
        self.groups = _SyncAPIProxy(self._async_client.groups)
        self.workflows = _SyncAPIProxy(self._async_client.workflows)
        self.architecture = _SyncAPIProxy(self._async_client.architecture)
        self.alerts = _SyncAPIProxy(self._async_client.alerts)
        self.messages = _SyncAPIProxy(self._async_client.messages)

    def ping(self) -> Dict[str, Any]:
        """Check node health."""
        return _run_sync(self._async_client.ping())

    def metrics(self) -> Dict[str, Any]:
        """Get node capacity and identity metrics."""
        return _run_sync(self._async_client.metrics())

    def close(self) -> None:
        """Close the underlying HTTP connection pool."""
        _run_sync(self._async_client.close())

    def __enter__(self) -> "SyncApolloClient":
        return self

    def __exit__(self, *_: Any) -> None:
        self.close()
