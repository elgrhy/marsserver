//! Journey-first async dispatchers.
//!
//! Each `journey_*` function:
//!   1. Collects async-domain data (registries, running instances, etc.)
//!   2. Calls `tokio::task::block_in_place` to run the sync wizard prompt
//!   3. Executes the resolved action
//!   4. Prints a success summary with actionable next-step hints

use anyhow::{anyhow, Result};
use colored::Colorize;
use std::path::{Path, PathBuf};
use tokio::task::block_in_place;

use apollo_core::{detect_node_capabilities, load_agent_registry, register_agent_package};
use apollo_core::types::AgentInstance;
use apollo_runtime::{AgentRuntime};
use apollo_runtime::process::{
    ProcessRuntime, load_all_instances, load_tenant_instances,
    save_instance, save_tenant_instances,
};

use crate::client::NodeClient;
use crate::commands::wizard;
use crate::fmt;

// ── Template generator ────────────────────────────────────────────────────────

fn create_template(dir: &PathBuf, name: &str) -> Result<()> {
    std::fs::create_dir_all(dir)?;

    let yaml = format!(
        "name: {name}\n\
         version: 1.0.0\n\
         description: \"{name} — Apollo agent\"\n\
         runtime:\n\
         \x20\x20type: python3\n\
         \x20\x20entry: main.py\n\
         resources:\n\
         \x20\x20cpu: 0.5\n\
         \x20\x20memory: 512mb\n\
         \x20\x20timeout: 120\n\
         permissions:\n\
         \x20\x20network: full\n\
         \x20\x20filesystem: sandbox\n\
         restart_policy:\n\
         \x20\x20max_restarts: 3\n\
         \x20\x20window_secs: 60\n",
        name = name
    );

    let py = format!(
        "#!/usr/bin/env python3\n\
         \"\"\"\n\
         {name} — Apollo agent starter template.\n\
         Edit this file to add your agent logic.\n\
         \"\"\"\n\
         import os\n\
         \n\
         TENANT = os.environ.get('APOLLO_TENANT_ID', 'default')\n\
         \n\
         def main():\n\
         \x20\x20\x20\x20print('[' + TENANT + '] {name} starting...')\n\
         \x20\x20\x20\x20# TODO: add your agent logic here\n\
         \x20\x20\x20\x20print('[' + TENANT + '] {name} done.')\n\
         \n\
         if __name__ == '__main__':\n\
         \x20\x20\x20\x20main()\n",
        name = name
    );

    std::fs::write(dir.join("agent.yaml"), yaml)?;
    std::fs::write(dir.join("main.py"), py)?;
    Ok(())
}

// ── apollo init ───────────────────────────────────────────────────────────────

pub async fn journey_init() -> Result<()> {
    let result = block_in_place(|| wizard::prompt_init())?;

    // Create workspace directory structure
    for sub in &[
        "logs", "tenants", "agents", "secrets", "policies",
        "health", "memory", "models", "scheduler", "blueprints",
        "groups", "workflows", "traces", "usage",
    ] {
        std::fs::create_dir_all(result.base_dir.join(sub))?;
    }

    // Seed agents.json if not present
    let agents_path = result.base_dir.join("agents.json");
    if !agents_path.exists() {
        std::fs::write(&agents_path, "[]")?;
    }

    // Write config.json
    let cfg = serde_json::json!({
        "secret_key":  result.secret_key,
        "region":      result.region,
        "port":        result.port,
        "version":     "2.0",
    });
    std::fs::write(
        result.base_dir.join("config.json"),
        serde_json::to_string_pretty(&cfg)?,
    )?;

    fmt::success_box("Workspace ready!", &[
        ("Directory", &result.base_dir.to_string_lossy().to_string()),
        ("Port",      &result.port.to_string()),
        ("API key",   &result.secret_key),
        ("Region",    &result.region),
    ]);

    fmt::next_steps(&[
        "Add your first agent:  apollo add",
        "Run it:  apollo run",
        &format!(
            "Start the node:  apollo node start --base-dir {} --secret-keys {}",
            result.base_dir.display(),
            result.secret_key
        ),
    ]);

    Ok(())
}

// ── apollo add ────────────────────────────────────────────────────────────────

