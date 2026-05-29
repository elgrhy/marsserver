"""
Apollo AI Agent Platform — Python SDK v2.2.0

Quick start (async):
    from apollo_sdk import ApolloClient

    async with ApolloClient("http://localhost:8080", key="your-key") as client:
        agents = await client.agents.list()
        instance = await client.agents.run("openclaw", tenant="alice")

Quick start (sync):
    from apollo_sdk import SyncApolloClient

    with SyncApolloClient("http://localhost:8080", key="your-key") as client:
        agents = client.agents.list()
        health = client.health.fleet()
"""

from .client import ApolloClient, SyncApolloClient, ApolloError
from .models import (
    AgentRecord,
    AgentSpec,
    AgentInstance,
    AlertCondition,
    AlertAction,
    AlertEvent,
    AlertRule,
    ArchitectureDecision,
    Blueprint,
    AgentGroup,
    GroupMember,
    BusMessage,
    ComplianceReport,
    ExecutionStats,
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
    RestartPolicy,
    RoutingDecision,
    RoutingRequest,
    ScheduledJob,
    TenantPolicy,
    TenantTokenStats,
    TokenUsage,
    TraceSummary,
    TraceSpan,
    UsageRecord,
    VolumeSpec,
    WorkflowDef,
    WorkflowRun,
    WorkflowStep,
)

__all__ = [
    # Clients
    "ApolloClient",
    "SyncApolloClient",
    "ApolloError",
    # Models
    "AgentRecord",
    "AgentSpec",
    "AgentInstance",
    "AlertCondition",
    "AlertAction",
    "AlertEvent",
    "AlertRule",
    "ArchitectureDecision",
    "Blueprint",
    "AgentGroup",
    "GroupMember",
    "BusMessage",
    "ComplianceReport",
    "ExecutionStats",
    "FleetHealthSummary",
    "HealthRecord",
    "JobRunRecord",
    "MemoryEntry",
    "MemoryQuery",
    "MemoryStats",
    "ModelFeedback",
    "ModelRecord",
    "ModelUsageRecord",
    "QuickClassifyRequest",
    "RestartPolicy",
    "RoutingDecision",
    "RoutingRequest",
    "ScheduledJob",
    "TenantPolicy",
    "TenantTokenStats",
    "TokenUsage",
    "TraceSummary",
    "TraceSpan",
    "UsageRecord",
    "VolumeSpec",
    "WorkflowDef",
    "WorkflowRun",
    "WorkflowStep",
]

__version__ = "2.2.0"
