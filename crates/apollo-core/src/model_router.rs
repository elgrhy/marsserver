//! Model Governance & Routing Layer.
//!
//! Provides cost-aware, latency-aware, and policy-aware LLM model routing.
//! Providers register their available models; the router selects the best
//! model for each request based on tenant policy, cost thresholds, and
//! real-time availability.
//!
//! Models are registered in `base_dir/models/registry.json`.
//! Per-tenant model usage is tracked in `base_dir/models/usage/{tenant_id}.json`.
//!
//! REST API:
//!   GET    /models                    — list all registered models
//!   PUT    /models/{model_id}         — register or update a model
//!   DELETE /models/{model_id}         — remove a model
//!   POST   /models/route              — get routing recommendation
//!   GET    /models/usage/{tenant_id}  — per-tenant model usage

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Model registry ────────────────────────────────────────────────────────────

/// A registered LLM model with routing metadata.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModelRecord {
    /// Unique identifier, e.g. "claude-sonnet-4-6", "gpt-4o", "llama-3-70b".
    pub model_id:            String,
    /// Provider: "anthropic", "openai", "local", "azure", "bedrock", …
    pub provider:            String,
    /// Optional API endpoint override (uses provider default if None).
    pub endpoint:            Option<String>,

    // ── Pricing ──────────────────────────────────────────────────────────────
    /// USD per 1M input tokens.
    pub cost_per_m_input:    f64,
    /// USD per 1M output tokens.
    pub cost_per_m_output:   f64,

    // ── Performance ──────────────────────────────────────────────────────────
    /// Observed median latency in ms (updated by the routing feedback loop).
    pub latency_p50_ms:      u64,
    /// Observed 99th-percentile latency.
    pub latency_p99_ms:      u64,
    /// Tokens per second (output speed).
    pub throughput_tok_s:    f32,

    // ── Capabilities ─────────────────────────────────────────────────────────
    /// Declared capabilities: "vision", "code", "function_calling", "long_context", …
    pub capabilities:        Vec<String>,
    /// Maximum context window in tokens.
    pub context_window:      u64,

    // ── Availability ─────────────────────────────────────────────────────────
    pub is_local:            bool,
    pub is_available:        bool,
    /// Routing priority (lower = preferred when all else equal).
    pub priority:            i32,

    pub registered_at:       u64,
    pub updated_at:          u64,
}

// ── Routing request & response ────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoutingRequest {
    pub tenant_id:          String,
    /// Preferred model ID (hint, may be overridden by policy).
    pub preferred_model:    Option<String>,
    /// Maximum acceptable cost for this single call in USD.
    pub max_cost_usd:       Option<f64>,
    /// Estimated input tokens (used for cost filtering).
    pub input_tokens:       Option<u64>,
    /// Estimated output tokens.
    pub output_tokens:      Option<u64>,
    /// Required capabilities: e.g. ["vision", "function_calling"].
    #[serde(default)]
    pub required_capabilities: Vec<String>,
    /// If true, only local/on-prem models are acceptable.
    pub require_local:      bool,
    /// Maximum acceptable latency (p50) in ms.
    pub max_latency_ms:     Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoutingDecision {
    pub selected_model:  String,
    pub provider:        String,
    pub endpoint:        Option<String>,
    pub estimated_cost:  Option<f64>,
    pub latency_p50_ms:  u64,
    pub reason:          String,
    pub fallback_models: Vec<String>,
}