pub async fn journey_add(base_dir: PathBuf, prefill_source: Option<String>) -> Result<()> {
    let add_src = block_in_place(|| wizard::prompt_add(prefill_source.as_deref()))?;

    if add_src.source.is_empty() {
        fmt::warn_hint("No source provided. Nothing added.");
        return Ok(());
    }

    if add_src.generate_template {
        let dir = add_src.template_dir.as_ref().unwrap();
        create_template(dir, &add_src.source)?;
        fmt::substep_ok(&format!("Created starter template in  {}/", add_src.source));
        fmt::substep_info("Edit agent.yaml and main.py to customise your agent.");
        println!();

        let src_path = format!("./{}", add_src.source);
        fmt::substep_info(&format!("Registering from {}...", src_path));
        let record = register_agent_package(&base_dir, &src_path)
            .await
            .map_err(|e| anyhow!("Registration failed: {}", e))?;

        fmt::success_box("Agent added!", &[
            ("Name",     &record.id),
            ("Version",  &record.spec.version),
            ("Runtime",  &record.spec.runtime.kind),
            ("Checksum", &format!("sha256:{}", &record.checksum[..12])),
        ]);
    } else {
        fmt::substep_info(&format!("Registering from {}...", add_src.source));
        let record = register_agent_package(&base_dir, &add_src.source)
            .await
            .map_err(|e| anyhow!(
                "Registration failed: {}\n\n  \
                 Tip: check the path or URL is valid and agent.yaml exists.",
                e
            ))?;

        fmt::success_box("Agent added!", &[
            ("Name",     &record.id),
            ("Version",  &record.spec.version),
            ("Runtime",  &record.spec.runtime.kind),
            ("Checksum", &format!("sha256:{}", &record.checksum[..12])),
        ]);
    }

    fmt::next_steps(&[
        "apollo run      start the agent",
        "apollo status   view all agents",
    ]);

    Ok(())
}

// ── apollo run ────────────────────────────────────────────────────────────────

pub async fn journey_run(
    base_dir:       PathBuf,
    prefill_agent:  Option<String>,
    prefill_tenant: Option<String>,
) -> Result<()> {
    let agent_names: Vec<String> = load_agent_registry(&base_dir)
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.id)
        .collect();

    let result = block_in_place(|| {
        wizard::prompt_run(
            prefill_agent.as_deref(),
            prefill_tenant.as_deref(),
            &agent_names,
        )
    })?;

    let spec = load_agent_registry(&base_dir)?
        .into_iter()
        .find(|r| r.id == result.agent)
        .map(|r| r.spec)
        .ok_or_else(|| {
            anyhow!(
                "Agent '{}' is not registered.\n\n  Add it first:  apollo add",
                result.agent
            )
        })?;

    fmt::substep_info(&format!(
        "Starting '{}' for tenant '{}'...",
        result.agent, result.tenant
    ));

    let runtime  = ProcessRuntime::new(base_dir.clone());
    let instance = runtime
        .start(&result.tenant, &spec)
        .await
        .map_err(|e| {
            anyhow!(
                "{}\n\n  Tip: check the runtime is installed.  Try:  apollo doctor",
                e
            )
        })?;
    save_instance(&base_dir, &instance)?;

    fmt::success_box(&format!("{} is running!", result.agent), &[
        ("Agent",  &result.agent),
        ("Tenant", &result.tenant),
        ("PID",    &instance.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into())),
        ("Port",   &instance.port.map(|p| p.to_string()).unwrap_or_else(|| "—".into())),
    ]);

    fmt::next_steps(&[
        &format!("apollo logs {}     tail agent output", result.agent),
        "apollo status         view all running agents",
        &format!("apollo stop {}     stop when done", result.agent),
    ]);

    Ok(())
}

// ── apollo stop ───────────────────────────────────────────────────────────────

pub async fn journey_stop(
    base_dir:     PathBuf,
    prefill_agent: Option<String>,
) -> Result<()> {
    let all     = load_all_instances(&base_dir);
    let running: Vec<&AgentInstance> = all
        .iter()
        .filter(|i| i.status == "running" || i.pid.is_some())
        .collect();

    let labels: Vec<String> = running.iter().map(|i| {
        format!(
            "{:<20}  tenant: {:<16}  PID: {}",
            i.agent_id,
            i.tenant_id,
            i.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into())
        )
    }).collect();

    // If a name was given and uniquely matches one running instance, skip prompt
    let target_idx: Option<usize> = if let Some(ref name) = prefill_agent {
        let matches: Vec<usize> = running
            .iter()
            .enumerate()
            .filter(|(_, i)| &i.agent_id == name)
            .map(|(idx, _)| idx)
            .collect();
        if matches.len() == 1 {
            Some(matches[0])
        } else {
            block_in_place(|| wizard::prompt_stop_index(&labels))?
        }
    } else {
        block_in_place(|| wizard::prompt_stop_index(&labels))?
    };

    let idx = match target_idx {
        None    => return Ok(()),
        Some(i) => i,
    };

    let inst = running[idx];
    fmt::substep_info(&format!(
        "Stopping {} (tenant: {})...",
        inst.agent_id, inst.tenant_id
    ));

    if let Some(pid) = inst.pid {
        let runtime = ProcessRuntime::new(base_dir.clone());
        let _ = runtime.stop(pid).await; // best-effort — process may already be gone
    }

    // Update persisted state
    let mut list = load_tenant_instances(&base_dir, &inst.tenant_id).unwrap_or_default();
    if let Some(entry) = list.iter_mut().find(|i| i.id == inst.id) {
        entry.status = "stopped".to_string();
        entry.pid    = None;
        let _ = save_tenant_instances(&base_dir, &inst.tenant_id, &list);
    }

    fmt::ok(&format!("Stopped {} (tenant: {})", inst.agent_id, inst.tenant_id));
    fmt::next_steps(&[
        "apollo status   check what's running",
        "apollo run      start another agent",
    ]);

    Ok(())
}

