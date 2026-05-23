//! Terminal formatting primitives for the Apollo CLI.
//!
//! Provides colored output, aligned tables, health bars, section separators,
//! and bordered boxes used across all command handlers and the dashboard.

use colored::Colorize;

// ── Colored message prefixes ──────────────────────────────────────────────────

pub fn ok(msg: &str)   { println!("{} {}", "[OK]".green().bold(),   msg); }
pub fn err(msg: &str)  { println!("{} {}", "[ERR]".red().bold(),    msg); }
pub fn warn(msg: &str) { println!("{} {}", "[WARN]".yellow().bold(), msg); }
pub fn info(msg: &str) { println!("{} {}", "[INFO]".cyan().bold(),   msg); }
pub fn dim(msg: &str)  { println!("{}", msg.dimmed()); }

/// Print a bold section separator:  ── Title ───────────────────────────
pub fn section(title: &str) {
    let line = format!("── {} ", title);
    let pad  = "─".repeat(60usize.saturating_sub(line.len()));
    println!("\n{}{}", line.cyan().bold(), pad.dimmed());
}

/// Print a key/value pair with aligned colon.
pub fn kv(key: &str, val: &str) {
    println!("  {:<24} {}", format!("{}:", key).bold(), val);
}

// ── ASCII health bar ──────────────────────────────────────────────────────────

/// Returns a coloured health bar like `████████░░ 82`.
pub fn health_bar(score: u32) -> String {
    let filled = (score / 10) as usize;
    let empty  = 10usize.saturating_sub(filled);
    let bar    = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    let score_str = format!(" {}", score);
    match score {
        80..=100 => format!("{}{}", bar.green(),   score_str.green()),
        40..=79  => format!("{}{}", bar.yellow(),  score_str.yellow()),
        _        => format!("{}{}", bar.red(),      score_str.red()),
    }
}

/// Return a colored status string.
pub fn status_str(s: &str) -> String {
    match s {
        "running"  | "Healthy"   => s.green().to_string(),
        "stopped"  | "Dead"      => s.red().to_string(),
        "Degraded" | "degraded"  => s.yellow().to_string(),
        "Critical" | "critical"  => s.red().bold().to_string(),
        _                        => s.dimmed().to_string(),
    }
}

// ── Aligned table ─────────────────────────────────────────────────────────────

/// Print a table with a header row and data rows, auto-sizing columns.
///
/// ```
/// print_table(&["NAME", "STATUS"], &[vec!["openclaw".into(), "running".into()]]);
/// ```
pub fn print_table(headers: &[&str], rows: &[Vec<String>]) {
    if headers.is_empty() { return; }

    // Compute column widths
    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                // strip ANSI escape codes before measuring length
                widths[i] = widths[i].max(strip_ansi(cell).len());
            }
        }
    }

    // Header
    let header_line: String = headers.iter().enumerate()
        .map(|(i, h)| format!("{:<width$}", h, width = widths[i] + 2))
        .collect::<Vec<_>>()
        .join("");
    println!("{}", header_line.bold().underline());

    // Rows
    for row in rows {
        let line: String = row.iter().enumerate()
            .map(|(i, cell)| {
                let w = widths.get(i).copied().unwrap_or(10) + 2;
                let visible_len = strip_ansi(cell).len();
                let pad = w.saturating_sub(visible_len);
                format!("{}{}", cell, " ".repeat(pad))
            })
            .collect::<Vec<_>>()
            .join("");
        println!("{}", line);
    }
}

/// Strip ANSI escape sequences to get the visible length of a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::new();
    let mut in_escape = false;
    for ch in s.chars() {
        if ch == '\x1b' { in_escape = true; continue; }
        if in_escape {
            if ch == 'm' { in_escape = false; }
            continue;
        }
        out.push(ch);
    }
    out
}

// ── Bordered box ──────────────────────────────────────────────────────────────

/// Print a bordered box suitable for dashboard panes.
///
/// ```
/// box_print("FLEET", &["Active: 14 / 200", "Region: us-east-1"]);
/// ```
pub fn box_print(title: &str, lines: &[String], width: usize) {
    let inner = width - 2;
    println!("╔{}╗", "═".repeat(inner));
    let title_padded = format!("  {}  ", title.bold());
    let title_vis    = format!("  {}  ", title);
    let pad = inner.saturating_sub(title_vis.len());
    println!("║{}{}║", title_padded, " ".repeat(pad));
    println!("╠{}╣", "═".repeat(inner));
    for line in lines {
        let vis_len = strip_ansi(line).len();
        let pad = inner.saturating_sub(vis_len + 1);
        println!("║ {}{} ║", line, " ".repeat(pad.saturating_sub(1)));
    }
    println!("╚{}╝", "═".repeat(inner));
}

