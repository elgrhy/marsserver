//! CLI commands for the agent memory layer.

use anyhow::Result;
use clap::Subcommand;
use apollo_core::memory::MemoryQuery;
use crate::client::NodeClient;
use crate::fmt;

#[derive(Subcommand)]
pub enum MemoryCmd {
    /// Retrieve a memory entry by key
    Get {
        tenant: String,
        agent: String,
        /// Memory entry key
        #[arg(value_name = "KEY")]
        entry_key: String,
    },
    /// Store or update a memory entry
    Put {
        tenant: String,
        agent:  String,
        /// Memory entry key
        #[arg(value_name = "KEY")]
        entry_key: String,
        /// JSON value to store (string, object, array, number)
        value:  String,
        /// TTL in seconds (omit for no expiry)
        #[arg(long)]
        ttl: Option<u64>,
        /// Comma-separated tags
        #[arg(long)]
        tags: Option<String>,
        /// Optional text for similarity search
        #[arg(long)]
        text: Option<String>,
    },
    /// Delete a memory entry
    Delete {
        tenant: String,
        agent: String,
        /// Memory entry key
        #[arg(value_name = "KEY")]
        entry_key: String,
    },
    /// List all live memory keys
    List { tenant: String, agent: String },
    /// Similarity search over memory
    Search {
        tenant: String,
        agent:  String,
        query:  String,
        #[arg(long, default_value = "10")]
        limit: usize,
        /// Comma-separated required tags
        #[arg(long)]
        tags: Option<String>,
    },
    /// Clear all memory for a tenant+agent
    Clear { tenant: String, agent: String },
    /// Show memory usage statistics
    Stats { tenant: String, agent: String },
}

pub fn run(cmd: MemoryCmd, client: &NodeClient) -> Result<()> {
    match cmd {
        MemoryCmd::Get { tenant, agent, entry_key } => {
            let entry = client.memory_get(&tenant, &agent, &entry_key)?;
            fmt::section(&format!("Memory: {}/{}/{}", tenant, agent, entry_key));
            fmt::kv("Key",        &entry.key);
            fmt::kv("Value",      &serde_json::to_string_pretty(&entry.value).unwrap_or_default());
            fmt::kv("Tags",       &entry.tags.join(", "));
            fmt::kv("Created",    &entry.created_at.to_string());
            fmt::kv("Updated",    &entry.updated_at.to_string());
            fmt::kv("TTL (secs)", &entry.ttl_secs.map(|t| t.to_string()).unwrap_or_else(|| "∞".into()));
            if let Some(ref t) = entry.text {
                fmt::kv("Search text", t);
            }
        }

        MemoryCmd::Put { tenant, agent, entry_key, value, ttl, tags, text } => {
            let parsed_value: serde_json::Value = serde_json::from_str(&value)
                .unwrap_or_else(|_| serde_json::Value::String(value.clone()));
            let tag_list: Vec<String> = tags
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let body = serde_json::json!({
                "value": parsed_value,
                "tags": tag_list,
                "text": text,
                "ttl_secs": ttl,
            });
            client.memory_put(&tenant, &agent, &entry_key, &body)?;
            fmt::ok(&format!("Stored memory key '{}' for {}/{}", entry_key, tenant, agent));
        }

        MemoryCmd::Delete { tenant, agent, entry_key } => {
            client.memory_delete(&tenant, &agent, &entry_key)?;
            fmt::ok(&format!("Deleted memory key '{}'", entry_key));
        }

        MemoryCmd::List { tenant, agent } => {
            let keys = client.memory_list(&tenant, &agent)?;
            if keys.is_empty() {
                fmt::info("No memory entries found.");
                return Ok(());
            }
            fmt::section(&format!("Memory keys — {}/{}", tenant, agent));
            for (i, k) in keys.iter().enumerate() {
                println!("  {:>3}.  {}", i + 1, k);
            }
        }

        MemoryCmd::Search { tenant, agent, query, limit, tags } => {
            let tag_list: Vec<String> = tags
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let q = MemoryQuery { query, tags: tag_list, limit, min_score: 0.0, embedding: None };
            let results = client.memory_search(&tenant, &agent, &q)?;
            if results.is_empty() {
                fmt::info("No matching memory entries.");
                return Ok(());
            }
            fmt::section("Memory Search Results");
            fmt::print_table(
                &["KEY", "SCORE", "TAGS", "VALUE PREVIEW"],
                &results.iter().map(|r| vec![
                    r.key.clone(),
                    format!("{:.2}", r.score),
                    r.entry.tags.join(", "),
                    value_preview(&r.entry.value),
                ]).collect::<Vec<_>>(),
            );
        }

        MemoryCmd::Clear { tenant, agent } => {
            client.memory_clear(&tenant, &agent)?;
            fmt::ok(&format!("Cleared all memory for {}/{}", tenant, agent));
        }

        MemoryCmd::Stats { tenant, agent } => {
            let s = client.memory_stats(&tenant, &agent)?;
            fmt::section(&format!("Memory Stats — {}/{}", tenant, agent));
            fmt::kv("Live entries",  &s.live_entries.to_string());
            fmt::kv("Total entries", &s.total_entries.to_string());
            fmt::kv("Store size",    &format!("{} bytes", s.total_bytes));
        }
    }
    Ok(())
}

fn value_preview(v: &serde_json::Value) -> String {
    let s = v.to_string();
    if s.len() > 40 { format!("{}…", &s[..40]) } else { s }
}
