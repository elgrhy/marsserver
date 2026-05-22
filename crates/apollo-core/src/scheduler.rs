//! Scheduled & Event-Driven Agent Execution.
//!
//! Enables three launch modes beyond the manual REST call:
//!
//! 1. **Cron** — standard 5-field cron expressions (min, hour, dom, mon, dow)
//! 2. **Interval** — run every N seconds
//! 3. **Once** — run exactly once at a given Unix timestamp
//!
//! The scheduler loop runs inside the node daemon and re-evaluates the job
//! list every 30 s. When a job fires it issues an internal `agent.run()` call.
//!
//! Jobs are persisted in `base_dir/scheduler/jobs.json`.
//! Run history (last 100 runs per job) lives in `base_dir/scheduler/history/{job_id}.jsonl`.
//!
//! REST API:
//!   POST   /schedule             — create job
//!   GET    /schedule             — list all jobs
//!   GET    /schedule/{id}        — get job
//!   PUT    /schedule/{id}        — update job
//!   DELETE /schedule/{id}        — delete job
//!   POST   /schedule/{id}/run    — manual trigger
//!   GET    /schedule/{id}/history — run history

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ── Schedule definition ───────────────────────────────────────────────────────

/// Schedule types supported by the Apollo scheduler.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// Standard 5-field cron: "0 * * * *" = every hour
    Cron { expression: String },
    /// Run every `secs` seconds.
    Interval { secs: u64 },
    /// Run once at Unix timestamp `at`.
    Once { at: u64 },
}

impl Schedule {
    /// Compute the next fire time after `after_ts` (Unix seconds).
    pub fn next_after(&self, after_ts: u64) -> Option<u64> {
        match self {
            Schedule::Once { at } => {
                if *at > after_ts { Some(*at) } else { None }
            }
            Schedule::Interval { secs } => Some(after_ts + secs),
            Schedule::Cron { expression } => next_cron(expression, after_ts),
        }
    }

    /// True if this schedule can ever fire again after `now`.
    pub fn has_future(&self, now: u64) -> bool {
        self.next_after(now).is_some()
    }
}

// ── Job record ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ScheduledJob {
    pub id:         String,
    pub name:       String,
    pub tenant_id:  String,
    pub agent_id:   String,
    /// Optional extra env vars injected when this job fires.
    #[serde(default)]
    pub extra_env:  HashMap<String, String>,

    pub schedule:   Schedule,
    pub enabled:    bool,

    pub created_at: u64,
    pub updated_at: u64,
    pub last_run:   Option<u64>,
    pub next_run:   u64,
    pub run_count:  u64,
    pub fail_count: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JobRunRecord {
    pub job_id:     String,
    pub fired_at:   u64,
    pub status:     String,   // "ok" | "error" | "skipped"
    pub message:    Option<String>,
    pub duration_ms: Option<u64>,
}

// ── Storage ───────────────────────────────────────────────────────────────────

fn jobs_path(base_dir: &Path) -> PathBuf {
    base_dir.join("scheduler").join("jobs.json")
}

fn history_path(base_dir: &Path, job_id: &str) -> PathBuf {
    base_dir.join("scheduler").join("history").join(format!("{}.jsonl", sanitize(job_id)))
}

