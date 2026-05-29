//! Real-time alerting engine for Apollo v2.2.
//!
//! Alert rules define a metric threshold and a delivery channel. The background
//! evaluation loop (every 30 s) checks each enabled rule against live health
//! and usage data, fires if the threshold is breached, and enforces a cooldown
//! to prevent alert storms.
//!
//! Storage:
//!   `{base_dir}/alerts/rules.json`   — persisted alert rules
//!   `{base_dir}/alerts/history.jsonl` — append-only fire history

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

/// Which metric to evaluate.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertMetric {
    /// Fire when health score for (tenant, agent) drops below threshold.
    HealthScore { tenant_id: String, agent_id: String },
    /// Fire when daily token usage for a tenant exceeds threshold (millions).
    TokenBudget { tenant_id: String },
    /// Fire when crash count in window for (tenant, agent) exceeds threshold.
    CrashCount { tenant_id: String, agent_id: String },
    /// Fire when fleet utilization (active/capacity) exceeds threshold (0–1).
    FleetUtilization,
}

/// Where to send the alert.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AlertChannel {
    /// Slack incoming webhook.
    Slack { webhook_url: String },
    /// PagerDuty Events API v2.
    PagerDuty {
        routing_key: String,
        /// critical | error | warning | info
        #[serde(default = "default_severity")]
        severity: String,
    },
    /// Generic HTTP webhook (reuses Apollo's HMAC-signed webhook format).
    Webhook {
        url: String,
        #[serde(default)]
        secret: Option<String>,
    },
}

fn default_severity() -> String { "warning".to_string() }

/// A persisted alert rule.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AlertRule {
    pub id:             String,
    pub name:           String,
    pub description:    String,
    pub metric:         AlertMetric,
    /// Numeric threshold (meaning depends on metric type).
    pub threshold:      f64,
    pub channel:        AlertChannel,
    /// Minimum seconds between firings for this rule (prevents spam).
    #[serde(default = "default_cooldown")]
    pub cooldown_secs:  u64,
    pub enabled:        bool,
    pub last_fired_at:  Option<u64>,
    pub created_at:     u64,
    pub updated_at:     u64,
}

fn default_cooldown() -> u64 { 300 }

/// One historical alert firing.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AlertFiring {
    pub id:           String,
    pub rule_id:      String,
    pub rule_name:    String,
    pub fired_at:     u64,
    pub metric_value: f64,
    pub message:      String,
    pub channel_type: String,
    pub delivered:    bool,
    pub error:        Option<String>,
}

// ── Storage ───────────────────────────────────────────────────────────────────

fn alerts_dir(base_dir: &Path) -> PathBuf { base_dir.join("alerts") }
fn rules_path(base_dir: &Path) -> PathBuf { alerts_dir(base_dir).join("rules.json") }
fn history_path(base_dir: &Path) -> PathBuf { alerts_dir(base_dir).join("history.jsonl") }

pub fn load_rules(base_dir: &Path) -> Vec<AlertRule> {
    let path = rules_path(base_dir);
    if !path.exists() { return vec![]; }
    fs::read_to_string(&path).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_rules(base_dir: &Path, rules: &[AlertRule]) -> Result<()> {
    fs::create_dir_all(alerts_dir(base_dir))?;
    fs::write(rules_path(base_dir), serde_json::to_string_pretty(rules)?)?;
    Ok(())
}

pub fn create_rule(base_dir: &Path, mut rule: AlertRule) -> Result<AlertRule> {
    let mut rules = load_rules(base_dir);
    if rule.id.is_empty() { rule.id = Uuid::new_v4().to_string(); }
    let now = now_unix();
    if rule.created_at == 0 { rule.created_at = now; }
    rule.updated_at = now;
    rule.enabled = true;
    rules.push(rule.clone());
    save_rules(base_dir, &rules)?;
    Ok(rule)
}

pub fn delete_rule(base_dir: &Path, rule_id: &str) -> Result<()> {
    let mut rules = load_rules(base_dir);
    let before = rules.len();
    rules.retain(|r| r.id != rule_id);
    if rules.len() == before {
        return Err(anyhow::anyhow!("Alert rule '{}' not found", rule_id));
    }
    save_rules(base_dir, &rules)
}

pub fn get_rule(base_dir: &Path, rule_id: &str) -> Option<AlertRule> {
    load_rules(base_dir).into_iter().find(|r| r.id == rule_id)
}

/// Load the last N alert firings from history.
pub fn load_history(base_dir: &Path, limit: usize) -> Vec<AlertFiring> {
    let path = history_path(base_dir);
    if !path.exists() { return vec![]; }
    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut firings: Vec<AlertFiring> = content.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    firings.reverse();
    firings.truncate(limit);
    firings
}

fn append_history(base_dir: &Path, firing: &AlertFiring) {
    let path = history_path(base_dir);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string(firing) {
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(f, "{}", json);
        }
    }
}

// ── Evaluation engine ─────────────────────────────────────────────────────────

