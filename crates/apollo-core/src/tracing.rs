//! Agent Observability Layer — step-level distributed tracing.
//!
//! Provides OpenTelemetry-compatible trace spans, tool-call logs, LLM token
//! accounting, and a causal trace graph per agent session. Agents (or their
//! harness) POST spans via the REST API; Apollo stores them in
//! `base_dir/traces/{tenant_id}/{agent_id}/{trace_id}.jsonl`.
//!
//! # Data model
//! - A **Trace** is one end-to-end session (e.g. one user request handled by
//!   an agent). It has a `trace_id`.
//! - A **Span** is one step within the trace (tool call, LLM inference,
//!   sub-agent delegation, etc.). Spans nest via `parent_span_id`.
//! - A **SpanEvent** is an instant within a span (e.g. "prompt sent").
//! - **TokenUsage** is attached to any span that calls an LLM.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Core types ────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Running,
    Ok,
    Error,
    Timeout,
    Cancelled,
}

impl Default for SpanStatus {
    fn default() -> Self { SpanStatus::Running }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TokenUsage {
    pub model:          String,
    pub input_tokens:   u64,
    pub output_tokens:  u64,
    /// Cost in USD (caller-computed from model pricing).
    pub cost_usd:       f64,
    pub provider:       String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SpanEvent {
    pub timestamp:   u64,
    pub name:        String,
    pub attributes:  HashMap<String, String>,
}

/// A single trace span — one measurable unit of work.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TraceSpan {
    /// 32-hex-char span ID.
    pub span_id:        String,
    /// 32-hex-char trace ID (shared across the session).
    pub trace_id:       String,
    /// Parent span ID for nesting; None = root span.
    pub parent_span_id: Option<String>,
    pub tenant_id:      String,
    pub agent_id:       String,
    /// Human name: "tool_call", "llm_inference", "agent_step", "sub_agent", …
    pub name:           String,
    /// Structured attributes: tool name, prompt hash, function args, …
    pub attributes:     HashMap<String, String>,
    /// Timestamped events within this span.
    pub events:         Vec<SpanEvent>,
    /// Token accounting for LLM spans.
    pub token_usage:    Option<TokenUsage>,
    pub status:         SpanStatus,
    pub error:          Option<String>,
    pub start_ts_ms:    u64,   // milliseconds since UNIX epoch
    pub end_ts_ms:      Option<u64>,
    pub duration_ms:    Option<u64>,
}

/// Summary record stored in `_index.jsonl` for fast listing.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TraceSummary {
    pub trace_id:   String,
    pub agent_id:   String,
    pub tenant_id:  String,
    pub root_span:  String,
    pub span_count: usize,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub duration_ms: Option<u64>,
    pub status:     SpanStatus,
    pub start_ts_ms: u64,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn trace_dir(base_dir: &Path, tenant_id: &str, agent_id: &str) -> PathBuf {
    base_dir.join("traces").join(sanitize(tenant_id)).join(sanitize(agent_id))
}

fn trace_file(base_dir: &Path, tenant_id: &str, agent_id: &str, trace_id: &str) -> PathBuf {
    trace_dir(base_dir, tenant_id, agent_id).join(format!("{}.jsonl", sanitize(trace_id)))
}

fn index_file(base_dir: &Path, tenant_id: &str, agent_id: &str) -> PathBuf {
    trace_dir(base_dir, tenant_id, agent_id).join("_index.jsonl")
}

/// Append a span to the trace's JSONL file.
pub fn append_span(base_dir: &Path, span: &TraceSpan) -> Result<()> {
    let dir = trace_dir(base_dir, &span.tenant_id, &span.agent_id);
    fs::create_dir_all(&dir).context("create trace dir")?;

    let path = trace_file(base_dir, &span.tenant_id, &span.agent_id, &span.trace_id);
    let mut file = fs::OpenOptions::new()
        .create(true).append(true)
        .open(&path)
        .context("open trace file")?;
    writeln!(file, "{}", serde_json::to_string(span)?)?;
    Ok(())
}

/// Load all spans for a trace, deduplicating by span_id (last occurrence wins).
/// This is append-friendly: concurrent writers append and readers deduplicate.
pub fn load_trace(
    base_dir: &Path,
    tenant_id: &str,
    agent_id: &str,
    trace_id: &str,
) -> Vec<TraceSpan> {
    let path = trace_file(base_dir, tenant_id, agent_id, trace_id);
    if !path.exists() { return vec![]; }
    let content = fs::read_to_string(&path).unwrap_or_default();
    // Use an IndexMap-like approach: last entry per span_id wins
    let mut by_id: std::collections::HashMap<String, TraceSpan> =
        std::collections::HashMap::new();
    for line in content.lines() {
        if let Ok(span) = serde_json::from_str::<TraceSpan>(line) {
            by_id.insert(span.span_id.clone(), span);
        }
    }
    let mut spans: Vec<TraceSpan> = by_id.into_values().collect();
    spans.sort_by_key(|s| s.start_ts_ms);
    spans
}

/// Record a span update by appending to the JSONL file.
///
/// We use append-only writes to avoid read-modify-rewrite races when multiple
/// agents POST spans for the same trace concurrently. `load_trace` deduplicates
/// by `span_id`, keeping the latest entry (last-writer-wins per span).
pub fn upsert_span(base_dir: &Path, span: &TraceSpan) -> Result<()> {
    append_span(base_dir, span)
}

/// List trace summaries for a tenant+agent (reads _index.jsonl).
pub fn list_traces(
    base_dir: &Path,
    tenant_id: &str,
    agent_id: &str,
    limit: usize,
) -> Vec<TraceSummary> {
    let path = index_file(base_dir, tenant_id, agent_id);
    if !path.exists() { return vec![]; }
    let content = fs::read_to_string(&path).unwrap_or_default();
    let mut summaries: Vec<TraceSummary> = content.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    summaries.sort_by(|a, b| b.start_ts_ms.cmp(&a.start_ts_ms));
    summaries.truncate(limit);
    summaries
}

/// Build and persist a TraceSummary from all spans in a completed trace.
pub fn finalize_trace(
    base_dir: &Path,
    tenant_id: &str,
    agent_id: &str,
    trace_id: &str,
) -> Result<TraceSummary> {
    let spans = load_trace(base_dir, tenant_id, agent_id, trace_id);

    let root = spans.iter().find(|s| s.parent_span_id.is_none());
    let root_name = root.map(|s| s.name.clone()).unwrap_or_else(|| "unknown".to_string());
    let start_ts = root.map(|s| s.start_ts_ms).unwrap_or(now_ms());

    let total_tokens: u64 = spans.iter()
        .filter_map(|s| s.token_usage.as_ref())
        .map(|t| t.input_tokens + t.output_tokens)
        .sum();

    let total_cost: f64 = spans.iter()
        .filter_map(|s| s.token_usage.as_ref())
        .map(|t| t.cost_usd)
        .sum();

    let duration_ms = spans.iter()
        .filter_map(|s| s.end_ts_ms)
        .max()
        .map(|end| end.saturating_sub(start_ts));

    let has_error = spans.iter().any(|s| s.status == SpanStatus::Error);
    let status = if has_error { SpanStatus::Error } else { SpanStatus::Ok };

    let summary = TraceSummary {
        trace_id:      trace_id.to_string(),
        agent_id:      agent_id.to_string(),
        tenant_id:     tenant_id.to_string(),
        root_span:     root_name,
        span_count:    spans.len(),
        total_tokens,
        total_cost_usd: total_cost,
        duration_ms,
        status,
        start_ts_ms:   start_ts,
    };

    // Append to index
    let idx_path = index_file(base_dir, tenant_id, agent_id);
    let dir = trace_dir(base_dir, tenant_id, agent_id);
    fs::create_dir_all(&dir)?;
    let mut f = fs::OpenOptions::new().create(true).append(true).open(&idx_path)?;
    writeln!(f, "{}", serde_json::to_string(&summary)?)?;

    Ok(summary)
}

/// Aggregate token usage stats per tenant (for billing).
pub fn tenant_token_stats(
    base_dir: &Path,
    tenant_id: &str,
) -> TokenStats {
    let traces_root = base_dir.join("traces").join(sanitize(tenant_id));
    let mut stats = TokenStats {
        tenant_id: tenant_id.to_string(),
        ..Default::default()
    };

    if !traces_root.exists() { return stats; }

    let agent_dirs = fs::read_dir(&traces_root).ok();
    if let Some(dirs) = agent_dirs {
        for entry in dirs.flatten() {
            let agent_dir = entry.path();
            if !agent_dir.is_dir() { continue; }

            let agent_id = entry.file_name().to_string_lossy().to_string();
            let traces = list_traces(base_dir, tenant_id, &agent_id, 10_000);
            for t in &traces {
                stats.total_input_tokens  += t.total_tokens / 2; // approx
                stats.total_output_tokens += t.total_tokens / 2;
                stats.total_cost_usd      += t.total_cost_usd;
                stats.trace_count         += 1;
            }
        }
    }

    stats
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct TokenStats {
    pub tenant_id:           String,
    pub total_input_tokens:  u64,
    pub total_output_tokens: u64,
    pub total_cost_usd:      f64,
    pub trace_count:         usize,
}

// ── ID generation ─────────────────────────────────────────────────────────────

/// Generate a new trace or span ID (random hex string).
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn sanitize(id: &str) -> String {
    id.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
