//! Distributed Agent Memory Layer — per-tenant per-agent key-value store
//! with full-text similarity search.
//!
//! Apollo agents can persist structured memories across sessions. Each entry
//! is a JSON value stored with optional tags and a TTL. The similarity search
//! uses a lightweight TF-IDF–style token overlap so no ML runtime is required.
//!
//! Storage layout:
//!   `base_dir/memory/{tenant_id}/{agent_id}/store.json`
//!
//! REST API (implemented in the node):
//!   PUT    /memory/{tenant_id}/{agent_id}/{key}       — store/update
//!   GET    /memory/{tenant_id}/{agent_id}/{key}       — retrieve
//!   DELETE /memory/{tenant_id}/{agent_id}/{key}       — delete
//!   POST   /memory/{tenant_id}/{agent_id}/search      — similarity search
//!   GET    /memory/{tenant_id}/{agent_id}             — list all keys
//!   DELETE /memory/{tenant_id}/{agent_id}             — clear all memory

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryEntry {
    /// Unique key within the tenant+agent namespace.
    pub key:        String,
    /// Arbitrary JSON value (string, object, array, number, …).
    pub value:      serde_json::Value,
    /// Optional tags for filtering searches.
    pub tags:       Vec<String>,
    /// Optional text representation used for similarity search.
    pub text:       Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    /// Seconds until expiry (None = never expires).
    pub ttl_secs:   Option<u64>,
}

impl MemoryEntry {
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_secs {
            now_unix() >= self.created_at + ttl
        } else {
            false
        }
    }

    /// Produce a searchable text blob (key + tags + text + JSON scalar leaves).
    pub fn searchable_text(&self) -> String {
        let mut parts = vec![self.key.clone()];
        parts.extend(self.tags.clone());
        if let Some(ref t) = self.text {
            parts.push(t.clone());
        }
        // Add top-level string values from the JSON object
        if let serde_json::Value::Object(ref map) = self.value {
            for v in map.values() {
                if let serde_json::Value::String(s) = v {
                    parts.push(s.clone());
                }
            }
        } else if let serde_json::Value::String(ref s) = self.value {
            parts.push(s.clone());
        }
        parts.join(" ").to_lowercase()
    }
}

/// A memory store for one tenant+agent pair.
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MemoryStore {
    pub entries: HashMap<String, MemoryEntry>,
}

impl MemoryStore {
    /// Prune expired entries in-place.
    pub fn prune_expired(&mut self) {
        self.entries.retain(|_, v| !v.is_expired());
    }

    /// Number of live (non-expired) entries.
    pub fn live_count(&self) -> usize {
        self.entries.values().filter(|e| !e.is_expired()).count()
    }
}

/// Query for similarity search.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryQuery {
    /// Free-text query string.
    pub query: String,
    /// Require all of these tags to be present.
    #[serde(default)]
    pub tags:  Vec<String>,
    /// Maximum results to return.
    #[serde(default = "default_limit")]
    pub limit: usize,
    /// Minimum similarity score (0.0–1.0).
    #[serde(default)]
    pub min_score: f32,
}

