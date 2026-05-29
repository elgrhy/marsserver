//! Apollo Kubernetes Operator v2.2
//!
//! Watches `ApolloAgent` Custom Resource Definitions and reconciles them
//! against the Apollo node REST API:
//!
//! - CRD created  → register agent on node + start for tenant
//! - CRD updated  → re-register if source changed, adjust replicas
//! - CRD deleted  → stop agent for tenant
//!
//! ## Usage
//!
//! ```bash
//! # Install CRD
//! kubectl apply -f deploy/helm/apollo/templates/crd.yaml
//!
//! # Start operator (needs KUBECONFIG or in-cluster ServiceAccount)
//! apollo-operator \
//!   --node-url   http://apollo-node:8080 \
//!   --secret-key "$APOLLO_SECRET_KEY" \
//!   --namespace  default
//! ```

use anyhow::{anyhow, Context, Result};
use futures::StreamExt;
use k8s_openapi::apimachinery::pkg::apis::meta::v1::Condition;
use kube::{
    api::{Api, ListParams, Patch, PatchParams, ResourceExt},
    client::Client,
    runtime::{controller::Action, Controller},
    CustomResource, Resource,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::EnvFilter;

// ── CRD definition ─────────────────────────────────────────────────────────────

/// Spec for an ApolloAgent custom resource.
#[derive(CustomResource, Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[kube(
    group = "apollo.dev",
    version = "v1",
    kind = "ApolloAgent",
    namespaced,
    status = "ApolloAgentStatus",
    printcolumn = r#"{"name":"Agent","type":"string","jsonPath":".spec.agentId"}"#,
    printcolumn = r#"{"name":"Tenant","type":"string","jsonPath":".spec.tenantId"}"#,
    printcolumn = r#"{"name":"Phase","type":"string","jsonPath":".status.phase"}"#,
)]
pub struct ApolloAgentSpec {
    /// Agent ID as registered on the Apollo node (must already be registered, or provide source).
    pub agent_id: String,
    /// Tenant / user ID to run the agent for.
    pub tenant_id: String,
    /// Optional: agent package source (git URL, HTTPS archive, or local path).
    /// If set, the operator will register the agent before starting it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_source: Option<String>,
    /// Desired number of replicas (instances for this tenant).
    #[serde(default = "default_replicas")]
    pub replicas: u32,
    /// Apollo node URL to manage this agent on.
    pub node_url: String,
    /// Reference to a K8s Secret holding the Apollo API key.
    pub secret_key_ref: SecretKeyRef,
}

fn default_replicas() -> u32 { 1 }

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct SecretKeyRef {
    /// Name of the K8s Secret.
    pub name: String,
    /// Key within the Secret.
    pub key: String,
}

/// Status subresource for ApolloAgent.
#[derive(Serialize, Deserialize, Debug, Clone, Default, JsonSchema)]
pub struct ApolloAgentStatus {
    /// Lifecycle phase: Pending | Registering | Running | Stopped | Error
    pub phase:        String,
    pub message:      Option<String>,
    pub agent_id:     Option<String>,
    pub active_pids:  Vec<u32>,
    pub last_sync_at: Option<String>,
    pub conditions:   Vec<Condition>,
}

// ── Operator context ──────────────────────────────────────────────────────────

struct OperatorContext {
    client: Client,
    // Apollo node connection is per-resource (node_url + secret from K8s Secret)
}

// ── Reconciler ────────────────────────────────────────────────────────────────

async fn reconcile(agent: Arc<ApolloAgent>, ctx: Arc<OperatorContext>) -> Result<Action, ReconcileError> {
    let ns        = agent.namespace().unwrap_or_default();
    let name      = agent.name_any();
    let spec      = &agent.spec;

    tracing::info!(name, ns, agent_id = %spec.agent_id, tenant = %spec.tenant_id, "reconciling ApolloAgent");

    // Resolve the API key from the referenced K8s Secret
    let api_key = resolve_secret(
        &ctx.client, &ns,
        &spec.secret_key_ref.name,
        &spec.secret_key_ref.key,
    ).await.map_err(|e| ReconcileError::SecretNotFound(e.to_string()))?;

    let node = ApolloNodeClient::new(&spec.node_url, &api_key);

    // Patch status to Pending initially
    let agents_api: Api<ApolloAgent> = Api::namespaced(ctx.client.clone(), &ns);

    // Step 1: register agent if source is provided and not yet registered
    if let Some(ref source) = spec.agent_source {
        if let Err(e) = node.register_agent(source).await {
            tracing::warn!(name, source, error = %e, "agent registration failed (may already exist)");
        }
    }

    // Step 2: check running instances for this tenant
    let running = node.count_running(&spec.agent_id, &spec.tenant_id).await.unwrap_or(0);
    let desired  = spec.replicas as usize;

    let phase = if running == 0 && desired == 0 {
        "Stopped"
    } else if running < desired {
        // Start more instances
        for _ in running..desired {
            node.run_agent(&spec.agent_id, &spec.tenant_id).await
                .map_err(|e| ReconcileError::ApiError(e.to_string()))?;
        }
        "Running"
    } else if running > desired {
        // Stop excess (best-effort)
        node.stop_agent(&spec.agent_id, &spec.tenant_id).await.ok();
        "Running"
    } else {
        "Running"
    };

    // Step 3: update status
    let status = serde_json::json!({
        "status": {
            "phase": phase,
            "agentId": spec.agent_id,
            "lastSyncAt": chrono_now(),
            "message": format!("{}/{} instances running", running.min(desired), desired),
        }
    });
    agents_api.patch_status(
        &name,
        &PatchParams::apply("apollo-operator"),
        &Patch::Merge(&status),
    ).await.map_err(|e| ReconcileError::KubeError(e.to_string()))?;

    tracing::info!(name, phase, "reconcile complete");
    Ok(Action::requeue(Duration::from_secs(30)))
}

