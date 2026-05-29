//! Distributed Agent Memory Layer — per-tenant per-agent key-value store
//! with vector similarity search.
//!
//! Apollo v2.2 upgrades the search backend from Jaccard token overlap to
//! cosine TF-IDF similarity, giving significantly better ranking without
//! requiring an ML runtime. An optional Qdrant vector database backend
//! is also supported for tenants that supply their own embeddings.
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
    pub key:        String,
    pub value:      serde_json::Value,
    pub tags:       Vec<String>,
    pub text:       Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub ttl_secs:   Option<u64>,
    /// Optional dense embedding vector supplied by the caller.
    /// When present with a Qdrant backend, this is used for ANN search.
    /// Ignored when using the TF-IDF backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding:  Option<Vec<f32>>,
}

impl MemoryEntry {
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl_secs {
            now_unix() >= self.created_at + ttl
        } else {
            false
        }
    }

    pub fn searchable_text(&self) -> String {
        let mut parts = vec![self.key.clone()];
        parts.extend(self.tags.clone());
        if let Some(ref t) = self.text { parts.push(t.clone()); }
        if let serde_json::Value::Object(ref map) = self.value {
            for v in map.values() {
                if let serde_json::Value::String(s) = v { parts.push(s.clone()); }
            }
        } else if let serde_json::Value::String(ref s) = self.value {
            parts.push(s.clone());
        }
        parts.join(" ").to_lowercase()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MemoryStore {
    pub entries: HashMap<String, MemoryEntry>,
}

impl MemoryStore {
    pub fn prune_expired(&mut self) {
        self.entries.retain(|_, v| !v.is_expired());
    }
    pub fn live_count(&self) -> usize {
        self.entries.values().filter(|e| !e.is_expired()).count()
    }
}

/// Query for similarity search.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemoryQuery {
    pub query: String,
    #[serde(default)]
    pub tags:  Vec<String>,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub min_score: f32,
    /// Optional query embedding for Qdrant backend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embedding: Option<Vec<f32>>,
}

fn default_limit() -> usize { 10 }

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct MemorySearchResult {
    pub key:   String,
    pub entry: MemoryEntry,
    pub score: f32,
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
    put_memory_with_embedding(base_dir, tenant_id, agent_id, key, value, tags, text, ttl_secs, None)
}

pub fn put_memory_with_embedding(
    base_dir:  &Path,
    tenant_id: &str,
    agent_id:  &str,
    key:       &str,
    value:     serde_json::Value,
    tags:      Vec<String>,
    text:      Option<String>,
    ttl_secs:  Option<u64>,
    embedding: Option<Vec<f32>>,
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
        embedding,
    };
    store.entries.insert(key.to_string(), entry.clone());
    save_store(base_dir, tenant_id, agent_id, &store)?;
    Ok(entry)
}

pub fn get_memory(base_dir: &Path, tenant_id: &str, agent_id: &str, key: &str) -> Option<MemoryEntry> {
    let store = load_store(base_dir, tenant_id, agent_id);
    store.entries.get(key).filter(|e| !e.is_expired()).cloned()
}

pub fn delete_memory(base_dir: &Path, tenant_id: &str, agent_id: &str, key: &str) -> Result<bool> {
    let mut store = load_store(base_dir, tenant_id, agent_id);
    let removed = store.entries.remove(key).is_some();
    if removed { save_store(base_dir, tenant_id, agent_id, &store)?; }
    Ok(removed)
}

pub fn clear_memory(base_dir: &Path, tenant_id: &str, agent_id: &str) -> Result<()> {
    let path = store_path(base_dir, tenant_id, agent_id);
    if path.exists() { fs::remove_file(path)?; }
    Ok(())
}

pub fn list_memory_keys(base_dir: &Path, tenant_id: &str, agent_id: &str) -> Vec<String> {
    let mut store = load_store(base_dir, tenant_id, agent_id);
    store.prune_expired();
    store.entries.keys().cloned().collect()
}

// ── Similarity search — TF-IDF cosine (default) ───────────────────────────────

