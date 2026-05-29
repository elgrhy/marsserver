/**
 * Apollo SDK for Node.js — End-to-end quickstart example.
 *
 * Demonstrates:
 *   1. Connecting to an Apollo node
 *   2. Registering an agent
 *   3. Setting tenant secrets
 *   4. Setting governance policy
 *   5. Running the agent for a tenant
 *   6. Posting a trace span
 *   7. Storing and searching memory
 *   8. Checking health
 *   9. Creating a scheduled job
 *  10. Creating a workflow
 *  11. Architecture selection
 *  12. Stopping the agent
 *
 * Prerequisites:
 *   - Apollo node running: apollo node start --secret-keys "demo-key"
 *
 * Usage:
 *   npm run build
 *   node dist/examples/quickstart.js
 *
 *   Or with ts-node:
 *   npx ts-node examples/quickstart.ts
 */

import {
  ApolloClient,
  ApolloError,
  TraceSpan,
  TenantPolicy,
  ScheduledJob,
  WorkflowDef,
  QuickClassifyRequest,
} from '../src/index';

const APOLLO_URL = process.env['APOLLO_NODE'] ?? 'http://localhost:8080';
const APOLLO_KEY = process.env['APOLLO_KEY'] ?? 'demo-key';
const AGENT_SOURCE = process.env['AGENT_SOURCE'] ?? './examples/openclaw';
const TENANT_ID = 'demo-user';
const AGENT_NAME = 'openclaw';

