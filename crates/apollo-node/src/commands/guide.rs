//! Built-in paginated documentation — navigable from the CLI.
//!
//! `apollo guide`               → interactive topic menu
//! `apollo guide <topic>`       → jump directly to a topic

use colored::Colorize;
use crate::fmt;

// ── Topic list ────────────────────────────────────────────────────────────────

const TOPICS: &[(&str, &str)] = &[
    ("quick-start",    "Installation & first run"),
    ("concepts",       "Architecture, tenants, agents"),
    ("observability",  "Distributed tracing, spans, token costs"),
    ("governance",     "Policy engine, data residency, quotas"),
    ("health",         "Health scoring, crash patterns, resource trends"),
    ("memory",         "Persistent KV store, TF-IDF similarity search"),
    ("routing",        "Model routing, cost budgets, EMA feedback"),
    ("scheduler",      "Cron, interval, one-shot job scheduling"),
    ("orchestration",  "Blueprints, agent groups, workflow DAGs"),
    ("arch-selection", "Architecture decision engine"),
    ("api",            "Full REST API quick reference"),
];

/// Dispatch: show topic by name, or interactive menu if None.
pub fn run(topic: Option<String>) {
    fmt::apollo_banner();
    println!("{}", "  Apollo v2.0 — Built-in Guide".cyan().bold());
    println!("{}", "  ─────────────────────────────".dimmed());
    println!();

    match topic.as_deref() {
        Some("quick-start")   | Some("quickstart")    => guide_quick_start(),
        Some("concepts")      | Some("arch")          => guide_concepts(),
        Some("observability") | Some("tracing")       => guide_observability(),
        Some("governance")    | Some("policy")        => guide_governance(),
        Some("health")                                 => guide_health(),
        Some("memory")                                 => guide_memory(),
        Some("routing")       | Some("models")        => guide_routing(),
        Some("scheduler")     | Some("schedule")      => guide_scheduler(),
        Some("orchestration")                          => guide_orchestration(),
        Some("arch-selection")                         => guide_arch_selection(),
        Some("api")                                    => guide_api(),
        Some(other) => {
            fmt::err(&format!("Unknown topic '{}'. Run 'apollo guide' for the topic list.", other));
        }
        None => interactive_menu(),
    }
}

// ── Interactive menu ──────────────────────────────────────────────────────────

fn interactive_menu() {
    println!("  {}\n", "Choose a topic to read:".bold());
    for (i, (slug, desc)) in TOPICS.iter().enumerate() {
        println!("  {}  {:<18} {}", format!("[{:>2}]", i + 1).cyan(), slug.bold(), desc.dimmed());
    }
    println!();
    println!("  {}", "Type the topic name or number, then Enter. 'q' to quit.".dimmed());
    println!();

    loop {
        use std::io::{self, BufRead, Write};
        print!("  {} ", "topic>".cyan());
        io::stdout().flush().ok();
        let mut line = String::new();
        if io::stdin().lock().read_line(&mut line).is_err() { break; }
        let input = line.trim().to_lowercase();
        if input == "q" || input == "quit" || input == "exit" { break; }

        // Number shortcut
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= TOPICS.len() {
                println!();
                dispatch_topic(TOPICS[n - 1].0);
                println!();
                continue;
            }
        }
        // Name match
        let found = TOPICS.iter().find(|(slug, _)| {
            slug.contains(&input.as_str()) || input.contains(slug)
        });
        if let Some((slug, _)) = found {
            println!();
            dispatch_topic(slug);
            println!();
        } else if !input.is_empty() {
            println!("  {}", format!("Unknown topic '{}'. Try a number 1-{}.", input, TOPICS.len()).yellow());
        }
    }
}

