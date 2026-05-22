//! Fully offline guided demo of all Apollo v2.0 platform modules.
//!
//! No running node required. Simulated data shows potential users exactly
//! what Apollo looks like in production.

use colored::Colorize;
use crate::fmt;

/// Run a single demo module by name, or "all" for the full tour.
pub fn run(module: Option<String>, quick: bool) {
    fmt::apollo_banner();
    println!("{}", "  Welcome to the Apollo v2.0 Platform Demo".cyan().bold());
    println!("{}", "  ─────────────────────────────────────────".dimmed());
    println!("  This demo runs fully offline — no node required.\n");

    if quick {
        run_quick_tour();
        return;
    }

    match module.as_deref() {
        Some("doctor")      | Some("node")          => demo_doctor(),
        Some("agent")       | Some("agents")        => demo_agents(),
        Some("governance")  | Some("policy")        => demo_governance(),
        Some("traces")      | Some("observability") => demo_traces(),
        Some("health")                               => demo_health(),
        Some("memory")                               => demo_memory(),
        Some("models")      | Some("routing")       => demo_routing(),
        Some("schedule")    | Some("scheduler")     => demo_scheduler(),
        Some("orchestration")                        => demo_orchestration(),
        Some("arch")        | Some("architecture")  => demo_arch(),
        Some(other) => {
            fmt::err(&format!("Unknown module '{}'. Try: doctor, governance, traces, health, memory, models, schedule, orchestration, arch", other));
        }
        None => run_full_tour(),
    }
}

fn run_quick_tour() {
    println!("{}", "  ⚡ Quick Tour — 60-second highlights\n".yellow().bold());

    fmt::demo_banner("DOCTOR", "12-point system validation");
    fmt::demo_cmd("apollo doctor");
    fmt::demo_out(&"[OK] Node Engine Initialized".green().to_string());
    fmt::demo_out(&"[OK] Observability Layer Active".green().to_string());
    fmt::demo_out(&"[OK] Policy Engine Active".green().to_string());
    fmt::demo_out(&"[OK] Architecture Selector Active".green().to_string());
    fmt::demo_out(&"[OK] Runtimes Detected: python3, node, rustc, deno".green().to_string());
    fmt::demo_out(&"STATUS: PRODUCTION READY  [Apollo v2.0]".cyan().bold().to_string());

    fmt::demo_banner("DASHBOARD", "Live fleet visibility in the terminal");
    fmt::demo_cmd("apollo dashboard --key my-key");
    fmt::demo_out(&"╔══════════════════════════════════════════════════════╗".cyan().to_string());
    fmt::demo_out(&"║  APOLLO MISSION CONTROL  v2.0  ·  14:30:01 UTC      ║".cyan().to_string());
    fmt::demo_out(&"║  Node: node-3421  ·  us-east-1  ·  Agents: 14/200  ║".cyan().to_string());
    fmt::demo_out(&"║  Healthy: 12  Degraded: 1  Critical: 1              ║".cyan().to_string());
    fmt::demo_out(&"╚══════════════════════════════════════════════════════╝".cyan().to_string());

    fmt::demo_banner("ARCH SELECT", "Automatic architecture selection");
    fmt::demo_cmd("apollo arch classify --tools 4 --branches 2 --tolerance 2");
    fmt::demo_out(&"  Selected:    MultiAgent".cyan().bold().to_string());
    fmt::demo_out("  Confidence:  71%");
    fmt::demo_out("  • 4 tools → multi-agent specialisation valuable");
    fmt::demo_out("  • 2 parallel branches → multi-agent parallelism pays off");

    println!("\n  {}", "Run 'apollo demo' for the full guided tour.".dimmed());
}