pub fn load_jobs(base_dir: &Path) -> Vec<ScheduledJob> {
    let path = jobs_path(base_dir);
    if !path.exists() { return vec![]; }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_jobs(base_dir: &Path, jobs: &[ScheduledJob]) -> Result<()> {
    let path = jobs_path(base_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create scheduler dir")?;
    }
    fs::write(path, serde_json::to_string_pretty(jobs)?)?;
    Ok(())
}

pub fn load_job(base_dir: &Path, job_id: &str) -> Option<ScheduledJob> {
    load_jobs(base_dir).into_iter().find(|j| j.id == job_id)
}

/// Create a new scheduled job.
pub fn create_job(base_dir: &Path, mut job: ScheduledJob) -> Result<ScheduledJob> {
    if job.id.is_empty() { job.id = Uuid::new_v4().to_string(); }
    let now = now_unix();
    job.created_at = now;
    job.updated_at = now;
    job.next_run   = job.schedule.next_after(now).unwrap_or(now + 60);

    let mut jobs = load_jobs(base_dir);
    jobs.push(job.clone());
    save_jobs(base_dir, &jobs)?;
    Ok(job)
}

/// Update an existing job.
pub fn update_job(base_dir: &Path, updated: ScheduledJob) -> Result<ScheduledJob> {
    let mut jobs = load_jobs(base_dir);
    let pos = jobs.iter().position(|j| j.id == updated.id)
        .ok_or_else(|| anyhow!("Job '{}' not found", updated.id))?;
    let mut j = updated;
    j.updated_at = now_unix();
    j.next_run   = j.schedule.next_after(now_unix()).unwrap_or(j.next_run);
    jobs[pos] = j.clone();
    save_jobs(base_dir, &jobs)?;
    Ok(j)
}

/// Delete a job by ID.
pub fn delete_job(base_dir: &Path, job_id: &str) -> Result<()> {
    let mut jobs = load_jobs(base_dir);
    let before = jobs.len();
    jobs.retain(|j| j.id != job_id);
    if jobs.len() == before {
        return Err(anyhow!("Job '{}' not found", job_id));
    }
    save_jobs(base_dir, &jobs)?;
    // Clean up history
    let hist = history_path(base_dir, job_id);
    if hist.exists() { let _ = fs::remove_file(hist); }
    Ok(())
}

/// Mark a job as having just fired (updates last_run, next_run, run_count).
pub fn mark_fired(base_dir: &Path, job_id: &str, success: bool, msg: Option<String>) -> Result<()> {
    let now = now_unix();
    let mut jobs = load_jobs(base_dir);
    let pos = jobs.iter().position(|j| j.id == job_id);

    if let Some(pos) = pos {
        let j = &mut jobs[pos];
        j.last_run  = Some(now);
        j.run_count += 1;
        if !success { j.fail_count += 1; }
        j.next_run  = j.schedule.next_after(now).unwrap_or(now + 86400);
        // Disable once-jobs after they fire
        if matches!(j.schedule, Schedule::Once { .. }) {
            j.enabled = false;
        }
        save_jobs(base_dir, &jobs)?;
    }

    // Append to history
    let run = JobRunRecord {
        job_id:      job_id.to_string(),
        fired_at:    now,
        status:      if success { "ok".to_string() } else { "error".to_string() },
        message:     msg,
        duration_ms: None,
    };
    let hist_path = history_path(base_dir, job_id);
    if let Some(parent) = hist_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&hist_path)?;
    writeln!(f, "{}", serde_json::to_string(&run)?)?;

    Ok(())
}

/// Load the last N run records for a job.
pub fn load_history(base_dir: &Path, job_id: &str, limit: usize) -> Vec<JobRunRecord> {
    let path = history_path(base_dir, job_id);
    if !path.exists() { return vec![]; }
    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut records: Vec<JobRunRecord> = content.lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    let len = records.len();
    if len > limit { records = records[len - limit..].to_vec(); }
    records.reverse(); // newest first
    records
}

/// Return all jobs that are due to fire at or before `now`.
pub fn due_jobs(base_dir: &Path, now: u64) -> Vec<ScheduledJob> {
    load_jobs(base_dir).into_iter()
        .filter(|j| j.enabled && j.next_run <= now)
        .collect()
}

// ── Simple cron parser ────────────────────────────────────────────────────────
//
// Supports the 5-field POSIX cron syntax:
//   minute  (0–59)
//   hour    (0–23)
//   dom     (1–31)
//   month   (1–12)
//   dow     (0–6, 0=Sunday)
//
// Supports: `*`, `*/n` (step), `n` (literal), `n-m` (range), `n,m,…` (list).