fn dispatch_topic(slug: &str) {
    match slug {
        "quick-start"   => guide_quick_start(),
        "concepts"      => guide_concepts(),
        "observability" => guide_observability(),
        "governance"    => guide_governance(),
        "health"        => guide_health(),
        "memory"        => guide_memory(),
        "routing"       => guide_routing(),
        "scheduler"     => guide_scheduler(),
        "orchestration" => guide_orchestration(),
        "arch-selection"=> guide_arch_selection(),
        "api"           => guide_api(),
        _               => {}
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn heading(text: &str) {
    let dashes = "─".repeat(60usize.saturating_sub(text.len() + 6));
    let line = format!("\n  ── {} {}", text, dashes);
    println!("{}", line.cyan().bold());
}

fn para(text: &str) {
    for line in text.trim().lines() {
        println!("  {}", line);
    }
}

fn code(cmd: &str) {
    println!("    {}", format!("$ {}", cmd).green());
}

fn bullet(text: &str) {
    println!("  •  {}", text);
}

fn kv(key: &str, val: &str) {
    println!("  {:<22} {}", format!("{}:", key).bold(), val);
}

// ═══════════════════════════════════════════════════════════════════════════════
// TOPIC CONTENT
// ═══════════════════════════════════════════════════════════════════════════════

fn guide_quick_start() {
    heading("QUICK START");
    para("Apollo is a self-hosted AI agent execution engine. Get a node running in
under two minutes.");

    heading("1 · Install");
    para("Build from source (requires Rust 1.75+):");
    code("git clone https://github.com/your-org/apollo && cd apollo");
    code("cargo build --release");
    code("export PATH=\"$PWD/target/release:$PATH\"");
    para("\nVerify:");
    code("apollo doctor");
    para("You should see 12 [OK] lines and STATUS: PRODUCTION READY.");

    heading("2 · Start the Node");
    code("apollo node start --secret-keys my-secret");
    para("The node starts an HTTP server on port 8080 with:\n\
          • Agent execution sandbox\n\
          • JWT / API-key authentication\n\
          • All v2.0 platform layers active");

    heading("3 · Register an Agent");
    code("apollo agent --base-dir .apollo add ./examples/openclaw");
    para("Apollo reads agent.yaml, detects runtime compatibility, and auto-installs
any missing runtime (Python, Node, Go, Deno, …).");

    heading("4 · Run Your First Agent");
    code("apollo agent --base-dir .apollo run openclaw --tenant alice");
    para("Each tenant gets an isolated workspace with injected secrets and
persistent volume mounts.");

    heading("5 · Monitor");
    code("apollo dashboard --key my-secret");
    para("Auto-refreshing terminal dashboard showing fleet health, live agents,
and scheduled jobs. Press 'q' to quit.");

    heading("Next");
    bullet("apollo guide concepts     — architecture overview");
    bullet("apollo demo               — offline walkthrough of all features");
    bullet("apollo guide api          — REST API quick reference");
}

fn guide_concepts() {
    heading("CONCEPTS");

    heading("Node");
    para("A node is a single Apollo daemon (apollo node start). Each node:
• Runs up to N agents simultaneously (--max-agents, default 200)
• Exposes the REST API on HTTP/HTTPS
• Reports capacity to the Hub coordinator
• Belongs to a region for data-residency routing");

    heading("Agent");
    para("An agent is any executable packaged with an agent.yaml manifest. Apollo
supports: python3, node, go, deno, bun, ruby, php, perl, java, .NET, Rust,
and custom launch commands.");

    para("\nKey manifest fields:");
    kv("name / version", "agent identity and rollback tracking");
    kv("runtime.type", "one of the supported runtimes");
    kv("runtime.entry", "entrypoint file");
    kv("resources", "cpu, memory, timeout limits");
    kv("volumes", "persistent mounts injected as APOLLO_VOLUME_*");
    kv("restart_policy", "max_restarts within window_secs");

    heading("Tenant");
    para("A tenant is any string identifier (user_id, org_id, etc.). Per-tenant:
• Isolated workspace directory
• Secrets injected at spawn time
• Governance policy enforcement
• Usage metering (CPU-seconds, memory-GB-seconds)
• Trace and health data segregation");

    heading("Hub");
    para("The Hub (apollo-hub) is an optional fleet coordinator:
• Aggregates /metrics from all nodes every 10 s
• Routes 'best node' queries by region and load
• Fires auto-scale webhook when utilisation > threshold
• Does not require auth — internal network only");

    heading("v2.0 Platform Layers");
    kv("Observability", "Step-level spans, token costs, billing");
    kv("Governance",    "Per-tenant policy: agents, tools, residency");
    kv("Health",        "Crash detection, scoring, memory leak tracking");
    kv("Memory",        "Cross-session KV store with TF-IDF search");
    kv("Model Routing", "Cost/latency/policy-aware LLM selection");
    kv("Scheduler",     "Cron, interval, one-shot jobs");
    kv("Orchestration", "Blueprints, agent groups, workflow DAGs");
    kv("Arch Selector", "Deterministic / SingleAgent / MultiAgent decisions");
}

fn guide_observability() {
    heading("OBSERVABILITY — Distributed Tracing");
    para("Apollo tracks execution as a tree of spans. Each span records:
• name, status (ok/error/timeout)
• start/end timestamps in milliseconds
• token usage: model, input tokens, output tokens, cost USD
• optional metadata (key-value pairs)");

    heading("Agent → Apollo flow");
    para("Your agent POSTs spans to the node during execution:");
    code(r#"curl -X POST http://localhost:8080/traces/user_123/openclaw/spans \
  -H "X-Apollo-Key: KEY" -H "Content-Type: application/json" \
  -d '{"name":"llm_call","status":"ok","start_ts_ms":1700000000,
       "end_ts_ms":1700000002000,
       "token_usage":{"model":"claude-sonnet-4-6","input_tokens":500,
                      "output_tokens":200,"cost_usd":0.009,"provider":"anthropic"}}'"#);

    heading("Finalize");
    para("After the session ends, finalize to build the cost summary:");
    code("curl -X POST http://localhost:8080/traces/USER/AGENT/TRACE_ID/finalize -H \"X-Apollo-Key: KEY\"");

    heading("CLI commands");
    code("apollo traces list user_123 openclaw        # trace summaries table");
    code("apollo traces get  user_123 openclaw TRACE  # full span tree");
    code("apollo traces tokens user_123               # billing cost rollup");

    heading("Token cost formula");
    para("  cost = (input_tokens / 1_000_000) × cost_per_m_input\n\
          + (output_tokens / 1_000_000) × cost_per_m_output");
    para("\nThe GET /traces/:tenant/tokens endpoint aggregates this across all
traces for the billing period.");
}

fn guide_governance() {
    heading("GOVERNANCE & POLICY");
    para("Every agent start is gated through the PolicyEngine. You set rules
per-tenant; Apollo enforces them automatically.");

    heading("Policy fields");
    kv("max_instances",       "max simultaneous agents for this tenant");
    kv("allowed_agents",      "whitelist of agent IDs (empty = all OK)");
    kv("blocked_agents",      "explicit deny list");
    kv("blocked_tools",       "tool names the agent may not call");
    kv("data_residency",      "required region (e.g. eu-west-1)");
    kv("max_tokens_per_day",  "token quota enforced via trace aggregation");
    kv("require_audit",       "all starts/stops written to events.jsonl");
    kv("model_policy",        "allowed_models list, require_local flag");
    kv("blocked_env_keys",    "env vars stripped before agent spawn");
    kv("forced_env",          "env vars always injected");

    heading("CLI");
    code("apollo policy set user_123 --max-instances 5 --require-audit \\");
    code("    --allowed-agents openclaw,databot --data-residency eu-west-1");
    code("apollo policy get user_123");
    code("apollo policy compliance user_123      # compliance score + violations");
    code("apollo policy delete user_123          # reset to permissive default");

    heading("Enforcement");
    para("A denied start returns HTTP 403 with a human-readable reason:
• Agent not in allowed list
• Region mismatch (data residency)
• Max instances exceeded
• Token quota exceeded");

    heading("Audit trail");
    para("When require_audit is true, every start/stop/rollback is appended to
.apollo/events.jsonl with timestamp, tenant, agent, and outcome.");
}

fn guide_health() {
    heading("HEALTH INTELLIGENCE");
    para("Apollo continuously monitors every running agent and maintains a
0-100 health score updated every 30 seconds.");

    heading("Score components");
    kv("Crash history",   "each crash deducts 15–25 points");
    kv("Crash recency",   "recent crashes penalise more heavily");
    kv("Memory trend",    "sustained leak → deduct points");
    kv("CPU spike",       "sustained >80% → minor deduction");
    kv("Uptime",          "longer stable uptime adds bonus");

    heading("Status tiers");
    kv("Healthy  (80-100)", "normal operation");
    kv("Degraded (40-79)",  "watch; may need intervention");
    kv("Critical (10-39)",  "likely failing; consider restart");
    kv("Dead     (0-9)",    "process gone or unresponsive");

    heading("Crash patterns");
    kv("startup_crash",  "crashes within first 30 s of start");
    kv("oom_loop",       "memory grows then crashes repeatedly");
    kv("periodic",       "crashes on a regular time interval");
    kv("unknown",        "no clear pattern detected");

    heading("Auto-restart");
    para("The restart_policy in agent.yaml sets max_restarts within window_secs.
When health.should_restart() returns true, the scheduler loop respawns
the agent automatically.");

    heading("CLI");
    code("apollo health fleet                        # all nodes summary");
    code("apollo health tenant user_123              # all agents for tenant");
    code("apollo health agent user_123 openclaw      # single agent detail");
    code("apollo dashboard --key KEY                 # live fleet view");
}

fn guide_memory() {
    heading("AGENT MEMORY");
    para("Agents can store and retrieve cross-session state via Apollo's
persistent KV memory layer.");

    heading("Storage");
    para("Each key stores:
• value  — arbitrary JSON
• tags   — searchable labels (e.g. [\"profile\", \"user\"])
• text   — plain-text description for TF-IDF search
• ttl    — optional expiry (seconds from now)");

    heading("TF-IDF similarity search");
    para("Apollo builds a per-store inverted index. At search time:
1. Tokenise the query
2. For each token, look up matching keys via inverted index
3. Score each key: TF × IDF per token, sum, normalise
4. Return top-N results ordered by score");
    para("\nThis gives fast, dependency-free semantic search without an
embedding model or vector database.");

    heading("CLI");
    code(r#"apollo memory put user_123 openclaw prefs '{"theme":"dark"}' --tags profile"#);
    code(r#"apollo memory search user_123 openclaw "user theme preference""#);
    code("apollo memory get    user_123 openclaw prefs");
    code("apollo memory list   user_123 openclaw");
    code("apollo memory stats  user_123 openclaw      # entry count, store size");
    code("apollo memory clear  user_123 openclaw      # wipe entire store");

    heading("REST");
    code("PUT    /memory/:tenant/:agent/:key    { value, tags, text, ttl_secs }");
    code("GET    /memory/:tenant/:agent/:key");
    code("DELETE /memory/:tenant/:agent/:key");
    code("GET    /memory/:tenant/:agent          # list all keys");
    code("POST   /memory/:tenant/:agent/search   { query, tags, limit }");
    code("DELETE /memory/:tenant/:agent          # clear all");
}

fn guide_routing() {
    heading("MODEL ROUTING");
    para("The model router selects the optimal LLM for each request based on
cost, latency, tenant policy, and live performance feedback.");

    heading("Routing signals");
    kv("cost_per_m_input/output", "model pricing in USD per million tokens");
    kv("latency_p50_ms",          "typical response time");
    kv("is_local",                 "prefer local if require_local=true");
    kv("is_available",             "skip unavailable models");
    kv("capabilities",             "required capabilities must be subset");
    kv("policy.allowed_models",    "tenant whitelist enforced");
    kv("EMA feedback",             "exponential moving average of observed latency");

    heading("Algorithm");
    para("1. Filter: remove unavailable, blocked, missing capabilities
2. Rank: score = (budget_remaining / est_cost) × latency_weight × local_pref
3. EMA adjust: multiply score by EMA(latency_observed / latency_p50)
4. Return best model with estimated cost and reasoning");

    heading("CLI");
    code("apollo models list");
    code("apollo models add claude-haiku-4-5 --provider anthropic \\");
    code("    --cost-input 0.25 --cost-output 1.25 --latency-p50 200 \\");
    code("    --capabilities text,code");
    code("apollo models route user_123 --input-tokens 2000 --output-tokens 800 --budget 0.05");
    code("apollo models usage user_123    # per-model cost breakdown");
    code("apollo models remove claude-haiku-4-5");

    heading("Feedback loop");
    para("After each inference, post observed latency back:");
    code("POST /models/feedback  { model_id, latency_ms, is_success }");
    para("The router updates the EMA and adjusts future scores automatically.");
}

fn guide_scheduler() {
    heading("SCHEDULER");
    para("Schedule agents to run on cron expressions, fixed intervals, or
at a specific Unix timestamp. No external cron daemon required.");

    heading("Schedule types");
    kv("cron",     "5-field POSIX expression: min hour dom mon dow");
    kv("interval", "repeat every N seconds");
    kv("once",     "run once at Unix timestamp, then disable");

    heading("CLI");
    code("apollo schedule create --name hourly-report \\");
    code("    --tenant user_123 --agent reporter \\");
    code("    --cron \"0 * * * *\"");
    println!();
    code("apollo schedule create --name heartbeat \\");
    code("    --tenant user_123 --agent monitor --interval 300");
    println!();
    code("apollo schedule list                     # all jobs with next-run time");
    code("apollo schedule get  JOB_ID");
    code("apollo schedule run  JOB_ID              # manual trigger");
    code("apollo schedule history JOB_ID           # run log");
    code("apollo schedule delete JOB_ID");

    heading("Cron syntax");
    para("  ┌────────── minute (0-59)
  │  ┌─────── hour (0-23)
  │  │  ┌──── day-of-month (1-31)
  │  │  │  ┌─ month (1-12)
  │  │  │  │  ┌ day-of-week (0-6, 0=Sun)
  0  *  *  *  *");
    para("\nSpecial: * any   */N every-N   1-5 range   1,3,5 list");

    heading("Background loop");
    para("The scheduler runs inside the node daemon every 30 s. Due jobs
(next_run <= now) are fired as tokio tasks. History is written to
.apollo/scheduler/history/{job_id}.jsonl.");
}

fn guide_orchestration() {
    heading("ORCHESTRATION");
    para("Three layers for deploying agents at scale:");

    heading("1 · Blueprints — Deployment templates");
    para("A Blueprint captures everything needed to deploy a specific agent:
agent ID, pinned version, target region, environment defaults, tags.");
    code("apollo blueprint create --name prod-crawler --agent openclaw \\");
    code("    --pin-version 2.1.0 --region us-east-1");
    code("apollo blueprint deploy BLUEPRINT_ID --tenant user_123");
    para("\nDeploying a blueprint runs the agent for the tenant with the
blueprint's pinned settings, bypassing manual configuration.");

    heading("2 · Agent Groups — Bulk operations");
    para("A Group is a named set of agents for a tenant. Start and stop
all members with one command:");
    code("apollo group create --name etl-suite --tenant user_123 \\");
    code("    --agents extractor,transformer,loader");
    code("apollo group run  GROUP_ID     # start all");
    code("apollo group stop GROUP_ID     # stop all");

    heading("3 · Workflows — DAG execution");
    para("A Workflow defines a directed acyclic graph of agent steps.
Apollo resolves dependencies and runs steps in parallel where possible.");
    code("apollo workflow create --file etl.json");
    code("apollo workflow run    WORKFLOW_ID");
    code("apollo workflow status RUN_ID       # step-by-step progress");
    code("apollo workflow arch   WORKFLOW_ID  # architecture recommendation");

    heading("Workflow JSON format");
    para(r#"  {
    "name": "ETL Pipeline", "tenant_id": "user_123",
    "steps": [
      {"step_id":"fetch",     "agent_id":"scraper",     "depends_on":[]},
      {"step_id":"transform", "agent_id":"processor",   "depends_on":["fetch"]},
      {"step_id":"load",      "agent_id":"db-writer",   "depends_on":["transform"]}
    ]
  }"#);
}

fn guide_arch_selection() {
    heading("ARCHITECTURE SELECTION");
    para("Apollo can automatically recommend the optimal execution model
for a workflow before it runs.");

    heading("Three models");
    kv("Deterministic", "Simple sequential pipeline, no LLM coordination");
    kv("SingleAgent",   "One LLM agent orchestrates all steps");
    kv("MultiAgent",    "Multiple specialised agents running in parallel");

    heading("Scoring signals");
    kv("tool_count",       "≥3 tools → SingleAgent/MultiAgent bonus");
    kv("parallel_branches","≥2 branches → MultiAgent bonus");
    kv("error_tolerance",  "high tolerance → MultiAgent (can retry independently)");
    kv("governance_strict","strict policy → Deterministic (auditability)");
    kv("distinct_agents",  "≥3 distinct agents → MultiAgent bonus");
    kv("optional_steps",   "many optional steps → SingleAgent/MultiAgent");

    heading("CLI");
    code("# Quick heuristic (no workflow file needed)");
    code("apollo arch classify --tools 4 --branches 2 --tolerance 2");
    println!();
    code("# Full analysis from a WorkflowDef JSON");
    code("apollo arch select --file etl.json");
    println!();
    code("# Architecture for a saved workflow");
    code("apollo workflow arch WORKFLOW_ID");

    heading("Output");
    para("Selected:    MultiAgent  (confidence 71%)

  Scores:
    Deterministic   10  █░░░░░░░░░
    SingleAgent     30  ███░░░░░░░
    MultiAgent      60  ██████░░░░

  Reasoning:
  • 4 tools → multi-agent specialisation valuable
  • 2 parallel branches → parallelism pays off
  • Open governance — all types viable

  Config: max_concurrency=2  fail_fast=false  phases=3");
}

fn guide_api() {
    heading("REST API QUICK REFERENCE");
    para("All requests require: X-Apollo-Key: <key>  or  Authorization: Bearer <JWT>");
    println!();

    heading("Node");
    code("GET  /metrics                          node capacity + identity");
    code("GET  /health                           basic liveness check");

    heading("Agents (v1.x)");
    code("GET  /agents/list");
    code("POST /agents/add      { source }");
    code("POST /agents/run      { agent, tenant }");
    code("DEL  /agents/stop     { agent, tenant }");
    code("POST /agents/rollback { agent }");
    code("POST /agents/remove   { agent }");

    heading("Secrets");
    code("PUT  /tenants/:id/secrets   { secrets: {KEY: VAL} }");
    code("DEL  /tenants/:id/secrets");

    heading("Usage Metering");
    code("GET  /usage");
    code("GET  /usage/:tenant");
    code("POST /usage/:tenant/reset");

    heading("Tracing (v2.0)");
    code("POST /traces/:t/:a/spans                 { name, status, start_ts_ms, end_ts_ms, token_usage }");
    code("GET  /traces/:t/:a                        list trace summaries");
    code("GET  /traces/:t/:a/:trace_id              full span tree");
    code("POST /traces/:t/:a/:trace_id/finalize");
    code("GET  /traces/:t/tokens                    billing token rollup");

    heading("Policy (v2.0)");
    code("PUT  /tenants/:t/policy    { max_instances, allowed_agents, … }");
    code("GET  /tenants/:t/policy");
    code("DEL  /tenants/:t/policy");
    code("GET  /tenants/:t/compliance");

    heading("Health (v2.0)");
    code("GET  /health/fleet/summary");
    code("GET  /health/:t                          all agents for tenant");
    code("GET  /health/:t/:a                       single agent");

    heading("Memory (v2.0)");
    code("PUT  /memory/:t/:a/:key    { value, tags, text, ttl_secs }");
    code("GET  /memory/:t/:a/:key");
    code("DEL  /memory/:t/:a/:key");
    code("GET  /memory/:t/:a         list keys");
    code("POST /memory/:t/:a/search  { query, tags, limit }");
    code("GET  /memory/:t/:a/stats");
    code("DEL  /memory/:t/:a         clear all");

    heading("Models (v2.0)");
    code("GET  /models");
    code("PUT  /models/:id    { provider, cost_per_m_input, … }");
    code("DEL  /models/:id");
    code("POST /models/route  { tenant_id, input_tokens, output_tokens, max_cost_usd }");
    code("POST /models/feedback { model_id, latency_ms, is_success }");
    code("GET  /models/usage/:t");

    heading("Scheduler (v2.0)");
    code("GET  /schedule");
    code("POST /schedule       { name, tenant_id, agent_id, schedule:{type,…} }");
    code("GET  /schedule/:id");
    code("DEL  /schedule/:id");
    code("POST /schedule/:id/run");
    code("GET  /schedule/:id/history");

    heading("Blueprints (v2.0)");
    code("GET  /blueprints");
    code("POST /blueprints         { name, agent_id, pin_version, region, tags }");
    code("GET  /blueprints/:id");
    code("DEL  /blueprints/:id");
    code("POST /blueprints/:id/deploy  { tenant_id }");

    heading("Groups (v2.0)");
    code("GET  /groups");
    code("POST /groups         { name, tenant_id, members:[{agent_id}] }");
    code("GET  /groups/:id");
    code("DEL  /groups/:id");
    code("POST /groups/:id/run");
    code("POST /groups/:id/stop");

    heading("Workflows (v2.0)");
    code("GET  /workflows");
    code("POST /workflows        { name, tenant_id, steps:[…] }");
    code("GET  /workflows/:id");
    code("DEL  /workflows/:id");
    code("POST /workflows/:id/run");
    code("GET  /workflows/:id/runs");
    code("GET  /workflows/runs/:run_id");

    heading("Architecture Selection (v2.0)");
    code("POST /architecture/select          { WorkflowDef JSON }");
    code("GET  /architecture/select/:wf_id   analyse saved workflow");
    code("POST /architecture/classify        { tenant_id, tool_count, parallel_branches, … }");

    println!();
    println!("  {}", "Full examples with curl: apollo guide observability, governance, etc.".dimmed());
}