// ── Per-tenant model usage ────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelUsageRecord {
    pub tenant_id:          String,
    pub usage_by_model:     HashMap<String, ModelUsageStat>,
    pub total_cost_usd:     f64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub last_updated:       u64,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ModelUsageStat {
    pub model_id:       String,
    pub call_count:     u64,
    pub input_tokens:   u64,
    pub output_tokens:  u64,
    pub cost_usd:       f64,
    pub avg_latency_ms: f64,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn registry_path(base_dir: &Path) -> PathBuf {
    base_dir.join("models").join("registry.json")
}

fn model_usage_path(base_dir: &Path, tenant_id: &str) -> PathBuf {
    base_dir.join("models").join("usage")
        .join(format!("{}.json", sanitize(tenant_id)))
}

pub fn load_model_registry(base_dir: &Path) -> Vec<ModelRecord> {
    let path = registry_path(base_dir);
    if !path.exists() { return vec![]; }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_model_registry(base_dir: &Path, models: &[ModelRecord]) -> Result<()> {
    let path = registry_path(base_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("create models dir")?;
    }
    fs::write(path, serde_json::to_string_pretty(models)?)?;
    Ok(())
}

pub fn register_model(base_dir: &Path, mut model: ModelRecord) -> Result<ModelRecord> {
    let mut models = load_model_registry(base_dir);
    model.updated_at = now_unix();
    if model.registered_at == 0 { model.registered_at = model.updated_at; }

    if let Some(pos) = models.iter().position(|m| m.model_id == model.model_id) {
        models[pos] = model.clone();
    } else {
        models.push(model.clone());
    }
    save_model_registry(base_dir, &models)?;
    Ok(model)
}

pub fn remove_model(base_dir: &Path, model_id: &str) -> Result<()> {
    let mut models = load_model_registry(base_dir);
    let before = models.len();
    models.retain(|m| m.model_id != model_id);
    if models.len() == before {
        return Err(anyhow!("Model '{}' not found", model_id));
    }
    save_model_registry(base_dir, &models)
}

pub fn load_model_usage(base_dir: &Path, tenant_id: &str) -> ModelUsageRecord {
    let path = model_usage_path(base_dir, tenant_id);
    if !path.exists() {
        return ModelUsageRecord { tenant_id: tenant_id.to_string(), ..Default::default() };
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| ModelUsageRecord { tenant_id: tenant_id.to_string(), ..Default::default() })
}

pub fn record_model_usage(
    base_dir:     &Path,
    tenant_id:    &str,
    model_id:     &str,
    input_tokens: u64,
    output_tokens: u64,
    cost_usd:     f64,
    latency_ms:   u64,
) -> Result<()> {
    let mut usage = load_model_usage(base_dir, tenant_id);

    let stat = usage.usage_by_model.entry(model_id.to_string())
        .or_insert_with(|| ModelUsageStat {
            model_id: model_id.to_string(),
            ..Default::default()
        });

    stat.call_count     += 1;
    stat.input_tokens   += input_tokens;
    stat.output_tokens  += output_tokens;
    stat.cost_usd       += cost_usd;
    stat.avg_latency_ms  = stat.avg_latency_ms * 0.9 + latency_ms as f64 * 0.1;

    usage.total_cost_usd      += cost_usd;
    usage.total_input_tokens  += input_tokens;
    usage.total_output_tokens += output_tokens;
    usage.last_updated         = now_unix();

    let path = model_usage_path(base_dir, tenant_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&usage)?)?;
    Ok(())
}

// ── Router ────────────────────────────────────────────────────────────────────

/// The stateless model router.
pub struct ModelRouter {
    pub base_dir: PathBuf,
}

impl ModelRouter {
    pub fn new(base_dir: PathBuf) -> Self { Self { base_dir } }

    /// Select the best model for a routing request.
    /// Policy (tenant) constraints are applied first, then cost/latency sorting.
    pub fn route(&self, req: &RoutingRequest) -> Result<RoutingDecision> {
        let models = load_model_registry(&self.base_dir);

        if models.is_empty() {
            return Err(anyhow!("No models registered. Use PUT /models/{{model_id}} to register one."));
        }

        // Load tenant policy model constraints
        let policy = crate::policy::load_policy(&self.base_dir, &req.tenant_id);
        let mp = &policy.model_policy;

        // Filter candidates
        let candidates: Vec<&ModelRecord> = models.iter()
            .filter(|m| m.is_available)
            .filter(|m| {
                // Local constraint
                if req.require_local || mp.require_local { m.is_local } else { true }
            })
            .filter(|m| {
                // Policy allowed models
                if let Some(ref allowed) = mp.allowed_models {
                    allowed.contains(&m.model_id)
                } else { true }
            })
            .filter(|m| {
                // Required capabilities
                req.required_capabilities.iter().all(|cap| m.capabilities.contains(cap))
            })
            .filter(|m| {
                // Cost filter
                if let (Some(max_cost), Some(in_tok), Some(out_tok)) =
                    (mp.max_cost_per_request_usd.or(req.max_cost_usd),
                     req.input_tokens, req.output_tokens)
                {
                    let est = (in_tok as f64 / 1_000_000.0) * m.cost_per_m_input
                        + (out_tok as f64 / 1_000_000.0) * m.cost_per_m_output;
                    est <= max_cost
                } else { true }
            })
            .filter(|m| {
                // Latency filter
                if let Some(max_lat) = req.max_latency_ms {
                    m.latency_p50_ms <= max_lat
                } else { true }
            })
            .collect();

        if candidates.is_empty() {
            return Err(anyhow!(
                "No models match the routing constraints. \
                 Relax cost/latency/capability requirements or register additional models."
            ));
        }

        // Preference order: explicit preferred → policy preferred → lowest priority score
        let selected = if let Some(ref pref) = req.preferred_model {
            candidates.iter().find(|m| m.model_id == *pref)
                .or_else(|| candidates.iter().min_by_key(|m| m.priority))
        } else if let Some(ref pref) = mp.preferred_model {
            candidates.iter().find(|m| m.model_id == *pref)
                .or_else(|| candidates.iter().min_by_key(|m| m.priority))
        } else {
            candidates.iter().min_by_key(|m| m.priority)
        }.ok_or_else(|| anyhow!("Routing failed: no suitable model found"))?;

        let estimated_cost = if let (Some(in_tok), Some(out_tok)) =
            (req.input_tokens, req.output_tokens)
        {
            Some(
                (in_tok  as f64 / 1_000_000.0) * selected.cost_per_m_input +
                (out_tok as f64 / 1_000_000.0) * selected.cost_per_m_output,
            )
        } else { None };

        let fallbacks: Vec<String> = candidates.iter()
            .filter(|m| m.model_id != selected.model_id)
            .take(3)
            .map(|m| m.model_id.clone())
            .collect();

        Ok(RoutingDecision {
            selected_model:  selected.model_id.clone(),
            provider:        selected.provider.clone(),
            endpoint:        selected.endpoint.clone(),
            estimated_cost,
            latency_p50_ms:  selected.latency_p50_ms,
            reason:          format!(
                "Selected '{}' (priority={}, cost_per_m_in=${:.4}, p50={}ms)",
                selected.model_id, selected.priority,
                selected.cost_per_m_input, selected.latency_p50_ms
            ),
            fallback_models: fallbacks,
        })
    }

    /// Update routing performance metrics (called by agent after a successful call).
    pub fn feedback(
        &self,
        model_id:    &str,
        latency_ms:  u64,
        is_success:  bool,
    ) -> Result<()> {
        let mut models = load_model_registry(&self.base_dir);
        if let Some(m) = models.iter_mut().find(|m| m.model_id == model_id) {
            if is_success {
                // EMA update for latency
                m.latency_p50_ms = (m.latency_p50_ms as f64 * 0.9 + latency_ms as f64 * 0.1) as u64;
            } else {
                m.latency_p99_ms = m.latency_p99_ms.max(latency_ms);
            }
            m.updated_at = now_unix();
            save_model_registry(&self.base_dir, &models)?;
        }
        Ok(())
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