fn run_full_tour() {
    let modules: Vec<(&str, fn())> = vec![
        ("Doctor & Node",          demo_doctor),
        ("Agent Lifecycle",        demo_agents),
        ("Governance & Policy",    demo_governance),
        ("Distributed Tracing",    demo_traces),
        ("Health Intelligence",    demo_health),
        ("Agent Memory",           demo_memory),
        ("Model Routing",          demo_routing),
        ("Scheduler",              demo_scheduler),
        ("Orchestration",          demo_orchestration),
        ("Architecture Selection", demo_arch),
    ];

    println!("  {} modules to explore. Press Enter after each section.\n",
        modules.len().to_string().bold());

    for (i, (name, func)) in modules.iter().enumerate() {
        println!("  {} of {} — {}", (i + 1).to_string().cyan(), modules.len(), name.bold());
        func();
        if i < modules.len() - 1 { fmt::demo_pause(); }
    }

    println!("\n{}", "  ══════════════════════════════════════════════════════".cyan());
    println!("{}", "    Demo complete! Apollo v2.0 is production-ready.".cyan().bold());
    println!("{}", "  ══════════════════════════════════════════════════════".cyan());
    println!("\n  Next steps:");
    println!("    apollo guide quick-start    — installation walkthrough");
    println!("    apollo node start           — start your first node");
    println!("    apollo guide api            — full REST API reference\n");
}

// ── Module demos ──────────────────────────────────────────────────────────────

fn demo_doctor() {
    fmt::demo_banner("DOCTOR", "12-point system validation — certifies every deployment");
    fmt::demo_cmd("apollo doctor");
    for line in &[
        "[OK] Node Engine Initialized",
        "[OK] Hub Connectivity Ready",
        "[OK] Event Spine Active",
        "[OK] Security Sandbox Enabled",
        "[OK] Observability Layer Active",
        "[OK] Policy Engine Active",
        "[OK] Health Intelligence Active",
        "[OK] Memory Layer Active",
        "[OK] Model Router Active",
        "[OK] Scheduler Active",
        "[OK] Orchestration APIs Active",
        "[OK] Architecture Selector Active",
        "[OK] Runtimes Detected: python3, node, rustc, deno, ruby",
        "STATUS: PRODUCTION READY  [Apollo v2.0]",
    ] {
        if line.starts_with("[OK]") {
            fmt::demo_out(&line.green().to_string());
        } else {
            fmt::demo_out(&line.cyan().bold().to_string());
        }
    }
}

fn demo_agents() {
    fmt::demo_banner("AGENT LIFECYCLE", "Register, run, monitor, rollback — any language");
    fmt::demo_cmd("apollo agent --base-dir .apollo add https://github.com/org/openclaw.git");
    fmt::demo_out(&"✓ Registered: openclaw v2.1.0  sha256:a3f1b2c4d5e6".green().to_string());

    fmt::demo_cmd("apollo agent --base-dir .apollo run openclaw --tenant user_123");
    fmt::demo_out(&"✓ Running: openclaw (tenant=user_123)  PID=78432  port=34201".green().to_string());

    fmt::demo_cmd("apollo agent --base-dir .apollo list");
    fmt::demo_out("  NAME                 VERSION      RUNTIME        CHECKSUM");
    fmt::demo_out("  openclaw             2.1.0        python3        a3f1b2c4d5e6");
    fmt::demo_out("  databot              1.4.2        node           f9e8d7c6b5a4");
    fmt::demo_out("  reporter             1.0.0        go             1a2b3c4d5e6f");
}

fn demo_governance() {
    fmt::demo_banner("GOVERNANCE & POLICY", "Per-tenant rules enforced before every agent start");
    fmt::demo_cmd("apollo policy set user_123 --allowed-agents openclaw,databot --data-residency eu-west-1 --require-audit");
    fmt::demo_out(&"[OK] Policy saved for tenant 'user_123'".green().to_string());

    fmt::demo_cmd("apollo policy get user_123");
    fmt::demo_out("  Max instances:       5");
    fmt::demo_out("  Allowed agents:      openclaw, databot");
    fmt::demo_out("  Data residency:      eu-west-1");
    fmt::demo_out("  Require audit:       true");
    fmt::demo_out("  Max tokens/day:      1,000,000");

    fmt::demo_cmd("apollo policy compliance user_123");
    fmt::demo_out("  Policy active:       true");
    fmt::demo_out("  Audit enabled:       true");
    fmt::demo_out("  Agent whitelist:     set");
    fmt::demo_out(&"  Compliance score:    92/100".green().to_string());
    fmt::demo_out(&"[OK] No compliance violations found.".green().to_string());
}