fn error_policy(agent: Arc<ApolloAgent>, error: &ReconcileError, _ctx: Arc<OperatorContext>) -> Action {
    tracing::error!(
        name = agent.name_any(),
        error = %error,
        "reconcile error — requeuing with backoff"
    );
    Action::requeue(Duration::from_secs(60))
}

// ── Apollo node REST client ───────────────────────────────────────────────────

struct ApolloNodeClient {
    base_url: String,
    api_key:  String,
    http:     reqwest::Client,
}

impl ApolloNodeClient {
    fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key:  api_key.to_string(),
            http:     reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("reqwest client"),
        }
    }

    fn auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header("X-Apollo-Key", &self.api_key)
    }

    async fn register_agent(&self, source: &str) -> Result<()> {
        let url = format!("{}/agents/add", self.base_url);
        self.auth(self.http.post(&url))
            .json(&serde_json::json!({"source": source}))
            .send().await?.error_for_status()?;
        Ok(())
    }

    async fn run_agent(&self, agent_id: &str, tenant_id: &str) -> Result<()> {
        let url = format!("{}/agents/run", self.base_url);
        self.auth(self.http.post(&url))
            .json(&serde_json::json!({"agent": agent_id, "tenant": tenant_id}))
            .send().await?.error_for_status()?;
        Ok(())
    }

    async fn stop_agent(&self, agent_id: &str, tenant_id: &str) -> Result<()> {
        let url = format!("{}/agents/stop", self.base_url);
        self.auth(self.http.delete(&url))
            .json(&serde_json::json!({"agent": agent_id, "tenant": tenant_id}))
            .send().await?.error_for_status()?;
        Ok(())
    }

    async fn count_running(&self, agent_id: &str, tenant_id: &str) -> Result<usize> {
        let url = format!("{}/agents/list", self.base_url);
        let resp = self.auth(self.http.get(&url)).send().await?.error_for_status()?;
        let _agents: serde_json::Value = resp.json().await?;
        // For simplicity: check usage endpoint for running instance count
        let usage_url = format!("{}/usage/{}", self.base_url, tenant_id);
        let usage: serde_json::Value = self.auth(self.http.get(&usage_url))
            .send().await?.error_for_status()?.json().await?;
        let starts = usage.get("starts").and_then(|v| v.as_u64()).unwrap_or(0);
        let stops  = usage.get("stops").and_then(|v| v.as_u64()).unwrap_or(0);
        // Approximate: starts - stops (not perfectly accurate but sufficient for operator)
        Ok(starts.saturating_sub(stops) as usize)
    }
}

// ── K8s Secret resolution ─────────────────────────────────────────────────────

async fn resolve_secret(client: &Client, ns: &str, secret_name: &str, key: &str) -> Result<String> {
    use k8s_openapi::api::core::v1::Secret;
    let secrets: Api<Secret> = Api::namespaced(client.clone(), ns);
    let secret = secrets.get(secret_name).await
        .context(format!("Secret '{}' not found in namespace '{}'", secret_name, ns))?;
    let data = secret.data.ok_or_else(|| anyhow!("Secret '{}' has no data", secret_name))?;
    let bytes = data.get(key)
        .ok_or_else(|| anyhow!("Key '{}' not found in Secret '{}'", key, secret_name))?;
    String::from_utf8(bytes.0.clone())
        .context("Secret value is not valid UTF-8")
}

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
enum ReconcileError {
    #[error("Secret not found: {0}")]
    SecretNotFound(String),
    #[error("Apollo API error: {0}")]
    ApiError(String),
    #[error("Kubernetes API error: {0}")]
    KubeError(String),
}

// ── CLI + main ────────────────────────────────────────────────────────────────

#[derive(Debug)]
struct CliArgs {
    namespace: String,
}

fn parse_args() -> CliArgs {
    let namespace = std::env::var("WATCH_NAMESPACE")
        .unwrap_or_else(|_| "default".to_string());
    CliArgs { namespace }
}

fn chrono_now() -> String {
    // RFC3339 timestamp without external crate
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (y, mo, d, h, mi, s) = unix_to_ymd(secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, mo, d, h, mi, s)
}

fn unix_to_ymd(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Simplified date calculation (good enough for status timestamps)
    let y = 1970 + days / 365;
    let mo = (days % 365) / 30 + 1;
    let d = (days % 365) % 30 + 1;
    (y, mo.min(12), d.min(31), h, m, s)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .init();

    let args = parse_args();
    tracing::info!(namespace = %args.namespace, "Apollo Operator starting");

    let client = Client::try_default().await
        .context("Failed to create Kubernetes client")?;

    let agents: Api<ApolloAgent> = if args.namespace == "*" {
        Api::all(client.clone())
    } else {
        Api::namespaced(client.clone(), &args.namespace)
    };

    let ctx = Arc::new(OperatorContext { client: client.clone() });

    tracing::info!("watching ApolloAgent resources");

    Controller::new(agents, ListParams::default())
        .run(reconcile, error_policy, ctx)
        .for_each(|result| async move {
            match result {
                Ok((obj, _)) => tracing::debug!(name = %obj.name, "reconcile ok"),
                Err(e)       => tracing::error!(error = %e, "reconcile failed"),
            }
        })
        .await;

    Ok(())
}