/// Evaluate all enabled rules against current state and fire where needed.
/// This is called every 30 s from the background loop in the node daemon.
pub async fn evaluate_rules(base_dir: &Path) {
    let mut rules = load_rules(base_dir);
    let now = now_unix();
    let mut changed = false;

    for rule in rules.iter_mut() {
        if !rule.enabled { continue; }

        // Cooldown check
        if let Some(last) = rule.last_fired_at {
            if now.saturating_sub(last) < rule.cooldown_secs { continue; }
        }

        // Evaluate metric
        let eval = evaluate_metric(base_dir, &rule.metric, rule.threshold);
        if let Some((value, message)) = eval {
            tracing::warn!(
                rule_id = %rule.id,
                rule_name = %rule.name,
                value,
                threshold = rule.threshold,
                "alert rule triggered"
            );
            let delivered = fire_channel(&rule.channel, &rule.name, &message).await;
            let channel_type = channel_type_name(&rule.channel);
            append_history(base_dir, &AlertFiring {
                id:           Uuid::new_v4().to_string(),
                rule_id:      rule.id.clone(),
                rule_name:    rule.name.clone(),
                fired_at:     now,
                metric_value: value,
                message,
                channel_type,
                delivered:    delivered.is_ok(),
                error:        delivered.err().map(|e| e.to_string()),
            });
            rule.last_fired_at = Some(now);
            rule.updated_at = now;
            changed = true;
        }
    }

    if changed {
        let _ = save_rules(base_dir, &rules);
    }
}

fn evaluate_metric(base_dir: &Path, metric: &AlertMetric, threshold: f64) -> Option<(f64, String)> {
    match metric {
        AlertMetric::HealthScore { tenant_id, agent_id } => {
            let rec = crate::health::load_health(base_dir, tenant_id, agent_id)?;
            let score = rec.health_score as f64;
            if score < threshold {
                Some((score, format!(
                    "Health score for {}/{} is {:.0} (threshold: {:.0})",
                    tenant_id, agent_id, score, threshold
                )))
            } else { None }
        }
        AlertMetric::TokenBudget { tenant_id } => {
            let stats = crate::tracing::tenant_token_stats(base_dir, tenant_id);
            let used = (stats.total_input_tokens + stats.total_output_tokens) as f64 / 1_000_000.0;
            if used > threshold {
                Some((used, format!(
                    "Tenant {} used {:.2}M tokens today (limit: {:.2}M)",
                    tenant_id, used, threshold
                )))
            } else { None }
        }
        AlertMetric::CrashCount { tenant_id, agent_id } => {
            let rec = crate::health::load_health(base_dir, tenant_id, agent_id)?;
            let crashes = rec.crash_count as f64;
            if crashes > threshold {
                Some((crashes, format!(
                    "{}/{} had {} crashes (threshold: {})",
                    tenant_id, agent_id, crashes as u64, threshold as u64
                )))
            } else { None }
        }
        AlertMetric::FleetUtilization => {
            let summary = crate::health::fleet_health_summary(base_dir);
            let avg_score = summary.avg_health_score as f64;
            // Utilization = inverse of avg health score (low score = high utilization/stress)
            let util_pct = (100.0 - avg_score) / 100.0;
            if util_pct > threshold {
                Some((util_pct, format!(
                    "Fleet utilization at {:.0}% (avg health {:.0}, threshold: {:.0}%)",
                    util_pct * 100.0, avg_score, threshold * 100.0
                )))
            } else { None }
        }
    }
}

async fn fire_channel(channel: &AlertChannel, rule_name: &str, message: &str) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    match channel {
        AlertChannel::Slack { webhook_url } => {
            let body = serde_json::json!({
                "text": format!(":warning: *Apollo Alert: {}*\n{}", rule_name, message),
                "blocks": [{
                    "type": "section",
                    "text": { "type": "mrkdwn", "text": format!(":warning: *Alert: {}*\n{}", rule_name, message) }
                }]
            });
            client.post(webhook_url).json(&body).send().await
                .context("Slack delivery failed")?
                .error_for_status()
                .context("Slack returned error")?;
        }
        AlertChannel::PagerDuty { routing_key, severity } => {
            let body = serde_json::json!({
                "routing_key": routing_key,
                "event_action": "trigger",
                "payload": {
                    "summary": format!("Apollo Alert: {} — {}", rule_name, message),
                    "severity": severity,
                    "source": "apollo-node",
                    "custom_details": { "rule": rule_name, "message": message }
                }
            });
            client.post("https://events.pagerduty.com/v2/enqueue")
                .json(&body).send().await
                .context("PagerDuty delivery failed")?
                .error_for_status()
                .context("PagerDuty returned error")?;
        }
        AlertChannel::Webhook { url, secret } => {
            let body = serde_json::json!({
                "event": "ALERT_FIRED",
                "rule": rule_name,
                "message": message,
                "timestamp": now_unix(),
            });
            let payload_str = serde_json::to_string(&body)?;
            let mut req = client.post(url)
                .header("Content-Type", "application/json")
                .body(payload_str.clone());
            if let Some(s) = secret {
                use hmac::{Hmac, Mac};
                use sha2::Sha256;
                let mut mac = Hmac::<Sha256>::new_from_slice(s.as_bytes())
                    .unwrap_or_else(|_| Hmac::<Sha256>::new_from_slice(b"").unwrap());
                mac.update(payload_str.as_bytes());
                let sig = hex::encode(mac.finalize().into_bytes());
                req = req.header("X-Apollo-Signature", format!("sha256={}", sig));
            }
            req.send().await.context("Webhook delivery failed")?
                .error_for_status().context("Webhook returned error")?;
        }
    }
    tracing::info!(rule = %rule_name, "alert delivered");
    Ok(())
}

fn channel_type_name(c: &AlertChannel) -> String {
    match c {
        AlertChannel::Slack { .. }      => "slack".to_string(),
        AlertChannel::PagerDuty { .. }  => "pagerduty".to_string(),
        AlertChannel::Webhook { .. }    => "webhook".to_string(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