fn demo_traces() {
    fmt::demo_banner("DISTRIBUTED TRACING", "Step-level spans with token costs — billing-ready");
    fmt::demo_cmd("apollo traces list user_123 openclaw");
    fmt::demo_out("  TRACE ID   SPANS  STATUS  COST $   DURATION ms  FINALIZED");
    fmt::demo_out("  a3f1b2c4     4    Ok      0.0124   1,823        yes");
    fmt::demo_out("  d5e6f7a8     2    Ok      0.0056   903          yes");
    fmt::demo_out("  b9c0d1e2     7    Ok      0.0298   4,211        yes");

    fmt::demo_cmd("apollo traces tokens user_123");
    fmt::demo_out("  Total input tokens:   48,302");
    fmt::demo_out("  Total output tokens:  12,891");
    fmt::demo_out(&"  Total cost (USD):     $0.0478".yellow().to_string());
    fmt::demo_out("  Trace count:          3");
    fmt::demo_out("");
    fmt::demo_out("  MODEL                  INPUT TOKENS   OUTPUT TOKENS   COST $");
    fmt::demo_out("  claude-sonnet-4-6      48,302         12,891          0.0478");
}

fn demo_health() {
    fmt::demo_banner("HEALTH INTELLIGENCE", "Continuous scoring: crashes, CPU, memory trends");
    fmt::demo_cmd("apollo health fleet");
    fmt::demo_out("  Total agents:   14");
    fmt::demo_out(&"  Healthy:        12".green().to_string());
    fmt::demo_out(&"  Degraded:        1".yellow().to_string());
    fmt::demo_out(&"  Critical:        1".red().to_string());
    fmt::demo_out("  Avg score:      88.4");
    fmt::demo_out(&"  Fleet health:   ████████░░ 88".green().to_string());

    fmt::demo_cmd("apollo health agent user_789 reporter");
    fmt::demo_out(&"  Status:         Degraded".yellow().to_string());
    fmt::demo_out(&"  Score:          45  ████░░░░░░ 45".yellow().to_string());
    fmt::demo_out("  Crash count:    3");
    fmt::demo_out(&"  Crash pattern:  startup_crash".red().to_string());
    fmt::demo_out("  Memory trend:   +2.4 MB/min");
}