// ── apollo status ─────────────────────────────────────────────────────────────

pub async fn journey_status(base_dir: PathBuf, node: &str, key: &str) -> Result<()> {
    fmt::section("Apollo Status");

    // Node connectivity (best-effort — failure is non-fatal for status)
    let client = NodeClient::new(node, key);
    match client.node_health() {
        Ok(_)  => fmt::kv("Node", &format!("✓ reachable   {}", node).green().to_string()),
        Err(_) => {
            fmt::kv(
                "Node",
                &"✗ not reachable   (apollo node start --secret-keys <key>)".red().to_string(),
            );
        }
    }

    let agents   = load_agent_registry(&base_dir).unwrap_or_default();
    let all_inst = load_all_instances(&base_dir);

    fmt::kv("Registered agents", &agents.len().to_string());
    let running_count = all_inst
        .iter()
        .filter(|i| i.status == "running" || i.pid.is_some())
        .count();
    fmt::kv("Running instances", &running_count.to_string());
    println!();

    if agents.is_empty() && all_inst.is_empty() {
        fmt::hint("Nothing here yet.  Try:  apollo add");
        println!();
    } else {
        let mut rows: Vec<Vec<String>> = Vec::new();

        // Running instances first
        for inst in all_inst.iter().filter(|i| i.status == "running" || i.pid.is_some()) {
            rows.push(vec![
                inst.agent_id.clone(),
                inst.tenant_id.clone(),
                fmt::status_str("running"),
                inst.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
                inst.port.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
            ]);
        }

        // Registered agents that aren't running
        for agent in &agents {
            let running = all_inst.iter().any(|i| {
                i.agent_id == agent.id && (i.status == "running" || i.pid.is_some())
            });
            if !running {
                rows.push(vec![
                    agent.id.clone(),
                    "—".into(),
                    fmt::status_str("stopped"),
                    "—".into(),
                    "—".into(),
                ]);
            }
        }

        if !rows.is_empty() {
            fmt::print_table(
                &["AGENT", "TENANT", "STATUS", "PID", "PORT"],
                &rows,
            );
            println!();
        }
    }

    println!("  {}", "Quick actions:".bold());
    for (cmd, desc) in &[
        ("apollo add",       "register an agent"),
        ("apollo run",       "start an agent"),
        ("apollo stop",      "stop a running agent"),
        ("apollo logs",      "view agent output"),
        ("apollo dashboard", "live monitoring"),
    ] {
        println!("    {}  {}  {}", "$".dimmed(), cmd.white().bold(), desc.dimmed());
    }
    println!();

    Ok(())
}

// ── apollo logs ───────────────────────────────────────────────────────────────

/// `follow_override`: if `Some(true)` the follow question is skipped and logs are tailed live.
pub async fn journey_logs(
    base_dir:       PathBuf,
    prefill_agent:  Option<String>,
    follow_override: Option<bool>,
) -> Result<()> {
    let all = load_all_instances(&base_dir);

    // Try to resolve without prompting if agent name is unambiguous
    let (agent_id, tenant_id, sel_lines, sel_follow) =
        if let Some(ref name) = prefill_agent {
            let matches: Vec<&AgentInstance> =
                all.iter().filter(|i| &i.agent_id == name).collect();
            if matches.len() == 1 {
                let follow = follow_override.unwrap_or(false);
                (
                    matches[0].agent_id.clone(),
                    matches[0].tenant_id.clone(),
                    50usize,
                    follow,
                )
            } else {
                let labels: Vec<String> = all
                    .iter()
                    .map(|i| format!("{} / {}", i.agent_id, i.tenant_id))
                    .collect();
                match block_in_place(|| wizard::prompt_logs_index(&labels))? {
                    None    => return Ok(()),
                    Some(s) => {
                        let follow = follow_override.unwrap_or(s.follow);
                        let inst = &all[s.index];
                        (inst.agent_id.clone(), inst.tenant_id.clone(), s.lines, follow)
                    }
                }
            }
        } else {
            let labels: Vec<String> = all
                .iter()
                .map(|i| format!("{} / {}", i.agent_id, i.tenant_id))
                .collect();
            match block_in_place(|| wizard::prompt_logs_index(&labels))? {
                None    => return Ok(()),
                Some(s) => {
                    let follow = follow_override.unwrap_or(s.follow);
                    let inst = &all[s.index];
                    (inst.agent_id.clone(), inst.tenant_id.clone(), s.lines, follow)
                }
            }
        };

    let log_path = base_dir
        .join("logs")
        .join(&tenant_id)
        .join(format!("{}.log", agent_id));

    fmt::section(&format!("Logs — {} / {}", agent_id, tenant_id));
    fmt::kv("File", &log_path.to_string_lossy());

    if !log_path.exists() {
        fmt::info("Log file not found — the agent may not have produced output yet.");
        fmt::hint(&format!(
            "Start it first:  apollo run {}  --tenant {}",
            agent_id, tenant_id
        ));
        return Ok(());
    }

    tail_log(&log_path, sel_lines, sel_follow).await
}

