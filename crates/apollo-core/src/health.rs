//! Agent Lifecycle Intelligence — health scoring, crash detection,
//! adaptive restart, and memory leak monitoring.
//!
//! Every running agent instance has a `HealthRecord` that is updated by the
//! background health-check loop (runs every 30 s). The health score (0–100)
//! is computed from:
//!   - Crash frequency over a sliding window
//!   - Memory trend (positive slope = potential leak)
//!   - CPU saturation (sustained >95% = degraded)
//!   - Response latency (if the agent exposes a health endpoint)
//!   - Time since last successful check-in
//!
//! Records are stored in `base_dir/health/{tenant_id}/{agent_id}.json`.
//! The REST layer exposes them at GET `/health/{tenant_id}/{agent_id}`.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const SLIDING_WINDOW_SECS: u64 = 300; // 5 minutes
const MAX_HISTORY:          usize = 60; // last 60 samples (30 min at 30-s intervals)

// ── Status enum ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,      // score >= 80
    Degraded,     // 40 <= score < 80
    Critical,     // score < 40
    Dead,         // process not found
    Unknown,      // no data yet
}

impl Default for HealthStatus {
    fn default() -> Self { HealthStatus::Unknown }
}

impl HealthStatus {
    pub fn from_score(score: f32) -> Self {
        if score >= 80.0 { HealthStatus::Healthy }
        else if score >= 40.0 { HealthStatus::Degraded }
        else { HealthStatus::Critical }
    }
}

// ── Crash record ─────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CrashRecord {
    pub timestamp:    u64,
    pub exit_code:    Option<i32>,
    pub signal:       Option<String>,
    pub was_oom:      bool,
    pub restart_attempted: bool,
}

// ── Resource sample ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ResourceSample {
    pub timestamp:   u64,
    pub cpu_pct:     f32,
    pub memory_mb:   u64,
    pub open_files:  u32,
}

// ── Health record ─────────────────────────────────────────────────────────────

/// Per-instance health state, persisted to disk and returned by the API.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct HealthRecord {
    pub tenant_id:        String,
    pub agent_id:         String,
    pub instance_id:      String,

    pub health_score:     f32,         // 0–100
    pub status:           HealthStatus,

    // Crash intelligence
    pub crash_count:      u32,         // lifetime
    pub recent_crashes:   Vec<CrashRecord>,  // within window
    pub restart_count:    u32,
    pub last_crash_ts:    Option<u64>,
    pub crash_pattern:    Option<String>,    // "oom_loop", "startup_crash", "periodic"

    // Resource trends
    pub resource_history: VecDeque<ResourceSample>,
    pub avg_cpu_pct:      f32,
    pub avg_memory_mb:    u64,
    pub memory_trend_mb_per_min: f64, // positive = likely leak
    pub peak_memory_mb:   u64,

    // Latency (optional — from agent health endpoint)
    pub last_latency_ms:  Option<u64>,
    pub avg_latency_ms:   f64,

    // Timing
    pub last_check_ts:    u64,
    pub uptime_secs:      u64,
    pub created_at:       u64,
}

impl HealthRecord {
    pub fn new(tenant_id: &str, agent_id: &str, instance_id: &str) -> Self {
        let now = now_unix();
        HealthRecord {
            tenant_id:        tenant_id.to_string(),
            agent_id:         agent_id.to_string(),
            instance_id:      instance_id.to_string(),
            health_score:     100.0,
            status:           HealthStatus::Unknown,
            crash_count:      0,
            recent_crashes:   vec![],
            restart_count:    0,
            last_crash_ts:    None,
            crash_pattern:    None,
            resource_history: VecDeque::new(),
            avg_cpu_pct:      0.0,
            avg_memory_mb:    0,
            memory_trend_mb_per_min: 0.0,
            peak_memory_mb:   0,
            last_latency_ms:  None,
            avg_latency_ms:   0.0,
            last_check_ts:    now,
            uptime_secs:      0,
            created_at:       now,
        }
    }

    /// Ingest a new resource sample and recompute health score.
    pub fn record_sample(&mut self, cpu_pct: f32, memory_mb: u64, latency_ms: Option<u64>) {
        let now = now_unix();
        let sample = ResourceSample {
            timestamp: now,
            cpu_pct,
            memory_mb,
            open_files: 0,
        };

        self.resource_history.push_back(sample);
        if self.resource_history.len() > MAX_HISTORY {
            self.resource_history.pop_front();
        }

        if memory_mb > self.peak_memory_mb { self.peak_memory_mb = memory_mb; }

        // Recompute averages
        let count = self.resource_history.len() as f64;
        self.avg_cpu_pct = (self.resource_history.iter().map(|s| s.cpu_pct as f64).sum::<f64>()
            / count) as f32;
        self.avg_memory_mb = (self.resource_history.iter().map(|s| s.memory_mb as f64).sum::<f64>()
            / count) as u64;

        // Memory trend via linear regression slope
        self.memory_trend_mb_per_min = compute_memory_trend(&self.resource_history);

        // Latency tracking
        if let Some(lat) = latency_ms {
            self.last_latency_ms = Some(lat);
            self.avg_latency_ms = (self.avg_latency_ms * 0.9) + (lat as f64 * 0.1);
        }

        self.last_check_ts = now;
        self.uptime_secs = now.saturating_sub(self.created_at);

        self.recompute_score();
    }