async function main(): Promise<void> {
  console.log('Apollo SDK Quickstart (TypeScript)');
  console.log('='.repeat(40));

  const client = new ApolloClient(APOLLO_URL, { key: APOLLO_KEY });

  try {
    // 1. Check node health
    console.log('\n[1] Checking node health...');
    const health = await client.ping();
    console.log(`    Status: ${health.status} (version ${health.version})`);

    // 2. List registered agents
    console.log('\n[2] Listing registered agents...');
    const agents = await client.agents.list();
    console.log(`    Found ${agents.length} registered agent(s).`);

    // 3. Register agent if not present
    const alreadyRegistered = agents.some((a) => a.id === AGENT_NAME);
    if (!alreadyRegistered) {
      console.log(`\n[3] Registering agent from '${AGENT_SOURCE}'...`);
      try {
        const record = await client.agents.add(AGENT_SOURCE);
        console.log(`    Registered: ${record.id} v${record.spec.version}`);
      } catch (err) {
        console.log(`    Skipping registration (agent source not available): ${err}`);
      }
    } else {
      console.log(`\n[3] Agent '${AGENT_NAME}' already registered.`);
    }

    // 4. Set per-tenant secrets
    console.log(`\n[4] Storing secrets for tenant '${TENANT_ID}'...`);
    await client.secrets.put(TENANT_ID, {
      OPENAI_API_KEY: 'sk-demo-placeholder',
      CUSTOM_TOKEN: 'token-xyz',
    });
    console.log('    Secrets stored.');

    // 5. Set governance policy
    console.log(`\n[5] Setting governance policy for '${TENANT_ID}'...`);
    const policy: TenantPolicy = {
      max_instances: 3,
      allowed_agents: [AGENT_NAME],
      blocked_tools: ['bash'],
      max_tokens_per_day: 500_000,
      require_audit: true,
    };
    await client.policy.put(TENANT_ID, policy);
    console.log('    Policy applied.');

    // 6. Run the agent
    console.log(`\n[6] Starting agent '${AGENT_NAME}' for tenant '${TENANT_ID}'...`);
    let instanceId: string | undefined;
    try {
      const instance = await client.agents.run(AGENT_NAME, TENANT_ID);
      instanceId = instance.id;
      console.log(`    Instance: ${instance.id}  PID: ${instance.pid}  Port: ${instance.port}`);
    } catch (err) {
      console.log(`    Could not start agent (expected without real agent): ${err}`);
    }

    // 7. Post a trace span
    console.log('\n[7] Recording a trace span...');
    const nowMs = Date.now();
    const span: TraceSpan = {
      tenant_id: TENANT_ID,
      agent_id: AGENT_NAME,
      name: 'llm_inference',
      status: 'ok',
      start_ts_ms: nowMs - 1500,
      end_ts_ms: nowMs,
      token_usage: {
        model: 'claude-sonnet-4-6',
        input_tokens: 500,
        output_tokens: 200,
        cost_usd: 0.002,
        provider: 'anthropic',
      },
    };

    try {
      const ids = await client.traces.postSpan(span);
      console.log(`    Span recorded. trace_id=${ids.trace_id}`);

      const summary = await client.traces.finalize(TENANT_ID, AGENT_NAME, ids.trace_id);
      console.log(`    Trace finalized. Total cost: $${summary.total_cost_usd.toFixed(4)}`);

      const tokenStats = await client.traces.tokenStats(TENANT_ID);
      console.log(`    Token stats — Input: ${tokenStats.total_input_tokens.toLocaleString()}, ` +
                  `Output: ${tokenStats.total_output_tokens.toLocaleString()}, ` +
                  `Cost: $${tokenStats.total_cost_usd.toFixed(4)}`);
    } catch (err) {
      console.log(`    Trace recording skipped: ${err}`);
    }

    // 8. Store memory
    console.log('\n[8] Storing agent memory...');
    await client.memory.put(TENANT_ID, AGENT_NAME, 'user_preferences', {
      value: { theme: 'dark', language: 'en' },
      tags: ['profile'],
      text: 'user prefers dark mode and English',
    });
    console.log('    Memory stored.');

    // 9. Search memory
    console.log('\n[9] Searching memory...');
    const memResults = await client.memory.search(TENANT_ID, AGENT_NAME, {
      query: 'user theme preference',
      limit: 3,
    });
    console.log(`    Found ${memResults.length} matching memory entries.`);

    // 10. Check health
    console.log('\n[10] Checking fleet health...');
    try {
      const fleet = await client.health.fleet();
      console.log(`     Healthy: ${fleet.healthy_count}  ` +
                  `Degraded: ${fleet.degraded_count}  ` +
                  `Critical: ${fleet.critical_count}`);
    } catch (err) {
      console.log(`     Fleet health unavailable: ${err}`);
    }

    // 11. Create a scheduled job
    console.log('\n[11] Creating a scheduled job...');
    try {
      const job: ScheduledJob = {
        name: 'demo-heartbeat',
        tenant_id: TENANT_ID,
        agent_id: AGENT_NAME,
        schedule: { type: 'interval', secs: 300 },
        enabled: false, // disabled so it doesn't auto-fire in demo
      };
      const createdJob = await client.schedule.create(job);
      console.log(`     Job created: ${createdJob.id} (${createdJob.name})`);

      // Clean up
      await client.schedule.delete(createdJob.id!);
      console.log('     Job deleted.');
    } catch (err) {
      console.log(`     Scheduler unavailable: ${err}`);
    }

    // 12. Define a workflow
    console.log('\n[12] Creating a workflow DAG...');
    try {
      const workflow: WorkflowDef = {
        name: 'Demo ETL Pipeline',
        tenant_id: TENANT_ID,
        steps: [
          { step_id: 'extract', name: 'Extract', agent_id: 'extractor', depends_on: [] },
          {
            step_id: 'transform',
            name: 'Transform',
            agent_id: 'transformer',
            depends_on: ['extract'],
          },
          {
            step_id: 'load',
            name: 'Load',
            agent_id: 'loader',
            depends_on: ['transform'],
          },
        ],
      };
      const created = await client.workflows.create(workflow);
      console.log(`     Workflow created: ${created.id} (${created.name})`);

      // Clean up
      await client.workflows.delete(created.id!);
      console.log('     Workflow deleted.');
    } catch (err) {
      console.log(`     Workflow API unavailable: ${err}`);
    }

    // 13. Architecture classification
    console.log('\n[13] Architecture classification...');
    try {
      const classifyReq: QuickClassifyRequest = {
        tenant_id: TENANT_ID,
        tool_count: 4,
        parallel_branches: 2,
        error_tolerance: 1,
        governance_strict: false,
      };
      const decision = await client.architecture.classify(classifyReq);
      console.log(`     Architecture: ${decision.architecture}  ` +
                  `Confidence: ${(decision.confidence * 100).toFixed(1)}%`);
      console.log(`     Top reason: ${decision.reasoning[0] ?? 'n/a'}`);
    } catch (err) {
      console.log(`     Architecture API unavailable: ${err}`);
    }

    // 14. Stop the agent
    if (instanceId) {
      console.log(`\n[14] Stopping agent '${AGENT_NAME}'...`);
      try {
        const result = await client.agents.stop(AGENT_NAME, TENANT_ID);
        console.log(`     ${JSON.stringify(result)}`);
      } catch (err) {
        console.log(`     Stop skipped: ${err}`);
      }
    }

    console.log('\n' + '='.repeat(40));
    console.log('Quickstart complete.');
  } catch (error) {
    if (error instanceof ApolloError) {
      console.error(`ApolloError [${error.statusCode}]: ${error.detail}`);
    } else {
      console.error('Unexpected error:', error);
    }
    process.exit(1);
  } finally {
    await client.close();
  }
}

main();
