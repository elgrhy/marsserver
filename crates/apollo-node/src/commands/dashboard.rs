//! Live auto-refresh terminal dashboard — Apollo Mission Control.
//!
//! Connects to a running Apollo node and redraws fleet status, health,
//! agents, and scheduled jobs every N seconds. Press 'q' or Ctrl+C to exit.

use anyhow::Result;
use colored::Colorize;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{self, ClearType},
};
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crate::client::NodeClient;

pub fn run(client: &NodeClient, refresh_secs: u64) -> Result<()> {
    // Switch to raw mode so we can read keypress without Enter
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let refresh = Duration::from_secs(refresh_secs);
    let mut last_draw = Instant::now() - refresh; // force immediate draw
    let mut countdown = 0u64;

    loop {
        // Handle keypresses (non-blocking)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('r') => { last_draw = Instant::now() - refresh; }
                    _ => {}
                }
            }
        }

        // Redraw if refresh interval elapsed
        let elapsed = last_draw.elapsed();
        if elapsed >= refresh {
            last_draw = Instant::now();
            countdown = refresh_secs;
            execute!(stdout, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
            draw_dashboard(&mut stdout, client, refresh_secs);
            stdout.flush()?;
        } else {
            // Update countdown line only
            countdown = refresh_secs.saturating_sub(elapsed.as_secs());
            let (_, rows) = terminal::size().unwrap_or((80, 24));
            execute!(stdout, cursor::MoveTo(0, rows - 1))?;
            let bar = format!(
                " Refreshing in {}s  [r] now  [q] quit ",
                countdown
            );
            print!("{}", bar.on_black().white());
            stdout.flush()?;
        }
    }

    // Restore terminal
    execute!(stdout, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

fn draw_dashboard(stdout: &mut io::Stdout, client: &NodeClient, refresh_secs: u64) {
    let width = terminal::size().map(|(w, _)| w as usize).unwrap_or(80).min(100);
    let now = chrono_now();

    // ── Header ────────────────────────────────────────────────────────────────
    let title = format!("  APOLLO MISSION CONTROL  v2.0  ·  {}  ", now);
    let pad = width.saturating_sub(title.len() + 2);
    println!("{}", format!("╔{}╗", "═".repeat(width - 2)).cyan());
    println!("{}", format!("║{}{}║", title.cyan().bold(), " ".repeat(pad)).cyan());
    println!("{}", format!("╠{}╣", "═".repeat(width - 2)).cyan());

    // ── Fleet + Health (side by side) ─────────────────────────────────────────
    let half = (width - 3) / 2;

    match (client.metrics(), client.health_fleet()) {
        (Ok(metrics), Ok(health)) => {
            let active  = metrics["active_agents"].as_u64().unwrap_or(0);
            let max_ag  = metrics["max_agents"].as_u64().unwrap_or(200);
            let node_id = metrics["node_id"].as_str().unwrap_or("?");
            let region  = metrics["region"].as_str().unwrap_or("?");

            let fleet_lines = vec![
                format!("Node:    {}", node_id.cyan()),
                format!("Region:  {}", region.yellow()),
                format!("Agents:  {}/{}", active.to_string().green(), max_ag),
                format!("Version: {}", "v2.0".cyan()),
            ];

            let total = health.total_instances.max(1);
            let h_bar = |n: usize| "█".repeat(n * 12 / total).to_string();
            let health_lines = vec![
                format!("● Healthy   {:>3}  {}", health.healthy,
                    h_bar(health.healthy).green()),
                format!("● Degraded  {:>3}  {}", health.degraded,
                    h_bar(health.degraded).yellow()),
                format!("● Critical  {:>3}  {}", health.critical,
                    h_bar(health.critical).red()),
                format!("● Dead      {:>3}  {}", health.dead,
                    h_bar(health.dead).dimmed()),
            ];

            print_two_panes(stdout, "FLEET", &fleet_lines, "HEALTH", &health_lines, half, width);
        }
        _ => {
            let msg = "  Unable to reach node. Is 'apollo node start' running?".red().to_string();
            println!("║{:<width$}║", msg, width = width - 2);
        }
    }

    println!("{}", format!("╠{}╣", "═".repeat(width - 2)).cyan());

    // ── Live Agents ───────────────────────────────────────────────────────────
    section_header(stdout, "LIVE AGENTS", width);
    match client.health_fleet() {
        Ok(summary) if summary.total_instances > 0 => {
            // Best effort: pull per-tenant health from fleet summary details
            print_agent_row_header(width);
            // We show fleet-level summary since we don't have per-agent listing here
            let score = summary.avg_health_score as u32;
            let bar = crate::fmt::health_bar(score);
            let row = format!("║  {:<20} {:<12} {:<9} {}",
                "(fleet aggregate)", "", "", bar);
            let pad = width.saturating_sub(strip_ansi_len(&row));
            println!("{}{}║", row, " ".repeat(pad));
        }
        _ => {
            let msg = "  No active agents.";
            println!("║{:<width$}║", msg, width = width - 2);
        }
    }

    println!("{}", format!("╠{}╣", "═".repeat(width - 2)).cyan());

    // ── Scheduled Jobs ────────────────────────────────────────────────────────
    section_header(stdout, "SCHEDULED JOBS", width);
    match client.schedule_list() {
        Ok(jobs) if !jobs.is_empty() => {
            let header = format!("║  {:<22} {:<12} {:<20} {:<6} {:<6}",
                "NAME".bold(), "TYPE".bold(), "NEXT RUN".bold(), "RUNS".bold(), "FAILS".bold());
            let pad = width.saturating_sub(strip_ansi_len(&header));
            println!("{}{}║", header, " ".repeat(pad));
            for job in jobs.iter().take(4) {
                let stype = match &job.schedule {
                    apollo_core::scheduler::Schedule::Cron { .. }     => "cron",
                    apollo_core::scheduler::Schedule::Interval { .. } => "interval",
                    apollo_core::scheduler::Schedule::Once { .. }     => "once",
                };
                let next = format_next_compact(job.next_run);
                let row = format!("║  {:<22} {:<12} {:<20} {:<6} {:<6}",
                    &job.name, stype, next, job.run_count, job.fail_count);
                let pad = width.saturating_sub(strip_ansi_len(&row));
                println!("{}{}║", row, " ".repeat(pad));
            }
        }
        _ => {
            let msg = "  No scheduled jobs.";
            println!("║{:<width$}║", msg, width = width - 2);
        }
    }

    // ── Footer ────────────────────────────────────────────────────────────────
    println!("{}", format!("╚{}╝", "═".repeat(width - 2)).cyan());
    let footer = format!(" Refreshing in {}s  [r] now  [q] quit ", refresh_secs);
    print!("{}", footer.on_black().white());
    let _ = stdout.flush();
}

fn section_header(_stdout: &mut io::Stdout, title: &str, width: usize) {
    let label = format!("  {}  ", title.bold());
    let label_plain = format!("  {}  ", title);
    let pad = width.saturating_sub(label_plain.len() + 2);
    println!("║{}{}║", label, " ".repeat(pad));
}

fn print_two_panes(
    _stdout: &mut io::Stdout,
    left_title: &str, left: &[String],
    right_title: &str, right: &[String],
    half: usize, width: usize,
) {
    let max_rows = left.len().max(right.len());
    let lt = format!("  {}", left_title.bold());
    let rt = format!("  {}", right_title.bold());
    let lpad = half.saturating_sub(lt.len() - left_title.len() / 2 + left_title.len() / 2);
    let rpad = (width - half - 3).saturating_sub(rt.len() - right_title.len() / 2 + right_title.len() / 2);
    println!("║{}{:>lp$}║{}{:>rp$}║",
        lt, " ", rt, " ", lp = lpad, rp = rpad);

    for i in 0..max_rows {
        let lc = left.get(i).map(|s| s.as_str()).unwrap_or("");
        let rc = right.get(i).map(|s| s.as_str()).unwrap_or("");
        let lv = strip_ansi_len(lc);
        let rv = strip_ansi_len(rc);
        let lpad = half.saturating_sub(lv + 2);
        let rpad = (width - half - 3).saturating_sub(rv + 2);
        println!("║  {}{:>lp$}║  {}{:>rp$}║",
            lc, " ", rc, " ", lp = lpad, rp = rpad);
    }
}

fn print_agent_row_header(width: usize) {
    let header = format!("║  {:<20} {:<12} {:<9} {}",
        "AGENT".bold(), "TENANT".bold(), "STATUS".bold(), "HEALTH".bold());
    let pad = width.saturating_sub(strip_ansi_len(&header));
    println!("{}{}║", header, " ".repeat(pad));
}

fn format_next_compact(ts: u64) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    if ts <= now { "overdue".into() }
    else {
        let d = ts - now;
        if d < 60 { format!("in {}s", d) }
        else if d < 3600 { format!("in {}m", d / 60) }
        else { format!("in {}h", d / 3600) }
    }
}

fn strip_ansi_len(s: &str) -> usize {
    let mut len = 0;
    let mut in_esc = false;
    for ch in s.chars() {
        if ch == '\x1b' { in_esc = true; continue; }
        if in_esc { if ch == 'm' { in_esc = false; } continue; }
        len += 1;
    }
    len
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02} UTC", h, m, s)
}