    /// Record a crash event.
    pub fn record_crash(&mut self, exit_code: Option<i32>, signal: Option<&str>, was_oom: bool) {
        let now = now_unix();
        self.crash_count += 1;
        self.last_crash_ts = Some(now);

        let crash = CrashRecord {
            timestamp: now,
            exit_code,
            signal: signal.map(|s| s.to_string()),
            was_oom,
            restart_attempted: false,
        };
        self.recent_crashes.push(crash);

        // Prune old crashes outside the window
        let cutoff = now.saturating_sub(SLIDING_WINDOW_SECS);
        self.recent_crashes.retain(|c| c.timestamp >= cutoff);

        // Detect crash patterns
        self.crash_pattern = detect_crash_pattern(self);

        self.recompute_score();
    }

    /// Record a restart.
    pub fn record_restart(&mut self) {
        self.restart_count += 1;
        if let Some(last) = self.recent_crashes.last_mut() {
            last.restart_attempted = true;
        }
    }

    /// Recompute the composite health score.
    fn recompute_score(&mut self) {
        let mut score = 100.0f32;

        // Crash penalty: -15 per crash in window, capped at -60
        let window_crashes = self.recent_crashes.len() as f32;
        score -= (window_crashes * 15.0).min(60.0);

        // CPU saturation penalty: -10 if avg > 95%
        if self.avg_cpu_pct > 95.0 {
            score -= 10.0;
        } else if self.avg_cpu_pct > 85.0 {
            score -= 5.0;
        }

        // Memory leak penalty: -20 if growing fast (>10 MB/min)
        if self.memory_trend_mb_per_min > 50.0 {
            score -= 20.0;
        } else if self.memory_trend_mb_per_min > 10.0 {
            score -= 10.0;
        }

        // Latency penalty: -10 if avg latency > 5s
        if self.avg_latency_ms > 5000.0 {
            score -= 10.0;
        } else if self.avg_latency_ms > 2000.0 {
            score -= 5.0;
        }

        self.health_score = score.max(0.0).min(100.0);
        self.status = HealthStatus::from_score(self.health_score);
    }

    /// Determines whether this instance should be auto-restarted.
    pub fn should_restart(&self, max_restarts: u32, window_secs: u64) -> bool {
        if self.status != HealthStatus::Dead && self.status != HealthStatus::Critical {
            return false;
        }
        // Don't restart if we've exceeded the restart budget
        if self.restart_count >= max_restarts {
            return false;
        }
        // Check restart budget within window
        let now = now_unix();
        let cutoff = now.saturating_sub(window_secs);
        let recent_restarts = self.recent_crashes.iter()
            .filter(|c| c.timestamp >= cutoff && c.restart_attempted)
            .count() as u32;

        recent_restarts < max_restarts
    }
}

// ── Crash pattern detection ───────────────────────────────────────────────────

fn detect_crash_pattern(rec: &HealthRecord) -> Option<String> {
    let crashes = &rec.recent_crashes;
    if crashes.len() < 2 { return None; }

    // OOM loop: multiple OOM crashes
    if crashes.iter().filter(|c| c.was_oom).count() >= 2 {
        return Some("oom_loop".to_string());
    }

    // Startup crash: crashed within first 30s of creation multiple times
    let start_crashes = crashes.iter()
        .filter(|c| c.timestamp.saturating_sub(rec.created_at) < 30)
        .count();
    if start_crashes >= 2 {
        return Some("startup_crash".to_string());
    }

    // Periodic crash: check if timestamps are roughly evenly spaced
    if crashes.len() >= 3 {
        let intervals: Vec<u64> = crashes.windows(2)
            .map(|w| w[1].timestamp.saturating_sub(w[0].timestamp))
            .collect();
        let avg = intervals.iter().sum::<u64>() / intervals.len() as u64;
        let variance = intervals.iter()
            .map(|&i| {
                let diff = i as i64 - avg as i64;
                (diff * diff) as u64
            })
            .sum::<u64>() / intervals.len() as u64;
        if avg > 0 && variance < (avg / 3).pow(2) {
            return Some("periodic".to_string());
        }
    }

    Some("repeated".to_string())
}