// ── Demo / guide helpers ──────────────────────────────────────────────────────

/// Print an Apollo feature banner for demo sections.
pub fn demo_banner(module: &str, tagline: &str) {
    let sep = "═".repeat(62);
    println!("\n{}", sep.cyan());
    println!("  {}  ─  {}", module.cyan().bold(), tagline.white());
    println!("{}", sep.cyan());
}

/// Print a simulated CLI command in demo mode.
pub fn demo_cmd(cmd: &str) {
    println!("\n  {}{}", "$ ".dimmed(), cmd.white().bold());
    println!();
}

/// Print a simulated output line in demo mode.
pub fn demo_out(line: &str) {
    println!("  {}", line);
}

/// Pause and wait for Enter key in demo mode.
pub fn demo_pause() {
    use std::io::{self, Write};
    print!("\n  {} ", "[ press Enter to continue ]".dimmed());
    io::stdout().flush().ok();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).ok();
}

/// Print a guide topic heading.
pub fn guide_heading(title: &str) {
    println!("\n{}", format!("  {}", title).cyan().bold().underline());
    println!();
}

/// Print a guide code block.
pub fn guide_code(code: &str) {
    for line in code.lines() {
        println!("    {}", line.white());
    }
}

/// Print a guide paragraph.
pub fn guide_para(text: &str) {
    for line in textwrap(text, 72) {
        println!("  {}", line);
    }
    println!();
}

fn textwrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if current.len() + word.len() + 1 > width {
            lines.push(current.clone());
            current = word.to_string();
        } else {
            if !current.is_empty() { current.push(' '); }
            current.push_str(word);
        }
    }
    if !current.is_empty() { lines.push(current); }
    lines
}

// ── Journey-first CLI formatting ──────────────────────────────────────────────

/// Numbered wizard step: "  [1/3] Configure the node"
pub fn step(n: usize, total: usize, msg: &str) {
    println!("\n  {} {}", format!("[{}/{}]", n, total).cyan().bold(), msg.bold());
}

/// Successful sub-step: "    ✓ Data directory created"
pub fn substep_ok(msg: &str) {
    println!("    {} {}", "✓".green().bold(), msg);
}

/// Informational sub-step: "    · Checking port 8080..."
pub fn substep_info(msg: &str) {
    println!("    {} {}", "·".dimmed(), msg.dimmed());
}

/// Boxed success summary with aligned key/value pairs.
///
/// ```
/// success_box("Agent added!", &[("Name", "openclaw"), ("Version", "1.0.0")]);
/// ```
pub fn success_box(title: &str, items: &[(&str, &str)]) {
    let w = 54usize;
    println!("\n  {} {}", "✓".green().bold(), title.green().bold());
    println!("  {}", "─".repeat(w).dimmed());
    for (k, v) in items {
        println!("  {:<22} {}", format!("{}:", k).dimmed(), v);
    }
    println!();
}

/// Numbered next-steps block.
pub fn next_steps(steps: &[&str]) {
    println!("  {}", "Next steps:".bold());
    for (i, s) in steps.iter().enumerate() {
        println!("  {}  {}", format!("{}", i + 1).cyan().bold(), s);
    }
    println!();
}

/// Single ℹ hint line.
pub fn hint(msg: &str) {
    println!("  {} {}", "ℹ".cyan(), msg.dimmed());
}

/// Inline command suggestion.
pub fn suggest(cmd: &str) {
    println!("    {} {}", "$".dimmed(), cmd.white().bold());
}

/// Warning hint line.
pub fn warn_hint(msg: &str) {
    println!("  {} {}", "⚠".yellow(), msg.yellow());
}

/// Thin separator line.
pub fn separator() {
    println!("  {}", "─".repeat(56).dimmed());
}

// ── Apollo banner ─────────────────────────────────────────────────────────────

pub fn apollo_banner() {
    println!("{}", r#"
   ___   ___  ____  __    __    ____
  / _ | / _ \/ __ \/ /   / /   / __ \
 / __ |/ ___/ /_/ / /___/ /___/ /_/ /
/_/ |_/_/   \____/_____/_____/\____/
"#.cyan().bold());
}