fn next_cron(expr: &str, after_ts: u64) -> Option<u64> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 { return None; }

    // Parse each field into an allowed-values set
    let minutes = parse_field(fields[0], 0, 59)?;
    let hours   = parse_field(fields[1], 0, 23)?;
    let doms    = parse_field(fields[2], 1, 31)?;
    let months  = parse_field(fields[3], 1, 12)?;
    let dows    = parse_field(fields[4], 0, 6)?;

    // Start searching from the next minute after `after_ts`
    let start = after_ts + 60;

    // Iterate up to 4 years (safety cap)
    let limit = start + 4 * 365 * 86400;
    let mut ts = start - (start % 60); // align to minute

    while ts < limit {
        let t = unix_to_parts(ts);
        if !months.contains(&t.month)  { ts = advance_to_month(ts, &months)?; continue; }
        if !doms.contains(&t.dom)       { ts += 86400; ts -= ts % 86400; continue; }
        if !dows.contains(&t.dow)       { ts += 86400; ts -= ts % 86400; continue; }
        if !hours.contains(&t.hour)     { ts += 3600;  ts -= ts % 3600;  continue; }
        if !minutes.contains(&t.minute) { ts += 60;    continue; }
        return Some(ts);
    }
    None
}

fn parse_field(field: &str, min: u32, max: u32) -> Option<Vec<u32>> {
    let mut result = Vec::new();
    for part in field.split(',') {
        if part == "*" {
            result.extend(min..=max);
        } else if let Some(rest) = part.strip_prefix("*/") {
            let step: u32 = rest.parse().ok()?;
            result.extend((min..=max).filter(|v| (v - min) % step == 0));
        } else if part.contains('-') {
            let sides: Vec<&str> = part.splitn(2, '-').collect();
            let lo: u32 = sides[0].parse().ok()?;
            let hi: u32 = sides[1].parse().ok()?;
            result.extend(lo..=hi);
        } else {
            let v: u32 = part.parse().ok()?;
            result.push(v);
        }
    }
    result.retain(|&v| v >= min && v <= max);
    result.sort_unstable();
    result.dedup();
    if result.is_empty() { None } else { Some(result) }
}

struct TimeParts { minute: u32, hour: u32, dom: u32, month: u32, dow: u32 }

fn unix_to_parts(ts: u64) -> TimeParts {
    // Simple calendar arithmetic (UTC)
    let secs = ts;
    let minute = ((secs / 60) % 60) as u32;
    let hour   = ((secs / 3600) % 24) as u32;
    let days   = secs / 86400;
    let dow    = ((days + 4) % 7) as u32; // 1970-01-01 was Thursday (=4)

    // Gregorian calendar decomposition
    let (year, month, dom) = days_to_ymd(days);
    let _ = year;
    TimeParts { minute, hour, dom, month, dow }
}

fn days_to_ymd(mut days: u64) -> (u32, u32, u32) {
    let mut year = 1970u32;
    loop {
        let dy = if is_leap(year) { 366 } else { 365 };
        if days < dy { break; }
        days -= dy;
        year += 1;
    }
    let months = [31, if is_leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u32;
    for dm in &months {
        if days < *dm { break; }
        days -= dm;
        month += 1;
    }
    (year, month, days as u32 + 1)
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn advance_to_month(ts: u64, months: &[u32]) -> Option<u64> {
    // Advance to the 1st of the next allowed month
    let t = unix_to_parts(ts);
    let next_month = months.iter().find(|&&m| m > t.month).copied()
        .unwrap_or_else(|| months[0]); // wrap to next year
    // Approximate: add ~28-31 days until we hit the right month
    let mut candidate = ts + 86400;
    for _ in 0..400 {
        let ct = unix_to_parts(candidate);
        if ct.month == next_month { return Some(candidate - (candidate % 86400)); }
        candidate += 86400;
    }
    None
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
