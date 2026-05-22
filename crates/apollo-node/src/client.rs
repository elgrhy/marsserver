//! NodeClient — typed REST client for all Apollo v2.0 API endpoints.
//!
//! Used by all v2.0 CLI commands to talk to a running Apollo node.
//! Connection parameters: `--node URL` and `--key KEY` (or env APOLLO_KEY).

use anyhow::{anyhow, Result};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

// Re-use apollo_core types so CLI output is type-safe
use apollo_core::health::{HealthRecord, FleetHealthSummary};
use apollo_core::memory::{MemoryEntry, MemoryQuery, MemorySearchResult, MemoryStats};
use apollo_core::model_router::{ModelRecord, RoutingRequest, RoutingDecision, ModelUsageRecord};
use apollo_core::scheduler::{ScheduledJob, JobRunRecord};
use apollo_core::orchestration::{
    Blueprint, AgentGroup, WorkflowDef, WorkflowRun,
};
use apollo_core::arch_selector::{ArchitectureDecision, QuickClassifyRequest, QuickClassifyResponse};
use apollo_core::policy::{TenantPolicy, ComplianceReport};
use apollo_core::tracing::{TraceSummary, TraceSpan, TokenStats};
use apollo_core::usage::TenantUsage;

// ── Client ────────────────────────────────────────────────────────────────────

pub struct NodeClient {
    base:   String,
    key:    String,
    client: Client,
}

impl NodeClient {
    pub fn new(node: &str, key: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert("x-apollo-key", HeaderValue::from_str(key).unwrap());
        let client = Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            base:   node.trim_end_matches('/').to_string(),
            key:    key.to_string(),
            client,
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self.client.get(&url).send()
            .map_err(|e| anyhow!("Connection failed ({}): {}", url, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(anyhow!("HTTP {} from {}: {}", status, path, body));
        }
        resp.json::<T>().map_err(|e| anyhow!("Parse error: {}", e))
    }

    fn post<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self.client.post(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(body)
            .send()
            .map_err(|e| anyhow!("Connection failed ({}): {}", url, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let b = resp.text().unwrap_or_default();
            return Err(anyhow!("HTTP {} from {}: {}", status, path, b));
        }
        resp.json::<T>().map_err(|e| anyhow!("Parse error: {}", e))
    }

    fn put<B: Serialize, T: DeserializeOwned>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base, path);
        let resp = self.client.put(&url)
            .header(CONTENT_TYPE, "application/json")
            .json(body)
            .send()
            .map_err(|e| anyhow!("Connection failed ({}): {}", url, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let b = resp.text().unwrap_or_default();
            return Err(anyhow!("HTTP {} from {}: {}", status, path, b));
        }
        resp.json::<T>().map_err(|e| anyhow!("Parse error: {}", e))
    }

    fn delete_req(&self, path: &str) -> Result<Value> {
        let url = format!("{}{}", self.base, path);
        let resp = self.client.delete(&url).send()
            .map_err(|e| anyhow!("Connection failed ({}): {}", url, e))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let b = resp.text().unwrap_or_default();
            return Err(anyhow!("HTTP {} from {}: {}", status, path, b));
        }
        resp.json::<Value>().map_err(|e| anyhow!("Parse error: {}", e))
    }

    fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.post(path, &serde_json::json!({}))
    }

    // ── Metrics / Node ────────────────────────────────────────────────────────

    pub fn metrics(&self) -> Result<Value> {
        self.get("/metrics")
    }

    pub fn node_health(&self) -> Result<Value> {
        self.get("/health")
    }

    // ── Usage ─────────────────────────────────────────────────────────────────

    pub fn usage_all(&self) -> Result<Vec<TenantUsage>> {
        self.get("/usage")
    }

    pub fn usage_tenant(&self, tenant: &str) -> Result<TenantUsage> {
        self.get(&format!("/usage/{}", tenant))
    }

    pub fn usage_reset(&self, tenant: &str) -> Result<Value> {
        self.post_empty(&format!("/usage/{}/reset", tenant))
    }

    // ── Traces ────────────────────────────────────────────────────────────────

    pub fn traces_list(&self, tenant: &str, agent: &str) -> Result<Vec<TraceSummary>> {
        self.get(&format!("/traces/{}/{}", tenant, agent))
    }

    pub fn traces_get(&self, tenant: &str, agent: &str, trace_id: &str) -> Result<Vec<TraceSpan>> {
        self.get(&format!("/traces/{}/{}/{}", tenant, agent, trace_id))
    }

    pub fn traces_tokens(&self, tenant: &str) -> Result<TokenStats> {
        self.get(&format!("/traces/{}/tokens", tenant))
    }

    // ── Policy ────────────────────────────────────────────────────────────────

    pub fn policy_get(&self, tenant: &str) -> Result<TenantPolicy> {
        self.get(&format!("/tenants/{}/policy", tenant))
    }

    pub fn policy_set(&self, tenant: &str, policy: &TenantPolicy) -> Result<Value> {
        self.put(&format!("/tenants/{}/policy", tenant), policy)
    }

    pub fn policy_delete(&self, tenant: &str) -> Result<Value> {
        self.delete_req(&format!("/tenants/{}/policy", tenant))
    }

    pub fn policy_compliance(&self, tenant: &str) -> Result<ComplianceReport> {
        self.get(&format!("/tenants/{}/compliance", tenant))
    }

    // ── Health ────────────────────────────────────────────────────────────────

    pub fn health_fleet(&self) -> Result<FleetHealthSummary> {
        self.get("/health/fleet/summary")
    }

    pub fn health_tenant(&self, tenant: &str) -> Result<Vec<HealthRecord>> {
        self.get(&format!("/health/{}", tenant))
    }

    pub fn health_agent(&self, tenant: &str, agent: &str) -> Result<HealthRecord> {
        self.get(&format!("/health/{}/{}", tenant, agent))
    }

    // ── Memory ────────────────────────────────────────────────────────────────

    pub fn memory_list(&self, tenant: &str, agent: &str) -> Result<Vec<String>> {
        self.get(&format!("/memory/{}/{}", tenant, agent))
    }

    pub fn memory_get(&self, tenant: &str, agent: &str, key: &str) -> Result<MemoryEntry> {
        self.get(&format!("/memory/{}/{}/{}", tenant, agent, key))
    }

    pub fn memory_put(&self, tenant: &str, agent: &str, key: &str, body: &Value) -> Result<Value> {
        self.put(&format!("/memory/{}/{}/{}", tenant, agent, key), body)
    }

    pub fn memory_delete(&self, tenant: &str, agent: &str, key: &str) -> Result<Value> {
        self.delete_req(&format!("/memory/{}/{}/{}", tenant, agent, key))
    }

    pub fn memory_search(&self, tenant: &str, agent: &str, q: &MemoryQuery) -> Result<Vec<MemorySearchResult>> {
        self.post(&format!("/memory/{}/{}/search", tenant, agent), q)
    }

    pub fn memory_clear(&self, tenant: &str, agent: &str) -> Result<Value> {
        self.delete_req(&format!("/memory/{}/{}", tenant, agent))
    }

    pub fn memory_stats(&self, tenant: &str, agent: &str) -> Result<MemoryStats> {
        self.get(&format!("/memory/{}/{}/stats", tenant, agent))
    }

    // ── Models ────────────────────────────────────────────────────────────────

    pub fn models_list(&self) -> Result<Vec<ModelRecord>> {
        self.get("/models")
    }

    pub fn model_put(&self, model_id: &str, rec: &ModelRecord) -> Result<Value> {
        self.put(&format!("/models/{}", model_id), rec)
    }

    pub fn model_delete(&self, model_id: &str) -> Result<Value> {
        self.delete_req(&format!("/models/{}", model_id))
    }

    pub fn models_route(&self, req: &RoutingRequest) -> Result<RoutingDecision> {
        self.post("/models/route", req)
    }

    pub fn models_usage(&self, tenant: &str) -> Result<ModelUsageRecord> {
        self.get(&format!("/models/usage/{}", tenant))
    }

    // ── Scheduler ────────────────────────────────────────────────────────────

    pub fn schedule_list(&self) -> Result<Vec<ScheduledJob>> {
        self.get("/schedule")
    }

    pub fn schedule_create(&self, job: &Value) -> Result<ScheduledJob> {
        self.post("/schedule", job)
    }

    pub fn schedule_get(&self, job_id: &str) -> Result<ScheduledJob> {
        self.get(&format!("/schedule/{}", job_id))
    }

    pub fn schedule_delete(&self, job_id: &str) -> Result<Value> {
        self.delete_req(&format!("/schedule/{}", job_id))
    }

    pub fn schedule_run(&self, job_id: &str) -> Result<Value> {
        self.post_empty(&format!("/schedule/{}/run", job_id))
    }

    pub fn schedule_history(&self, job_id: &str) -> Result<Vec<JobRunRecord>> {
        self.get(&format!("/schedule/{}/history", job_id))
    }

    // ── Blueprints ────────────────────────────────────────────────────────────

    pub fn blueprints_list(&self) -> Result<Vec<Blueprint>> {
        self.get("/blueprints")
    }

    pub fn blueprint_create(&self, bp: &Value) -> Result<Blueprint> {
        self.post("/blueprints", bp)
    }

    pub fn blueprint_get(&self, id: &str) -> Result<Blueprint> {
        self.get(&format!("/blueprints/{}", id))
    }

    pub fn blueprint_delete(&self, id: &str) -> Result<Value> {
        self.delete_req(&format!("/blueprints/{}", id))
    }

    pub fn blueprint_deploy(&self, id: &str, tenant: &str) -> Result<Value> {
        self.post(&format!("/blueprints/{}/deploy", id), &serde_json::json!({"tenant_id": tenant}))
    }

    // ── Groups ────────────────────────────────────────────────────────────────

    pub fn groups_list(&self) -> Result<Vec<AgentGroup>> {
        self.get("/groups")
    }

    pub fn group_create(&self, body: &Value) -> Result<AgentGroup> {
        self.post("/groups", body)
    }

    pub fn group_get(&self, id: &str) -> Result<AgentGroup> {
        self.get(&format!("/groups/{}", id))
    }

    pub fn group_delete(&self, id: &str) -> Result<Value> {
        self.delete_req(&format!("/groups/{}", id))
    }

    pub fn group_run(&self, id: &str) -> Result<Value> {
        self.post_empty(&format!("/groups/{}/run", id))
    }

    pub fn group_stop(&self, id: &str) -> Result<Value> {
        self.post_empty(&format!("/groups/{}/stop", id))
    }

    // ── Workflows ────────────────────────────────────────────────────────────

    pub fn workflows_list(&self) -> Result<Vec<WorkflowDef>> {
        self.get("/workflows")
    }

    pub fn workflow_create(&self, def: &WorkflowDef) -> Result<WorkflowDef> {
        self.post("/workflows", def)
    }

    pub fn workflow_get(&self, id: &str) -> Result<WorkflowDef> {
        self.get(&format!("/workflows/{}", id))
    }

    pub fn workflow_delete(&self, id: &str) -> Result<Value> {
        self.delete_req(&format!("/workflows/{}", id))
    }

    pub fn workflow_run(&self, id: &str) -> Result<WorkflowRun> {
        self.post_empty(&format!("/workflows/{}/run", id))
    }

    pub fn workflow_runs(&self, id: &str) -> Result<Vec<WorkflowRun>> {
        self.get(&format!("/workflows/{}/runs", id))
    }

    pub fn workflow_run_get(&self, run_id: &str) -> Result<WorkflowRun> {
        self.get(&format!("/workflows/runs/{}", run_id))
    }

    pub fn workflow_arch(&self, id: &str) -> Result<ArchitectureDecision> {
        self.get(&format!("/architecture/select/{}", id))
    }

    // ── Architecture selection ────────────────────────────────────────────────

    pub fn arch_select(&self, def: &WorkflowDef) -> Result<ArchitectureDecision> {
        self.post("/architecture/select", def)
    }

    pub fn arch_classify(&self, req: &QuickClassifyRequest) -> Result<QuickClassifyResponse> {
        self.post("/architecture/classify", req)
    }
}