/// Simple linear regression over memory samples to detect growth trend.
fn compute_memory_trend(history: &VecDeque<ResourceSample>) -> f64 {
    let n = history.len();
    if n < 3 { return 0.0; }

    // x = sample index, y = memory_mb
    let n_f = n as f64;
    let sum_x: f64 = (0..n).map(|i| i as f64).sum();
    let sum_y: f64 = history.iter().map(|s| s.memory_mb as f64).sum();
    let sum_xy: f64 = history.iter().enumerate()
        .map(|(i, s)| i as f64 * s.memory_mb as f64)
        .sum();
    let sum_x2: f64 = (0..n).map(|i| (i * i) as f64).sum();

    let denom = n_f * sum_x2 - sum_x * sum_x;
    if denom == 0.0 { return 0.0; }

    // slope in MB per sample; convert to MB/min (sample every 30s → 2 samples/min)
    let slope = (n_f * sum_xy - sum_x * sum_y) / denom;
    slope * 2.0
}

// ── Storage ───────────────────────────────────────────────────────────────────

fn health_path(base_dir: &Path, tenant_id: &str, agent_id: &str) -> PathBuf {
    base_dir.join("health")
        .join(sanitize(tenant_id))
        .join(format!("{}.json", sanitize(agent_id)))
}

pub fn load_health(base_dir: &Path, tenant_id: &str, agent_id: &str) -> Option<HealthRecord> {
    let path = health_path(base_dir, tenant_id, agent_id);
    if !path.exists() { return None; }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
}

pub fn save_health(base_dir: &Path, record: &HealthRecord) -> Result<()> {
    let path = health_path(base_dir, &record.tenant_id, &record.agent_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create health dir")?;
    }
    fs::write(path, serde_json::to_string_pretty(record)?)?;
    Ok(())
}

pub fn load_or_create_health(
    base_dir: &Path,
    tenant_id: &str,
    agent_id: &str,
    instance_id: &str,
) -> HealthRecord {
    load_health(base_dir, tenant_id, agent_id)
        .unwrap_or_else(|| HealthRecord::new(tenant_id, agent_id, instance_id))
}

/// List all health records for a tenant.
pub fn list_tenant_health(base_dir: &Path, tenant_id: &str) -> Vec<HealthRecord> {
    let dir = base_dir.join("health").join(sanitize(tenant_id));
    if !dir.exists() { return vec![]; }
    fs::read_dir(&dir)
        .ok()
        .map(|entries| {
            entries.flatten()
                .filter_map(|e| {
                    let raw = fs::read_to_string(e.path()).ok()?;
                    serde_json::from_str(&raw).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Fleet-level health summary.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct FleetHealthSummary {
    pub total_instances:   usize,
    pub healthy:           usize,
    pub degraded:          usize,
    pub critical:          usize,
    pub dead:              usize,
    pub instances_needing_restart: usize,
    pub avg_health_score:  f32,
    pub generated_at:      u64,
}

pub fn fleet_health_summary(base_dir: &Path) -> FleetHealthSummary {
    let health_dir = base_dir.join("health");
    let mut all_records: Vec<HealthRecord> = vec![];

    if let Ok(tenants) = fs::read_dir(&health_dir) {
        for tenant_entry in tenants.flatten() {
            if !tenant_entry.path().is_dir() { continue; }
            if let Ok(agents) = fs::read_dir(tenant_entry.path()) {
                for agent_entry in agents.flatten() {
                    if let Ok(raw) = fs::read_to_string(agent_entry.path()) {
                        if let Ok(rec) = serde_json::from_str::<HealthRecord>(&raw) {
                            all_records.push(rec);
                        }
                    }
                }
            }
        }
    }

    let total = all_records.len();
    if total == 0 {
        return FleetHealthSummary { generated_at: now_unix(), ..Default::default() };
    }

    let healthy  = all_records.iter().filter(|r| r.status == HealthStatus::Healthy).count();
    let degraded = all_records.iter().filter(|r| r.status == HealthStatus::Degraded).count();
    let critical = all_records.iter().filter(|r| r.status == HealthStatus::Critical).count();
    let dead     = all_records.iter().filter(|r| r.status == HealthStatus::Dead).count();
    let avg_score = all_records.iter().map(|r| r.health_score as f64).sum::<f64>() / total as f64;

    let needing_restart = all_records.iter()
        .filter(|r| r.should_restart(3, 60))
        .count();

    FleetHealthSummary {
        total_instances: total,
        healthy,
        degraded,
        critical,
        dead,
        instances_needing_restart: needing_restart,
        avg_health_score: avg_score as f32,
        generated_at: now_unix(),
    }
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