fn default_limit() -> usize { 10 }

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemorySearchResult {
    pub key:    String,
    pub entry:  MemoryEntry,
    pub score:  f32,   // 0.0–1.0
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn store_path(base_dir: &Path, tenant_id: &str, agent_id: &str) -> PathBuf {
    base_dir.join("memory")
        .join(sanitize(tenant_id))
        .join(sanitize(agent_id))
        .join("store.json")
}

pub fn load_store(base_dir: &Path, tenant_id: &str, agent_id: &str) -> MemoryStore {
    let path = store_path(base_dir, tenant_id, agent_id);
    if !path.exists() { return MemoryStore::default(); }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_store(base_dir: &Path, tenant_id: &str, agent_id: &str, store: &MemoryStore) -> Result<()> {
    let path = store_path(base_dir, tenant_id, agent_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create memory dir")?;
    }
    fs::write(path, serde_json::to_string_pretty(store)?)?;
    Ok(())
}

// ── Public CRUD API ───────────────────────────────────────────────────────────

/// Store or update a memory entry.
pub fn put_memory(
    base_dir:  &Path,
    tenant_id: &str,
    agent_id:  &str,
    key:       &str,
    value:     serde_json::Value,
    tags:      Vec<String>,
    text:      Option<String>,
    ttl_secs:  Option<u64>,
) -> Result<MemoryEntry> {
    let mut store = load_store(base_dir, tenant_id, agent_id);
    store.prune_expired();

    let now = now_unix();
    let entry = MemoryEntry {
        key:        key.to_string(),
        value,
        tags,
        text,
        created_at: store.entries.get(key).map(|e| e.created_at).unwrap_or(now),
        updated_at: now,
        ttl_secs,
    };

    store.entries.insert(key.to_string(), entry.clone());
    save_store(base_dir, tenant_id, agent_id, &store)?;
    Ok(entry)
}

/// Retrieve a single memory entry by key.
pub fn get_memory(
    base_dir:  &Path,
    tenant_id: &str,
    agent_id:  &str,
    key:       &str,
) -> Option<MemoryEntry> {
    let store = load_store(base_dir, tenant_id, agent_id);
    store.entries.get(key)
        .filter(|e| !e.is_expired())
        .cloned()
}

/// Delete a memory entry.
pub fn delete_memory(
    base_dir:  &Path,
    tenant_id: &str,
    agent_id:  &str,
    key:       &str,
) -> Result<bool> {
    let mut store = load_store(base_dir, tenant_id, agent_id);
    let removed = store.entries.remove(key).is_some();
    if removed {
        save_store(base_dir, tenant_id, agent_id, &store)?;
    }
    Ok(removed)
}

/// Clear all memory for a tenant+agent.
pub fn clear_memory(base_dir: &Path, tenant_id: &str, agent_id: &str) -> Result<()> {
    let path = store_path(base_dir, tenant_id, agent_id);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// List all live memory keys.
pub fn list_memory_keys(
    base_dir:  &Path,
    tenant_id: &str,
    agent_id:  &str,
) -> Vec<String> {
    let mut store = load_store(base_dir, tenant_id, agent_id);
    store.prune_expired();
    store.entries.keys().cloned().collect()
}

// ── Similarity search ─────────────────────────────────────────────────────────

/// Search memory entries using TF-IDF–style token overlap.
pub fn search_memory(
    base_dir:  &Path,
    tenant_id: &str,
    agent_id:  &str,
    query:     &MemoryQuery,
) -> Vec<MemorySearchResult> {
    let mut store = load_store(base_dir, tenant_id, agent_id);
    store.prune_expired();

    let query_tokens = tokenize(&query.query);
    if query_tokens.is_empty() {
        // Return all (filtered by tags) if no query text
        let mut results: Vec<MemorySearchResult> = store.entries.values()
            .filter(|e| tag_matches(e, &query.tags))
            .map(|e| MemorySearchResult { key: e.key.clone(), entry: e.clone(), score: 1.0 })
            .collect();
        results.truncate(query.limit);
        return results;
    }

    // Score each entry
    let mut scored: Vec<(String, MemoryEntry, f32)> = store.entries.values()
        .filter(|e| tag_matches(e, &query.tags))
        .map(|e| {
            let doc_tokens = tokenize(&e.searchable_text());
            let score = jaccard_similarity(&query_tokens, &doc_tokens);
            (e.key.clone(), e.clone(), score)
        })
        .filter(|(_, _, score)| *score >= query.min_score)
        .collect();

    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(query.limit);

    scored.into_iter()
        .map(|(key, entry, score)| MemorySearchResult { key, entry, score })
        .collect()
}

fn tag_matches(entry: &MemoryEntry, required_tags: &[String]) -> bool {
    required_tags.iter().all(|t| entry.tags.contains(t))
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect()
}

fn jaccard_similarity(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() && b.is_empty() { return 1.0; }
    if a.is_empty() || b.is_empty() { return 0.0; }

    let set_a: std::collections::HashSet<&String> = a.iter().collect();
    let set_b: std::collections::HashSet<&String> = b.iter().collect();

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 { 0.0 } else { intersection as f32 / union as f32 }
}

// ── Memory stats ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MemoryStats {
    pub tenant_id:     String,
    pub agent_id:      String,
    pub total_entries: usize,
    pub live_entries:  usize,
    pub total_bytes:   u64,
}

pub fn memory_stats(base_dir: &Path, tenant_id: &str, agent_id: &str) -> MemoryStats {
    let store = load_store(base_dir, tenant_id, agent_id);
    let total  = store.entries.len();
    let live   = store.live_count();
    let bytes  = serde_json::to_string(&store).map(|s| s.len() as u64).unwrap_or(0);
    MemoryStats {
        tenant_id:     tenant_id.to_string(),
        agent_id:      agent_id.to_string(),
        total_entries: total,
        live_entries:  live,
        total_bytes:   bytes,
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