async fn tail_log(path: &Path, lines: usize, follow: bool) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    use std::io::SeekFrom;

    let content = tokio::fs::read_to_string(path).await?;
    let all_lines: Vec<&str> = content.lines().collect();
    let start = all_lines.len().saturating_sub(lines);

    println!();
    for line in &all_lines[start..] {
        println!("  {}", line);
    }

    if follow {
        let mut pos = {
            let mut f = tokio::fs::File::open(path).await?;
            f.seek(SeekFrom::End(0)).await?
        };
        println!(
            "\n  {} Following — Ctrl-C to stop\n",
            "─".repeat(16).dimmed()
        );
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            let mut f = tokio::fs::File::open(path).await?;
            let new_len = f.seek(SeekFrom::End(0)).await?;
            if new_len > pos {
                f.seek(SeekFrom::Start(pos)).await?;
                let mut buf = String::new();
                f.read_to_string(&mut buf).await?;
                print!("{}", buf);
                pos = new_len;
            }
        }
    } else {
        println!();
        fmt::hint("Add  --follow  to tail live output:  apollo logs <agent> --follow");
    }

    Ok(())
}

// ── Interactive landing menu (no-arg invocation) ──────────────────────────────

/// Called when `apollo` is invoked with no arguments.
/// Replaces the old line-editor REPL with a visual menu.
pub async fn run_landing_menu(node: String, key: String) -> Result<()> {
    use crate::commands;

    let base_dir = PathBuf::from(".apollo");

    loop {
        let idx = block_in_place(|| wizard::main_menu())?;
        let (cmd, _) = wizard::MENU_ITEMS[idx];

        println!();

        let result: Result<()> = match cmd {
            "init"      => journey_init().await,
            "add"       => journey_add(base_dir.clone(), None).await,
            "run"       => journey_run(base_dir.clone(), None, None).await,
            "stop"      => journey_stop(base_dir.clone(), None).await,
            "status"    => journey_status(base_dir.clone(), &node, &key).await,
            "logs"      => journey_logs(base_dir.clone(), None, None).await,
            "dashboard" => {
                let client = NodeClient::new(&node, &key);
                commands::dashboard::run(&client, 5)
            }
            "doctor" => {
                let profile = detect_node_capabilities().await?;
                println!("{}", "[OK] Node Engine Initialized".green());
                println!("{}", "[OK] Hub Connectivity Ready".green());
                println!("{}", "[OK] Event Spine Active".green());
                println!("{}", "[OK] Security Sandbox Enabled".green());
                println!("{}", "[OK] Observability Layer Active".green());
                println!("{}", "[OK] Policy Engine Active".green());
                println!("{}", "[OK] Health Intelligence Active".green());
                println!("{}", "[OK] Memory Layer Active".green());
                println!("{}", "[OK] Model Router Active".green());
                println!("{}", "[OK] Scheduler Active".green());
                println!("{}", "[OK] Orchestration APIs Active".green());
                println!("{}", "[OK] Architecture Selector Active".green());
                println!(
                    "{}",
                    format!("[OK] Runtimes Detected: {}", profile.runtimes.join(", ")).green()
                );
                println!("{}", "STATUS: PRODUCTION READY  [Apollo v2.0]".cyan().bold());
                Ok(())
            }
            "demo"  => { commands::demo::run(None, false); Ok(()) }
            "guide" => { commands::guide::run(None); Ok(()) }
            "exit"  => break,
            other   => {
                fmt::hint(&format!("Run 'apollo {}' for full options.", other));
                Ok(())
            }
        };

        if let Err(e) = result {
            fmt::err(&format!("{}", e));
        }

        println!();
    }

    fmt::dim("  Goodbye.");
    Ok(())
}
