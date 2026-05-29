//! Agent-to-agent messaging bus for Apollo v2.2.
//!
//! Provides topic-based pub/sub message passing between running agents.
//! Messages are persisted to JSONL files so they survive restarts.
//! Consumers poll with a sequence number to receive only new messages.
//!
//! Storage:
//!   `{base_dir}/messages/{topic}/messages.jsonl` — append-only message log
//!   `{base_dir}/messages/{topic}/meta.json`      — sequence counter + stats

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

// ── Types ─────────────────────────────────────────────────────────────────────

/// A single message on the bus.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BusMessage {
    /// Unique message ID.
    pub id:            String,
    /// Monotonically increasing sequence number within this topic.
    pub seq:           u64,
    pub topic:         String,
    pub sender_tenant: String,
    pub sender_agent:  String,
    /// Arbitrary JSON payload.
    pub payload:       serde_json::Value,
    pub timestamp:     u64,
    /// Seconds until message expires (None = never).
    pub ttl_secs:      Option<u64>,
}

impl BusMessage {
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_secs {
            now_unix() >= self.timestamp + ttl
        } else {
            false
        }
    }
}

/// Metadata persisted per topic.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TopicMeta {
    pub last_seq:      u64,
    pub total_messages: u64,
    pub created_at:    u64,
    pub last_message_at: Option<u64>,
}

/// Stats returned by the list-topics endpoint.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TopicStats {
    pub topic:          String,
    pub last_seq:       u64,
    pub total_messages: u64,
    pub created_at:     u64,
    pub last_message_at: Option<u64>,
}

// ── Storage paths ─────────────────────────────────────────────────────────────

fn messages_root(base_dir: &Path) -> PathBuf {
    base_dir.join("messages")
}

fn topic_dir(base_dir: &Path, topic: &str) -> PathBuf {
    messages_root(base_dir).join(sanitize(topic))
}

fn messages_path(base_dir: &Path, topic: &str) -> PathBuf {
    topic_dir(base_dir, topic).join("messages.jsonl")
}

fn meta_path(base_dir: &Path, topic: &str) -> PathBuf {
    topic_dir(base_dir, topic).join("meta.json")
}

// ── Meta helpers ──────────────────────────────────────────────────────────────

fn load_meta(base_dir: &Path, topic: &str) -> TopicMeta {
    let path = meta_path(base_dir, topic);
    if !path.exists() { return TopicMeta::default(); }
    fs::read_to_string(&path).ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_meta(base_dir: &Path, topic: &str, meta: &TopicMeta) {
    let path = meta_path(base_dir, topic);
    if let Some(p) = path.parent() { let _ = fs::create_dir_all(p); }
    let _ = fs::write(&path, serde_json::to_string_pretty(meta).unwrap_or_default());
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Publish a message to a topic. Returns the stored message with assigned seq.
pub fn publish(
    base_dir:      &Path,
    topic:         &str,
    sender_tenant: &str,
    sender_agent:  &str,
    payload:       serde_json::Value,
    ttl_secs:      Option<u64>,
) -> Result<BusMessage> {
    let dir = topic_dir(base_dir, topic);
    fs::create_dir_all(&dir).context("create topic dir")?;

    let mut meta = load_meta(base_dir, topic);
    if meta.created_at == 0 { meta.created_at = now_unix(); }
    meta.last_seq += 1;
    meta.total_messages += 1;
    meta.last_message_at = Some(now_unix());

    let msg = BusMessage {
        id:            Uuid::new_v4().to_string(),
        seq:           meta.last_seq,
        topic:         topic.to_string(),
        sender_tenant: sender_tenant.to_string(),
        sender_agent:  sender_agent.to_string(),
        payload,
        timestamp:     now_unix(),
        ttl_secs,
    };

    // Append to JSONL
    let mut file = fs::OpenOptions::new()
        .create(true).append(true)
        .open(messages_path(base_dir, topic))
        .context("open messages file")?;
    writeln!(file, "{}", serde_json::to_string(&msg)?)?;

    save_meta(base_dir, topic, &meta);
    tracing::debug!(topic, seq = meta.last_seq, "message published");
    Ok(msg)
}

/// Poll messages from a topic with seq > since_seq (exclusive).
pub fn poll(
    base_dir:  &Path,
    topic:     &str,
    since_seq: u64,
    limit:     usize,
) -> Vec<BusMessage> {
    let path = messages_path(base_dir, topic);
    if !path.exists() { return vec![]; }

    let content = fs::read_to_string(&path).unwrap_or_default();
    let limit = limit.min(1000);

    content.lines()
        .filter_map(|line| serde_json::from_str::<BusMessage>(line).ok())
        .filter(|m| m.seq > since_seq && !m.is_expired())
        .take(limit)
        .collect()
}

/// Clear all messages from a topic.
pub fn clear_topic(base_dir: &Path, topic: &str) -> Result<()> {
    let dir = topic_dir(base_dir, topic);
    if dir.exists() {
        fs::remove_dir_all(&dir).context("remove topic dir")?;
    }
    Ok(())
}

/// List all known topics with their stats.
pub fn list_topics(base_dir: &Path) -> Vec<TopicStats> {
    let root = messages_root(base_dir);
    if !root.exists() { return vec![]; }
    let entries = fs::read_dir(&root).ok();
    let Some(entries) = entries else { return vec![]; };
    entries.flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let topic = e.file_name().to_string_lossy().to_string();
            let meta = load_meta(base_dir, &topic);
            Some(TopicStats {
                topic,
                last_seq:       meta.last_seq,
                total_messages: meta.total_messages,
                created_at:     meta.created_at,
                last_message_at: meta.last_message_at,
            })
        })
        .collect()
}

/// Return the latest N messages from a topic (regardless of seq).
pub fn peek_latest(base_dir: &Path, topic: &str, limit: usize) -> Vec<BusMessage> {
    let path = messages_path(base_dir, topic);
    if !path.exists() { return vec![]; }
    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut msgs: VecDeque<BusMessage> = VecDeque::new();
    for line in content.lines() {
        if let Ok(m) = serde_json::from_str::<BusMessage>(line) {
            if !m.is_expired() {
                if msgs.len() >= limit { msgs.pop_front(); }
                msgs.push_back(m);
            }
        }
    }
    msgs.into_iter().collect()
}

/// Compact a topic's JSONL file by removing expired messages.
pub fn compact_topic(base_dir: &Path, topic: &str) -> Result<usize> {
    let path = messages_path(base_dir, topic);
    if !path.exists() { return Ok(0); }
    let content = fs::read_to_string(&path)?;
    let live: Vec<BusMessage> = content.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|m: &BusMessage| !m.is_expired())
        .collect();
    let removed = content.lines().count().saturating_sub(live.len());
    let mut file = fs::OpenOptions::new()
        .write(true).truncate(true)
        .open(&path)?;
    for msg in &live {
        writeln!(file, "{}", serde_json::to_string(msg)?)?;
    }
    Ok(removed)
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

fn now_unix() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}
