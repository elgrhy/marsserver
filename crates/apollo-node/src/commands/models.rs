//! CLI commands for model registry and routing.

use anyhow::Result;
use clap::Subcommand;
use apollo_core::model_router::{ModelRecord, RoutingRequest};
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum ModelsCmd {
    /// List all registered LLM models
    List,
    /// Register or update a model
    Add {
        model_id: String,
        #[arg(long)] provider:     String,
        #[arg(long)] cost_input:   f64,
        #[arg(long)] cost_output:  f64,
        #[arg(long, default_value = "800")]   latency_p50:    u64,
        #[arg(long, default_value = "2000")]  latency_p99:    u64,
        #[arg(long, default_value = "50.0")]  throughput:     f32,
        /// Comma-separated capabilities: text,code,function_calling,vision
        #[arg(long, default_value = "text")]
        capabilities: String,
        #[arg(long, default_value = "200000")] context_window: u64,
        /// Mark as local/self-hosted model
        #[arg(long)] local: bool,
        #[arg(long, default_value = "1")] priority: i32,
    },
    /// Remove a model from the registry
    Remove { model_id: String },
    /// Get a routing recommendation for a tenant
    Route {
        tenant: String,
        /// Estimated input tokens
        #[arg(long, default_value = "1000")] input_tokens:  u64,
        /// Estimated output tokens
        #[arg(long, default_value = "500")]  output_tokens: u64,
        /// Max cost in USD
        #[arg(long)] budget: Option<f64>,
        /// Max acceptable latency in ms
        #[arg(long)] max_latency: Option<u64>,
        /// Require a local model
        #[arg(long)] local: bool,
        /// Comma-separated required capabilities
        #[arg(long)] capabilities: Option<String>,
        /// Preferred model ID (override)
        #[arg(long)] prefer: Option<String>,
    },
    /// Show per-model usage and cost for a tenant
    Usage { tenant: String },
}

pub fn run(cmd: ModelsCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        ModelsCmd::List => {
            let models = client.models_list()?;
            if models.is_empty() {
                fmt::info("No models registered. Use 'apollo models add' to register one.");
                return Ok(());
            }
            fmt::section("Registered Models");
            fmt::print_table(
                &["MODEL ID", "PROVIDER", "COST IN $/M", "COST OUT $/M", "LATENCY p50", "LOCAL", "AVAIL"],
                &models.iter().map(|m| vec![
                    m.model_id.clone(),
                    m.provider.clone(),
                    format!("{:.2}", m.cost_per_m_input),
                    format!("{:.2}", m.cost_per_m_output),
                    format!("{}ms", m.latency_p50_ms),
                    if m.is_local { "yes".into() } else { "no".into() },
                    if m.is_available { "✓".into() } else { "✗".into() },
                ]).collect::<Vec<_>>(),
            );
        }

        ModelsCmd::Add {
            model_id, provider, cost_input, cost_output,
            latency_p50, latency_p99, throughput, capabilities,
            context_window, local, priority,
        } => {
            use std::time::{SystemTime, UNIX_EPOCH};
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
            let caps: Vec<String> = capabilities.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let rec = ModelRecord {
                model_id:          model_id.clone(),
                provider,
                endpoint:          None,
                cost_per_m_input:  cost_input,
                cost_per_m_output: cost_output,
                latency_p50_ms:    latency_p50,
                latency_p99_ms:    latency_p99,
                throughput_tok_s:  throughput,
                capabilities:      caps,
                context_window,
                is_local:          local,
                is_available:      true,
                priority,
                registered_at:     now,
                updated_at:        now,
            };
            client.model_put(&model_id, &rec)?;
            fmt::ok(&format!("Registered model '{}'", model_id));
        }

        ModelsCmd::Remove { model_id } => {
            client.model_delete(&model_id)?;
            fmt::ok(&format!("Removed model '{}'", model_id));
        }

        ModelsCmd::Route {
            tenant, input_tokens, output_tokens, budget,
            max_latency, local, capabilities, prefer,
        } => {
            let caps: Vec<String> = capabilities.unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let req = RoutingRequest {
                tenant_id:             tenant.clone(),
                input_tokens:          Some(input_tokens),
                output_tokens:         Some(output_tokens),
                max_cost_usd:          budget,
                max_latency_ms:        max_latency,
                require_local:         local,
                required_capabilities: caps,
                preferred_model:       prefer,
            };
            let decision = client.models_route(&req)?;
            fmt::section("Routing Decision");
            fmt::kv("Selected model", &decision.selected_model);
            fmt::kv("Provider",       &decision.provider);
            fmt::kv("Est. cost (USD)", &decision.estimated_cost
                .map(|c| format!("${:.4}", c))
                .unwrap_or_else(|| "—".into()));
            fmt::kv("Reason",         &decision.reason);
            if !decision.fallback_models.is_empty() {
                fmt::kv("Fallbacks", &decision.fallback_models.join(", "));
            }
        }

        ModelsCmd::Usage { tenant } => {
            let record = client.models_usage(&tenant)?;
            fmt::section(&format!("Model Usage — tenant: {}", tenant));
            fmt::kv("Total cost (USD)",    &format!("${:.4}", record.total_cost_usd));
            fmt::kv("Total input tokens",  &record.total_input_tokens.to_string());
            fmt::kv("Total output tokens", &record.total_output_tokens.to_string());
            if !record.usage_by_model.is_empty() {
                println!();
                fmt::print_table(
                    &["MODEL", "CALLS", "INPUT TOKENS", "OUTPUT TOKENS", "COST $"],
                    &record.usage_by_model.values().map(|s| vec![
                        s.model_id.clone(),
                        s.call_count.to_string(),
                        s.input_tokens.to_string(),
                        s.output_tokens.to_string(),
                        format!("{:.4}", s.cost_usd),
                    ]).collect::<Vec<_>>(),
                );
            }
        }
    }
    Ok(())
}