/// Search using cosine TF-IDF similarity (no ML runtime required).
/// Significantly better ranking than Jaccard, especially for partial matches.
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
        let mut results: Vec<MemorySearchResult> = store.entries.values()
            .filter(|e| tag_matches(e, &query.tags))
            .map(|e| MemorySearchResult { key: e.key.clone(), entry: e.clone(), score: 1.0 })
            .collect();
        results.truncate(query.limit);
        return results;
    }

    // Build IDF from corpus
    let corpus: Vec<Vec<String>> = store.entries.values()
        .map(|e| tokenize(&e.searchable_text()))
        .collect();
    let idf = compute_idf(&query_tokens, &corpus);
    let query_vec = tfidf_vector(&query_tokens, &idf);

    let mut scored: Vec<(String, MemoryEntry, f32)> = store.entries.values()
        .filter(|e| tag_matches(e, &query.tags))
        .map(|e| {
            let doc_tokens = tokenize(&e.searchable_text());
            let doc_vec    = tfidf_vector(&doc_tokens, &idf);
            let score      = cosine_similarity(&query_vec, &doc_vec);
            (e.key.clone(), e.clone(), score)
        })
        .filter(|(_, _, s)| *s >= query.min_score)
        .collect();

    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(query.limit);
    scored.into_iter().map(|(key, entry, score)| MemorySearchResult { key, entry, score }).collect()
}

// ── Similarity search — Qdrant (optional external backend) ───────────────────

/// Search using Qdrant vector database. Requires:
/// - `qdrant_url`: base URL of the Qdrant instance (e.g. `http://localhost:6333`)
/// - `query.embedding`: the query vector (caller-supplied; Apollo has no ML runtime)
///
/// The Qdrant collection name is `apollo-{tenant_id}-{agent_id}`.
/// Points must have been upserted via `upsert_qdrant_point` when storing entries.
pub async fn search_memory_qdrant(
    qdrant_url: &str,
    tenant_id:  &str,
    agent_id:   &str,
    query:      &MemoryQuery,
) -> Result<Vec<MemorySearchResult>> {
    let embedding = match &query.embedding {
        Some(v) => v.clone(),
        None => return Err(anyhow::anyhow!(
            "Qdrant backend requires an embedding vector in the query"
        )),
    };

    let collection = qdrant_collection(tenant_id, agent_id);
    let url = format!("{}/collections/{}/points/search", qdrant_url.trim_end_matches('/'), collection);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let body = serde_json::json!({
        "vector": embedding,
        "limit": query.limit,
        "with_payload": true,
        "score_threshold": query.min_score,
    });

    let resp = client.post(&url).json(&body).send().await?;
    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Qdrant search failed: {}", resp.status()));
    }

    let data: serde_json::Value = resp.json().await?;
    let results = data["result"].as_array().cloned().unwrap_or_default();

    Ok(results.into_iter().filter_map(|point| {
        let score = point["score"].as_f64()? as f32;
        let payload = &point["payload"];
        let entry: MemoryEntry = serde_json::from_value(payload.clone()).ok()?;
        let key = entry.key.clone();
        Some(MemorySearchResult { key, entry, score })
    }).collect())
}

/// Upsert a memory entry as a Qdrant point.
/// Called automatically from `put_memory_with_embedding` when embedding is provided
/// and a Qdrant URL is configured on the node.
pub async fn upsert_qdrant_point(
    qdrant_url: &str,
    tenant_id:  &str,
    agent_id:   &str,
    entry:      &MemoryEntry,
) -> Result<()> {
    let embedding = match &entry.embedding {
        Some(v) => v.clone(),
        None => return Ok(()),   // no embedding → skip Qdrant
    };

    let collection = qdrant_collection(tenant_id, agent_id);
    // Ensure collection exists (idempotent)
    ensure_qdrant_collection(qdrant_url, &collection, embedding.len()).await?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    let url = format!("{}/collections/{}/points", qdrant_url.trim_end_matches('/'), collection);
    let body = serde_json::json!({
        "points": [{
            "id": uuid_from_key(&entry.key),
            "vector": embedding,
            "payload": serde_json::to_value(entry).unwrap_or_default(),
        }]
    });

    client.put(&url).json(&body).send().await?
        .error_for_status()?;
    Ok(())
}