fn demo_memory() {
    fmt::demo_banner("AGENT MEMORY", "Cross-session KV store with TF-IDF similarity search");
    fmt::demo_cmd(r#"apollo memory put user_123 openclaw user_prefs '{"theme":"dark","lang":"en"}' --tags profile"#);
    fmt::demo_out(&"[OK] Stored memory key 'user_prefs' for user_123/openclaw".green().to_string());

    fmt::demo_cmd("apollo memory search user_123 openclaw \"user theme preference\"");
    fmt::demo_out("  KEY          SCORE  TAGS     VALUE PREVIEW");
    fmt::demo_out("  user_prefs   0.67   profile  {\"theme\":\"dark\",\"lang\":\"en\"}");

    fmt::demo_cmd("apollo memory stats user_123 openclaw");
    fmt::demo_out("  Live entries:   3");
    fmt::demo_out("  Total entries:  3");
    fmt::demo_out("  Store size:     412 bytes");
}

fn demo_routing() {
    fmt::demo_banner("MODEL ROUTING", "Cost/latency/policy-aware LLM selection with EMA feedback");
    fmt::demo_cmd("apollo models list");
    fmt::demo_out("  MODEL ID              PROVIDER    COST IN $/M  COST OUT $/M  LATENCY p50  LOCAL  AVAIL");
    fmt::demo_out("  claude-sonnet-4-6     anthropic   3.00         15.00         800ms        no     ✓");
    fmt::demo_out("  claude-haiku-4-5      anthropic   0.25          1.25         200ms        no     ✓");
    fmt::demo_out("  llama-3-70b           local       0.00          0.00         400ms        yes    ✓");

    fmt::demo_cmd("apollo models route user_123 --input-tokens 2000 --output-tokens 800 --budget 0.05");
    fmt::demo_out(&"  Selected model:     claude-haiku-4-5".cyan().bold().to_string());
    fmt::demo_out("  Provider:           anthropic");
    fmt::demo_out(&"  Est. cost (USD):    $0.0015".green().to_string());
    fmt::demo_out("  Est. latency:       200ms");
    fmt::demo_out("  Reason:             lowest cost within budget, satisfies all capabilities");
}

fn demo_scheduler() {
    fmt::demo_banner("SCHEDULER", "Cron, interval, and one-shot jobs — built-in parser, no deps");
    fmt::demo_cmd("apollo schedule create --name hourly-report --tenant user_123 --agent reporter --cron \"0 * * * *\"");
    fmt::demo_out(&"[OK] Created job 'hourly-report' (id: a1b2c3d4)".green().to_string());

    fmt::demo_cmd("apollo schedule list");
    fmt::demo_out("  ID        NAME            TENANT     AGENT      TYPE          NEXT RUN    RUNS  FAILS  ENABLED");
    fmt::demo_out("  a1b2c3d4  hourly-report   user_123   reporter   cron(0 * * * *)  in 23m  48    0      ✓");
    fmt::demo_out("  e5f6a7b8  heartbeat       user_456   monitor    every 300s    in 4m       201   2      ✓");

    fmt::demo_cmd("apollo schedule run a1b2c3d4");
    fmt::demo_out(&"[OK] Triggered job 'a1b2c3'".green().to_string());
}

fn demo_orchestration() {
    fmt::demo_banner("ORCHESTRATION", "Blueprints · Agent groups · Workflow DAGs");

    fmt::demo_cmd("apollo blueprint create --name prod-crawler --agent openclaw --pin-version 2.1.0 --region us-east-1");
    fmt::demo_out(&"[OK] Created blueprint 'prod-crawler' (id: bp-a1b2c3)".green().to_string());

    fmt::demo_cmd("apollo blueprint deploy bp-a1b2c3 --tenant user_123");
    fmt::demo_out(&"[OK] Deployed blueprint for tenant 'user_123'".green().to_string());

    fmt::demo_cmd("apollo workflow create --file etl.json");
    fmt::demo_out(&"[OK] Created workflow 'ETL Pipeline' (id: wf-d4e5f6)".green().to_string());

    fmt::demo_cmd("apollo workflow run wf-d4e5f6");
    fmt::demo_out("  STEP ID    STATUS      AGENT PID  STARTED     FINISHED");
    fmt::demo_out(&"  extract    Completed   78432       T+0s        T+12s".green().to_string());
    fmt::demo_out(&"  transform  Completed   78433       T+12s       T+28s".green().to_string());
    fmt::demo_out(&"  load       Running     78434       T+28s       —".yellow().to_string());
}

fn demo_arch() {
    fmt::demo_banner("ARCHITECTURE SELECTION", "DAG topology + governance → Deterministic / SingleAgent / MultiAgent");
    fmt::demo_cmd("apollo arch classify --tools 4 --branches 2 --tolerance 2");
    fmt::demo_out(&"  Selected:    MultiAgent".cyan().bold().to_string());
    fmt::demo_out("  Confidence:  71%");
    fmt::demo_out("");
    fmt::demo_out("  Scores:");
    fmt::demo_out(&"    Deterministic   10  █░░░░░░░░░".red().to_string());
    fmt::demo_out(&"    SingleAgent     30  ███░░░░░░░".yellow().to_string());
    fmt::demo_out(&"    MultiAgent      60  ██████░░░░".cyan().to_string());
    fmt::demo_out("");
    fmt::demo_out("  Reasoning:");
    fmt::demo_out("  • 4 tools → multi-agent specialisation valuable");
    fmt::demo_out("  • 2 parallel branches → multi-agent parallelism pays off");
    fmt::demo_out("  • Open governance — all architecture types viable");
    fmt::demo_out("");
    fmt::demo_out("  Config:  max_concurrency=2  fail_fast=false  phases=3");

    fmt::demo_cmd("apollo workflow arch wf-d4e5f6");
    fmt::demo_out(&"  Selected:    MultiAgent  (confidence: 74%)".cyan().bold().to_string());
    fmt::demo_out("  DAG:  5 steps · 3 agents · width 3 · critical path 3");
    fmt::demo_out("  Governance: all agents allowed · strictness 0%");
}
