"""
Apollo SDK — End-to-end quickstart example.

Demonstrates:
  1. Connecting to an Apollo node
  2. Registering an agent
  3. Setting tenant secrets
  4. Running the agent for a tenant
  5. Checking health
  6. Posting a trace span
  7. Retrieving token usage
  8. Storing and searching memory
  9. Setting policy
  10. Stopping the agent

Prerequisites:
  - Apollo node running: apollo node start --secret-keys "demo-key"
  - An agent package at ./examples/openclaw (or adjust AGENT_SOURCE below)

Usage:
  pip install apollo-sdk
  python quickstart.py
"""

import asyncio
import time
import os

from apollo_sdk import ApolloClient, TraceSpan, TokenUsage, TenantPolicy


APOLLO_URL = os.environ.get("APOLLO_NODE", "http://localhost:8080")
APOLLO_KEY = os.environ.get("APOLLO_KEY", "demo-key")
AGENT_SOURCE = os.environ.get("AGENT_SOURCE", "./examples/openclaw")
TENANT_ID = "demo-user"
AGENT_NAME = "openclaw"


async def main() -> None:
    print("Apollo SDK Quickstart")
    print("=" * 40)

    async with ApolloClient(APOLLO_URL, key=APOLLO_KEY) as client:

        # 1. Check node health
        print("\n[1] Checking node health...")
        health = await client.ping()
        print(f"    Node status: {health.get('status')} (version {health.get('version')})")

        # 2. Check current agents
        print("\n[2] Listing registered agents...")
        agents = await client.agents.list()
        print(f"    Found {len(agents)} registered agent(s).")

        # 3. Register the agent if not already present
        if not any(a.id == AGENT_NAME for a in agents):
            print(f"\n[3] Registering agent from '{AGENT_SOURCE}'...")
            try:
                record = await client.agents.add(AGENT_SOURCE)
                print(f"    Registered: {record.id} v{record.spec.version}")
            except Exception as exc:
                print(f"    Skipping registration (agent source not available): {exc}")
        else:
            print(f"\n[3] Agent '{AGENT_NAME}' already registered.")

        # 4. Set per-tenant secrets
        print(f"\n[4] Storing secrets for tenant '{TENANT_ID}'...")
        await client.secrets.put(TENANT_ID, {
            "OPENAI_API_KEY": "sk-demo-placeholder",
            "CUSTOM_TOKEN": "token-xyz",
        })
        print("    Secrets stored.")

        # 5. Set governance policy
        print(f"\n[5] Setting governance policy for '{TENANT_ID}'...")
        policy = TenantPolicy(
            max_instances=3,
            allowed_agents=[AGENT_NAME],
            blocked_tools=["bash"],
            max_tokens_per_day=500_000,
            require_audit=True,
        )
        await client.policy.put(TENANT_ID, policy)
        print("    Policy applied.")

        # 6. Run the agent
        print(f"\n[6] Starting agent '{AGENT_NAME}' for tenant '{TENANT_ID}'...")
        try:
            instance = await client.agents.run(AGENT_NAME, TENANT_ID)
            print(f"    Instance ID: {instance.id}  PID: {instance.pid}  Port: {instance.port}")
        except Exception as exc:
            print(f"    Could not start agent (expected in demo without real agent): {exc}")
            instance = None

        # 7. Post a trace span
        print("\n[7] Recording a trace span...")
        now_ms = int(time.time() * 1000)
        span = TraceSpan(
            tenant_id=TENANT_ID,
            agent_id=AGENT_NAME,
            name="llm_inference",
            status="ok",
            start_ts_ms=now_ms - 1500,
            end_ts_ms=now_ms,
            token_usage=TokenUsage(
                model="claude-sonnet-4-6",
                input_tokens=500,
                output_tokens=200,
                cost_usd=0.002,
                provider="anthropic",
            ),
        )
        try:
            ids = await client.traces.post_span(span)
            trace_id = ids["trace_id"]
            print(f"    Span recorded. trace_id={trace_id}")

            # Finalize the trace
            summary = await client.traces.finalize(TENANT_ID, AGENT_NAME, trace_id)
            print(f"    Trace finalized. Total cost: ${summary.total_cost_usd:.4f}")
        except Exception as exc:
            print(f"    Trace recording skipped: {exc}")

        # 8. Store memory
        print("\n[8] Storing agent memory...")
        await client.memory.put(
            TENANT_ID,
            AGENT_NAME,
            "user_preferences",
            {"theme": "dark", "language": "en"},
            tags=["profile"],
            text="user prefers dark mode and English",
        )
        print("    Memory stored.")

        # 9. Search memory
        print("\n[9] Searching memory...")
        results = await client.memory.search(
            TENANT_ID, AGENT_NAME, "user theme preference", limit=3
        )
        print(f"    Found {len(results)} matching memory entries.")

        # 10. Check health
        print("\n[10] Checking agent health...")
        try:
            health_record = await client.health.agent(TENANT_ID, AGENT_NAME)
            print(f"     Health score: {health_record.score:.1f}/100  "
                  f"Crashes: {health_record.crash_count}")
        except Exception as exc:
            print(f"     No health record yet: {exc}")

        # 11. Fleet health summary
        print("\n[11] Fleet health summary...")
        try:
            fleet = await client.health.fleet()
            print(f"     Healthy: {fleet.healthy_count}  "
                  f"Degraded: {fleet.degraded_count}  "
                  f"Critical: {fleet.critical_count}")
        except Exception as exc:
            print(f"     Fleet summary unavailable: {exc}")

        # 12. Token stats (for billing)
        print(f"\n[12] Token usage for tenant '{TENANT_ID}'...")
        try:
            token_stats = await client.traces.token_stats(TENANT_ID)
            print(f"     Total input tokens:  {token_stats.total_input_tokens:,}")
            print(f"     Total output tokens: {token_stats.total_output_tokens:,}")
            print(f"     Total cost:          ${token_stats.total_cost_usd:.4f}")
        except Exception as exc:
            print(f"     Token stats unavailable: {exc}")

        # 13. Stop the agent
        if instance is not None:
            print(f"\n[13] Stopping agent '{AGENT_NAME}'...")
            try:
                result = await client.agents.stop(AGENT_NAME, TENANT_ID)
                print(f"     {result}")
            except Exception as exc:
                print(f"     Stop skipped: {exc}")

        print("\n" + "=" * 40)
        print("Quickstart complete.")


if __name__ == "__main__":
    asyncio.run(main())