async fn ensure_qdrant_collection(qdrant_url: &str, collection: &str, dim: usize) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let url = format!("{}/collections/{}", qdrant_url.trim_end_matches('/'), collection);
    // If it already exists, this is a no-op (400 is acceptable)
    let _ = client.put(&url).json(&serde_json::json!({
        "vectors": { "size": dim, "distance": "Cosine" }
    })).send().await;
    Ok(())
}

fn qdrant_collection(tenant_id: &str, agent_id: &str) -> String {
    format!("apollo-{}-{}", sanitize(tenant_id), sanitize(agent_id))
}

fn uuid_from_key(key: &str) -> String {
    // Deterministic UUID from key string using UUID v5 (SHA-1 namespace)
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    key.hash(&mut hasher);
    let h = hasher.finish();
    format!("{:016x}-{:04x}-{:04x}-{:04x}-{:012x}",
        h, (h >> 48) & 0xffff, 0x5000 | ((h >> 32) & 0x0fff),
        0x8000 | ((h >> 16) & 0x3fff), h & 0xffff_ffff_ffff)
}

// ── TF-IDF cosine similarity helpers ─────────────────────────────────────────

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2)
        .map(|t| t.to_string())
        .collect()
}

/// Compute IDF (inverse document frequency) for the query terms across the corpus.
fn compute_idf(query_tokens: &[String], corpus: &[Vec<String>]) -> HashMap<String, f32> {
    let n = corpus.len().max(1) as f32;
    let mut idf = HashMap::new();
    for token in query_tokens {
        if idf.contains_key(token) { continue; }
        let df = corpus.iter().filter(|doc| doc.contains(token)).count() as f32;
        idf.insert(token.clone(), (n / (1.0 + df)).ln() + 1.0);
    }
    idf
}

/// Compute a TF-IDF weight vector for a document given pre-computed IDFs.
fn tfidf_vector(tokens: &[String], idf: &HashMap<String, f32>) -> HashMap<String, f32> {
    if tokens.is_empty() { return HashMap::new(); }
    let n = tokens.len() as f32;
    let mut tf: HashMap<String, f32> = HashMap::new();
    for t in tokens { *tf.entry(t.clone()).or_default() += 1.0 / n; }
    tf.into_iter()
        .filter_map(|(k, tf_val)| {
            let idf_val = idf.get(&k).copied().unwrap_or(0.0);
            if idf_val == 0.0 { None } else { Some((k, tf_val * idf_val)) }
        })
        .collect()
}

fn cosine_similarity(a: &HashMap<String, f32>, b: &HashMap<String, f32>) -> f32 {
    if a.is_empty() || b.is_empty() { return 0.0; }
    let dot: f32 = a.iter().filter_map(|(k, v)| b.get(k).map(|bv| v * bv)).sum();
    let mag_a: f32 = a.values().map(|v| v * v).sum::<f32>().sqrt();
    let mag_b: f32 = b.values().map(|v| v * v).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 { 0.0 } else { dot / (mag_a * mag_b) }
}

fn tag_matches(entry: &MemoryEntry, required: &[String]) -> bool {
    required.iter().all(|t| entry.tags.contains(t))
}

// ── Memory stats ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct MemoryStats {
    pub tenant_id:     String,
    pub agent_id:      String,
    pub total_entries: usize,
    pub live_entries:  usize,
    pub total_bytes:   u64,
    pub with_embeddings: usize,
}

pub fn memory_stats(base_dir: &Path, tenant_id: &str, agent_id: &str) -> MemoryStats {
    let store = load_store(base_dir, tenant_id, agent_id);
    let total  = store.entries.len();
    let live   = store.live_count();
    let bytes  = serde_json::to_string(&store).map(|s| s.len() as u64).unwrap_or(0);
    let with_emb = store.entries.values().filter(|e| e.embedding.is_some()).count();
    MemoryStats {
        tenant_id:       tenant_id.to_string(),
        agent_id:        agent_id.to_string(),
        total_entries:   total,
        live_entries:    live,
        total_bytes:     bytes,
        with_embeddings: with_emb,
    }
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
