//! Unit tests for all Apollo v2.0 platform modules.
//!
//! Run with: `cargo test -p apollo-core`

#[cfg(test)]
mod tracing_tests {
    use crate::tracing::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn test_span_append_and_load() {
        let dir = temp();
        let base = dir.path();
        let span = TraceSpan {
            span_id:        new_id(),
            trace_id:       new_id(),
            parent_span_id: None,
            tenant_id:      "tenant1".into(),
            agent_id:       "agent1".into(),
            name:           "llm_inference".into(),
            attributes:     HashMap::new(),
            events:         vec![],
            token_usage:    Some(TokenUsage {
                model:         "claude-sonnet-4-6".into(),
                input_tokens:  100,
                output_tokens: 50,
                cost_usd:      0.0015,
                provider:      "anthropic".into(),
            }),
            status:         SpanStatus::Ok,
            error:          None,
            start_ts_ms:    now_ms() - 1000,
            end_ts_ms:      Some(now_ms()),
            duration_ms:    Some(1000),
        };

        append_span(base, &span).unwrap();
        let spans = load_trace(base, "tenant1", "agent1", &span.trace_id);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].span_id, span.span_id);
        assert_eq!(spans[0].token_usage.as_ref().unwrap().input_tokens, 100);
    }

    #[test]
    fn test_upsert_span_updates_existing() {
        let dir = temp();
        let base = dir.path();
        let trace_id = new_id();
        let span_id  = new_id();

        let mut span = TraceSpan {
            span_id:        span_id.clone(),
            trace_id:       trace_id.clone(),
            parent_span_id: None,
            tenant_id:      "t1".into(),
            agent_id:       "a1".into(),
            name:           "step".into(),
            attributes:     HashMap::new(),
            events:         vec![],
            token_usage:    None,
            status:         SpanStatus::Running,
            error:          None,
            start_ts_ms:    now_ms(),
            end_ts_ms:      None,
            duration_ms:    None,
        };

        upsert_span(base, &span).unwrap();
        span.status    = SpanStatus::Ok;
        span.end_ts_ms = Some(now_ms());
        upsert_span(base, &span).unwrap();

        let spans = load_trace(base, "t1", "a1", &trace_id);
        assert_eq!(spans.len(), 1, "Should not duplicate the span");
        assert_eq!(spans[0].status, SpanStatus::Ok);
    }

    #[test]
    fn test_finalize_trace_builds_summary() {
        let dir = temp();
        let base = dir.path();
        let trace_id = new_id();

        for _ in 0..3 {
            let span = TraceSpan {
                span_id:        new_id(),
                trace_id:       trace_id.clone(),
                parent_span_id: None,
                tenant_id:      "t1".into(),
                agent_id:       "a1".into(),
                name:           "step".into(),
                attributes:     HashMap::new(),
                events:         vec![],
                token_usage:    Some(TokenUsage {
                    model: "gpt-4o".into(),
                    input_tokens: 10,
                    output_tokens: 5,
                    cost_usd: 0.001,
                    provider: "openai".into(),
                }),
                status:    SpanStatus::Ok,
                error:     None,
                start_ts_ms: now_ms(),
                end_ts_ms:   Some(now_ms() + 500),
                duration_ms: Some(500),
            };
            append_span(base, &span).unwrap();
        }

        let summary = finalize_trace(base, "t1", "a1", &trace_id).unwrap();
        assert_eq!(summary.span_count, 3);
        assert!((summary.total_cost_usd - 0.003).abs() < 1e-9);
        assert_eq!(summary.status, SpanStatus::Ok);
    }
}

#[cfg(test)]
mod policy_tests {
    use crate::policy::*;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn test_default_policy_allows_all() {
        let dir = temp();
        let engine = PolicyEngine::new(dir.path().to_path_buf());

        // No policy set → everything allowed
        let decision = engine.check_run("tenant1", "openclaw", "us-east-1", 0);
        assert!(matches!(decision, PolicyDecision::Allow));
    }

    #[test]
    fn test_blocked_agent_is_denied() {
        let dir = temp();
        let base = dir.path();
        let engine = PolicyEngine::new(base.to_path_buf());

        let policy = TenantPolicy {
            tenant_id:     "tenant1".into(),
            blocked_agents: vec!["dangerous-agent".into()],
            ..Default::default()
        };
        save_policy(base, &policy).unwrap();

        let decision = engine.check_run("tenant1", "dangerous-agent", "us-east-1", 0);
        assert!(matches!(decision, PolicyDecision::Deny { code: PolicyViolation::AgentBlocked, .. }));
    }

    #[test]
    fn test_allowed_agents_whitelist() {
        let dir = temp();
        let base = dir.path();
        let engine = PolicyEngine::new(base.to_path_buf());

        let policy = TenantPolicy {
            tenant_id:     "tenant1".into(),
            allowed_agents: Some(vec!["trusted-agent".into()]),
            ..Default::default()
        };
        save_policy(base, &policy).unwrap();

        let ok = engine.check_run("tenant1", "trusted-agent", "us-east-1", 0);
        assert!(matches!(ok, PolicyDecision::Allow));

        let deny = engine.check_run("tenant1", "other-agent", "us-east-1", 0);
        assert!(matches!(deny, PolicyDecision::Deny { code: PolicyViolation::AgentNotAllowed, .. }));
    }

    #[test]
    fn test_region_residency_constraint() {
        let dir = temp();
        let base = dir.path();
        let engine = PolicyEngine::new(base.to_path_buf());

        let policy = TenantPolicy {
            tenant_id:     "tenant1".into(),
            data_residency: Some("eu-west-1".into()),
            ..Default::default()
        };
        save_policy(base, &policy).unwrap();

        let deny = engine.check_run("tenant1", "agent", "us-east-1", 0);
        assert!(matches!(deny, PolicyDecision::Deny { code: PolicyViolation::RegionMismatch, .. }));

        let ok = engine.check_run("tenant1", "agent", "eu-west-1", 0);
        assert!(matches!(ok, PolicyDecision::Allow));
    }

    #[test]
    fn test_capacity_limit() {
        let dir = temp();
        let base = dir.path();
        let engine = PolicyEngine::new(base.to_path_buf());

        let policy = TenantPolicy {
            tenant_id:    "tenant1".into(),
            max_instances: Some(2),
            ..Default::default()
        };
        save_policy(base, &policy).unwrap();

        assert!(matches!(engine.check_run("tenant1", "a", "us-east-1", 1), PolicyDecision::Allow));
        assert!(matches!(engine.check_run("tenant1", "a", "us-east-1", 2), PolicyDecision::Deny { code: PolicyViolation::CapacityExceeded, .. }));
    }

    #[test]
    fn test_tool_policy() {
        let dir = temp();
        let base = dir.path();
        let engine = PolicyEngine::new(base.to_path_buf());

        let policy = TenantPolicy {
            tenant_id:    "tenant1".into(),
            blocked_tools: vec!["bash".into(), "file_write".into()],
            ..Default::default()
        };
        save_policy(base, &policy).unwrap();

        let deny = engine.check_tool("tenant1", "bash");
        assert!(matches!(deny, PolicyDecision::Deny { .. }));

        let ok = engine.check_tool("tenant1", "http_get");
        assert!(matches!(ok, PolicyDecision::Allow));
    }

    #[test]
    fn test_compliance_report() {
        let dir = temp();
        let base = dir.path();

        let policy = TenantPolicy {
            tenant_id:      "t1".into(),
            data_residency: Some("eu-west-1".into()),
            require_audit:  true,
            max_instances:  Some(5),
            ..Default::default()
        };
        save_policy(base, &policy).unwrap();

        let report = compliance_report(base, "t1");
        assert!(report.policy_active);
        assert_eq!(report.data_residency.as_deref(), Some("eu-west-1"));
        assert!(report.require_audit);
        assert_eq!(report.max_instances, Some(5));
    }
}

#[cfg(test)]
mod health_tests {
    use crate::health::*;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn test_initial_health_score_is_100() {
        let rec = HealthRecord::new("t1", "a1", "inst-1");
        assert_eq!(rec.health_score, 100.0);
        assert_eq!(rec.status, HealthStatus::Unknown);
    }

    #[test]
    fn test_crash_reduces_score() {
        let mut rec = HealthRecord::new("t1", "a1", "inst-1");
        rec.record_crash(Some(1), None, false);
        assert!(rec.health_score < 100.0);
        assert!(rec.crash_count == 1);
    }

    #[test]
    fn test_multiple_crashes_degrade_status() {
        let mut rec = HealthRecord::new("t1", "a1", "inst-1");
        for _ in 0..4 {
            rec.record_crash(Some(1), None, false);
        }
        assert!(rec.health_score <= 40.0);
        assert!(matches!(rec.status, HealthStatus::Critical | HealthStatus::Degraded));
    }

    #[test]
    fn test_normal_samples_maintain_health() {
        let mut rec = HealthRecord::new("t1", "a1", "inst-1");
        for _ in 0..10 {
            rec.record_sample(10.0, 128, Some(50));
        }
        assert!(rec.health_score >= 80.0);
        assert_eq!(rec.status, HealthStatus::Healthy);
    }

    #[test]
    fn test_high_cpu_reduces_score() {
        let mut rec = HealthRecord::new("t1", "a1", "inst-1");
        for _ in 0..5 {
            rec.record_sample(98.0, 128, None);
        }
        assert!(rec.health_score < 100.0);
    }

    #[test]
    fn test_health_persistence() {
        let dir = temp();
        let base = dir.path();
        let mut rec = HealthRecord::new("tenant1", "myagent", "inst-42");
        rec.record_sample(25.0, 256, Some(100));
        save_health(base, &rec).unwrap();

        let loaded = load_health(base, "tenant1", "myagent").unwrap();
        assert_eq!(loaded.instance_id, "inst-42");
        assert_eq!(loaded.avg_memory_mb, 256);
    }

    #[test]
    fn test_fleet_health_summary() {
        let dir = temp();
        let base = dir.path();

        let mut r1 = HealthRecord::new("t1", "a1", "i1");
        r1.record_sample(10.0, 100, None);
        save_health(base, &r1).unwrap();

        let mut r2 = HealthRecord::new("t1", "a2", "i2");
        for _ in 0..5 { r2.record_crash(Some(1), None, false); }
        save_health(base, &r2).unwrap();

        let summary = fleet_health_summary(base);
        assert_eq!(summary.total_instances, 2);
        assert!(summary.avg_health_score < 100.0);
    }

    #[test]
    fn test_oom_pattern_detection() {
        let mut rec = HealthRecord::new("t1", "a1", "i1");
        rec.record_crash(None, None, true);
        rec.record_crash(None, None, true);
        assert_eq!(rec.crash_pattern.as_deref(), Some("oom_loop"));
    }
}

#[cfg(test)]
mod memory_tests {
    use crate::memory::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    #[test]
    fn test_put_and_get_memory() {
        let dir = temp();
        let base = dir.path();

        let entry = put_memory(
            base, "t1", "a1", "user_pref",
            json!({"theme": "dark", "language": "en"}),
            vec!["preferences".to_string()],
            Some("user prefers dark theme".to_string()),
            None,
        ).unwrap();

        assert_eq!(entry.key, "user_pref");

        let loaded = get_memory(base, "t1", "a1", "user_pref").unwrap();
        assert_eq!(loaded.value["theme"], "dark");
    }

    #[test]
    fn test_delete_memory() {
        let dir = temp();
        let base = dir.path();

        put_memory(base, "t1", "a1", "key1", json!("value"), vec![], None, None).unwrap();
        let removed = delete_memory(base, "t1", "a1", "key1").unwrap();
        assert!(removed);

        let none = get_memory(base, "t1", "a1", "key1");
        assert!(none.is_none());
    }

    #[test]
    fn test_memory_search_finds_relevant_entries() {
        let dir = temp();
        let base = dir.path();

        put_memory(base, "t1", "a1", "python_info",
            json!("Python 3.12 was released"),
            vec!["programming".to_string()],
            Some("Python 3.12 release notes".to_string()),
            None,
        ).unwrap();

        put_memory(base, "t1", "a1", "rust_info",
            json!("Rust 1.80 stable"),
            vec!["programming".to_string()],
            Some("Rust 1.80 stable release".to_string()),
            None,
        ).unwrap();

        put_memory(base, "t1", "a1", "food_fact",
            json!("Pizza is a popular dish"),
            vec!["food".to_string()],
            Some("Pizza origin story".to_string()),
            None,
        ).unwrap();

        let query = MemoryQuery {
            query: "Python programming release".to_string(),
            tags:  vec![],
            limit: 5,
            min_score: 0.0,
        };

        let results = search_memory(base, "t1", "a1", &query);
        assert!(!results.is_empty());
        assert_eq!(results[0].key, "python_info",
            "Python result should rank highest for 'Python programming release'");
    }

    #[test]
    fn test_ttl_expiry() {
        let dir = temp();
        let base = dir.path();

        put_memory(base, "t1", "a1", "ephemeral",
            json!("short lived"),
            vec![],
            None,
            Some(0),  // expires immediately
        ).unwrap();

        // TTL of 0 means expires at created_at, so it's already expired
        let result = get_memory(base, "t1", "a1", "ephemeral");
        assert!(result.is_none(), "Entry with TTL=0 should be expired");
    }

    #[test]
    fn test_list_keys() {
        let dir = temp();
        let base = dir.path();

        for i in 0..5 {
            put_memory(base, "t1", "a1", &format!("key{}", i),
                json!(i), vec![], None, None).unwrap();
        }

        let keys = list_memory_keys(base, "t1", "a1");
        assert_eq!(keys.len(), 5);
    }

    #[test]
    fn test_clear_memory() {
        let dir = temp();
        let base = dir.path();

        put_memory(base, "t1", "a1", "k1", json!(1), vec![], None, None).unwrap();
        put_memory(base, "t1", "a1", "k2", json!(2), vec![], None, None).unwrap();
        clear_memory(base, "t1", "a1").unwrap();

        let keys = list_memory_keys(base, "t1", "a1");
        assert!(keys.is_empty());
    }

    #[test]
    fn test_tag_filter_in_search() {
        let dir = temp();
        let base = dir.path();

        put_memory(base, "t1", "a1", "docs_entry",
            json!("technical documentation"),
            vec!["docs".to_string(), "tech".to_string()],
            Some("API reference guide".to_string()),
            None,
        ).unwrap();

        put_memory(base, "t1", "a1", "blog_entry",
            json!("casual post"),
            vec!["blog".to_string()],
            Some("API blog post".to_string()),
            None,
        ).unwrap();

        let query = MemoryQuery {
            query: "API".to_string(),
            tags:  vec!["docs".to_string()],  // only docs entries
            limit: 10,
            min_score: 0.0,
        };

        let results = search_memory(base, "t1", "a1", &query);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "docs_entry");
    }
}

#[cfg(test)]
mod model_router_tests {
    use crate::model_router::*;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    fn make_model(id: &str, priority: i32, cost: f64, latency: u64) -> ModelRecord {
        ModelRecord {
            model_id:         id.to_string(),
            provider:         "test".to_string(),
            endpoint:         None,
            cost_per_m_input:  cost,
            cost_per_m_output: cost,
            latency_p50_ms:   latency,
            latency_p99_ms:   latency * 2,
            throughput_tok_s: 100.0,
            capabilities:     vec!["text".to_string()],
            context_window:   128_000,
            is_local:         false,
            is_available:     true,
            priority,
            registered_at:    0,
            updated_at:       0,
        }
    }

    #[test]
    fn test_register_and_list() {
        let dir = temp();
        let base = dir.path();

        register_model(base, make_model("m1", 1, 0.5, 200)).unwrap();
        register_model(base, make_model("m2", 2, 1.0, 100)).unwrap();

        let models = load_model_registry(base);
        assert_eq!(models.len(), 2);
    }

    #[test]
    fn test_route_selects_lowest_priority() {
        let dir = temp();
        let base = dir.path();

        register_model(base, make_model("cheap-fast", 1, 0.5, 100)).unwrap();
        register_model(base, make_model("expensive-slow", 10, 5.0, 1000)).unwrap();

        let router = ModelRouter::new(base.to_path_buf());
        let req = RoutingRequest {
            tenant_id:             "t1".to_string(),
            preferred_model:       None,
            max_cost_usd:          None,
            input_tokens:          None,
            output_tokens:         None,
            required_capabilities: vec![],
            require_local:         false,
            max_latency_ms:        None,
        };

        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, "cheap-fast");
    }

    #[test]
    fn test_route_respects_preferred_model() {
        let dir = temp();
        let base = dir.path();

        register_model(base, make_model("m1", 1, 0.5, 100)).unwrap();
        register_model(base, make_model("m2", 2, 5.0, 500)).unwrap();

        let router = ModelRouter::new(base.to_path_buf());
        let req = RoutingRequest {
            tenant_id:             "t1".to_string(),
            preferred_model:       Some("m2".to_string()),
            max_cost_usd:          None,
            input_tokens:          None,
            output_tokens:         None,
            required_capabilities: vec![],
            require_local:         false,
            max_latency_ms:        None,
        };

        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, "m2");
    }

    #[test]
    fn test_route_filters_by_latency() {
        let dir = temp();
        let base = dir.path();

        let mut slow_model = make_model("slow", 1, 0.1, 2000);
        slow_model.latency_p50_ms = 2000;
        register_model(base, slow_model).unwrap();

        let mut fast_model = make_model("fast", 2, 1.0, 100);
        fast_model.latency_p50_ms = 100;
        register_model(base, fast_model).unwrap();

        let router = ModelRouter::new(base.to_path_buf());
        let req = RoutingRequest {
            tenant_id:             "t1".to_string(),
            preferred_model:       None,
            max_cost_usd:          None,
            input_tokens:          None,
            output_tokens:         None,
            required_capabilities: vec![],
            require_local:         false,
            max_latency_ms:        Some(500),
        };

        let decision = router.route(&req).unwrap();
        assert_eq!(decision.selected_model, "fast");
    }

    #[test]
    fn test_remove_model() {
        let dir = temp();
        let base = dir.path();

        register_model(base, make_model("m1", 1, 0.5, 100)).unwrap();
        remove_model(base, "m1").unwrap();

        let models = load_model_registry(base);
        assert!(models.is_empty());
    }

    #[test]
    fn test_usage_recording() {
        let dir = temp();
        let base = dir.path();

        record_model_usage(base, "tenant1", "claude-sonnet-4-6", 1000, 500, 0.01, 200).unwrap();
        record_model_usage(base, "tenant1", "claude-sonnet-4-6", 2000, 800, 0.02, 180).unwrap();

        let usage = load_model_usage(base, "tenant1");
        assert_eq!(usage.usage_by_model["claude-sonnet-4-6"].call_count, 2);
        assert!((usage.total_cost_usd - 0.03).abs() < 1e-9);
    }
}

#[cfg(test)]
mod scheduler_tests {
    use crate::scheduler::{
        ScheduledJob, Schedule, load_job, create_job, save_jobs,
        delete_job, mark_fired, due_jobs,
    };
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    fn make_job(tenant: &str, agent: &str, schedule: Schedule) -> ScheduledJob {
        ScheduledJob {
            id:         String::new(),
            name:       "test-job".to_string(),
            tenant_id:  tenant.to_string(),
            agent_id:   agent.to_string(),
            extra_env:  HashMap::new(),
            schedule,
            enabled:    true,
            created_at: 0,
            updated_at: 0,
            last_run:   None,
            next_run:   0,
            run_count:  0,
            fail_count: 0,
        }
    }

    #[test]
    fn test_create_and_load_job() {
        let dir = temp();
        let base = dir.path();

        let job = create_job(base, make_job(
            "t1", "openclaw",
            Schedule::Interval { secs: 300 },
        )).unwrap();

        assert!(!job.id.is_empty());
        assert!(job.next_run > 0);

        let loaded = load_job(base, &job.id).unwrap();
        assert_eq!(loaded.agent_id, "openclaw");
    }

    #[test]
    fn test_delete_job() {
        let dir = temp();
        let base = dir.path();

        let job = create_job(base, make_job(
            "t1", "a1",
            Schedule::Once { at: u64::MAX },
        )).unwrap();

        delete_job(base, &job.id).unwrap();
        assert!(load_job(base, &job.id).is_none());
    }

    #[test]
    fn test_interval_schedule_next_after() {
        let sched = Schedule::Interval { secs: 60 };
        let now = 1_000_000u64;
        let next = sched.next_after(now).unwrap();
        assert_eq!(next, now + 60);
    }

    #[test]
    fn test_once_schedule_fires_once() {
        let future_ts = 9_999_999_999u64;
        let sched = Schedule::Once { at: future_ts };
        let now = 1_000u64;
        assert_eq!(sched.next_after(now).unwrap(), future_ts);
        // Past the scheduled time — no more fires
        assert!(sched.next_after(future_ts).is_none());
    }

    #[test]
    fn test_cron_hourly() {
        // "0 * * * *" — every hour at minute 0
        let sched = Schedule::Cron { expression: "0 * * * *".to_string() };
        let now = 1_700_000_000u64; // some unix timestamp
        let next = sched.next_after(now).unwrap();
        assert!(next > now, "Next fire should be in the future");
        assert!(next - now <= 3600, "Next fire within an hour");
    }

    #[test]
    fn test_due_jobs_returns_ready() {
        use crate::scheduler::save_jobs;
        let dir = temp();
        let base = dir.path();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Build jobs manually so next_run is exactly what we want
        let overdue = ScheduledJob {
            id: "job-overdue".to_string(),
            name: "overdue".to_string(),
            tenant_id: "t1".to_string(),
            agent_id: "a1".to_string(),
            extra_env: HashMap::new(),
            schedule: Schedule::Interval { secs: 60 },
            enabled: true,
            created_at: now,
            updated_at: now,
            last_run: None,
            next_run: 1,  // far in the past — overdue
            run_count: 0,
            fail_count: 0,
        };

        let future = ScheduledJob {
            id: "job-future".to_string(),
            name: "future".to_string(),
            tenant_id: "t1".to_string(),
            agent_id: "a1".to_string(),
            extra_env: HashMap::new(),
            schedule: Schedule::Interval { secs: 86400 },
            enabled: true,
            created_at: now,
            updated_at: now,
            last_run: None,
            next_run: u64::MAX,  // far in the future
            run_count: 0,
            fail_count: 0,
        };

        save_jobs(base, &[overdue, future]).unwrap();

        let due = due_jobs(base, now);
        assert_eq!(due.len(), 1, "Only the overdue job should be returned");
        assert_eq!(due[0].id, "job-overdue");
    }

    #[test]
    fn test_mark_fired_updates_state() {
        let dir = temp();
        let base = dir.path();

        let job = create_job(base, make_job(
            "t1", "a1",
            Schedule::Interval { secs: 60 },
        )).unwrap();

        mark_fired(base, &job.id, true, None).unwrap();

        let updated = load_job(base, &job.id).unwrap();
        assert_eq!(updated.run_count, 1);
        assert!(updated.last_run.is_some());
    }
}

#[cfg(test)]
mod orchestration_tests {
    use crate::orchestration::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    // ── Blueprint tests ───────────────────────────────────────────────────────

    #[test]
    fn test_blueprint_crud() {
        let dir = temp();
        let base = dir.path();

        let bp = Blueprint {
            id:               String::new(),
            name:             "My Blueprint".to_string(),
            description:      "A test blueprint".to_string(),
            agent_id:         "openclaw".to_string(),
            pin_version:      Some("1.0.0".to_string()),
            default_env:      HashMap::new(),
            cpu_override:     None,
            memory_override:  None,
            timeout_override: None,
            tags:             vec!["prod".to_string()],
            region:           Some("us-east-1".to_string()),
            created_at:       0,
            updated_at:       0,
        };

        let saved = save_blueprint(base, bp).unwrap();
        assert!(!saved.id.is_empty());
        assert!(saved.created_at > 0);

        let loaded = load_blueprint(base, &saved.id).unwrap();
        assert_eq!(loaded.agent_id, "openclaw");
        assert_eq!(loaded.tags, vec!["prod"]);

        delete_blueprint(base, &saved.id).unwrap();
        assert!(load_blueprint(base, &saved.id).is_none());
    }

    #[test]
    fn test_list_blueprints() {
        let dir = temp();
        let base = dir.path();

        for i in 0..3 {
            save_blueprint(base, Blueprint {
                id: String::new(), name: format!("bp-{}", i),
                description: String::new(), agent_id: "a".to_string(),
                pin_version: None, default_env: HashMap::new(),
                cpu_override: None, memory_override: None, timeout_override: None,
                tags: vec![], region: None, created_at: 0, updated_at: 0,
            }).unwrap();
        }

        let bps = list_blueprints(base);
        assert_eq!(bps.len(), 3);
    }

    // ── Agent Group tests ─────────────────────────────────────────────────────

    #[test]
    fn test_group_crud() {
        let dir = temp();
        let base = dir.path();

        let group = AgentGroup {
            id:          String::new(),
            name:        "My Group".to_string(),
            description: "Test group".to_string(),
            tenant_id:   "tenant1".to_string(),
            members:     vec![
                GroupMember { agent_id: "a1".to_string(), blueprint_id: None, tenant_override: None },
                GroupMember { agent_id: "a2".to_string(), blueprint_id: None, tenant_override: None },
            ],
            status:      GroupStatus::Idle,
            created_at:  0,
            updated_at:  0,
        };

        let saved = save_group(base, group).unwrap();
        assert_eq!(saved.members.len(), 2);

        set_group_status(base, &saved.id, GroupStatus::Running).unwrap();
        let loaded = load_group(base, &saved.id).unwrap();
        assert_eq!(loaded.status, GroupStatus::Running);

        delete_group(base, &saved.id).unwrap();
        assert!(load_group(base, &saved.id).is_none());
    }

    // ── Workflow tests ────────────────────────────────────────────────────────

    #[test]
    fn test_workflow_definition_crud() {
        let dir = temp();
        let base = dir.path();

        let def = WorkflowDef {
            id:          String::new(),
            name:        "ETL Pipeline".to_string(),
            description: "Extract, transform, load".to_string(),
            tenant_id:   "t1".to_string(),
            steps: vec![
                WorkflowStep {
                    step_id:      "extract".to_string(),
                    name:         "Extract".to_string(),
                    agent_id:     "extractor".to_string(),
                    depends_on:   vec![],
                    env:          HashMap::new(),
                    optional:     false,
                    timeout_secs: Some(300),
                },
                WorkflowStep {
                    step_id:      "transform".to_string(),
                    name:         "Transform".to_string(),
                    agent_id:     "transformer".to_string(),
                    depends_on:   vec!["extract".to_string()],
                    env:          HashMap::new(),
                    optional:     false,
                    timeout_secs: None,
                },
            ],
            created_at: 0,
            updated_at: 0,
        };

        let saved = save_workflow_def(base, def).unwrap();
        assert_eq!(saved.steps.len(), 2);

        let loaded = load_workflow_def(base, &saved.id).unwrap();
        assert_eq!(loaded.name, "ETL Pipeline");
    }

    #[test]
    fn test_workflow_run_creation() {
        let dir = temp();
        let base = dir.path();

        let def = save_workflow_def(base, WorkflowDef {
            id: String::new(), name: "test".to_string(),
            description: String::new(), tenant_id: "t1".to_string(),
            steps: vec![
                WorkflowStep {
                    step_id: "s1".to_string(), name: "Step 1".to_string(),
                    agent_id: "a1".to_string(), depends_on: vec![],
                    env: HashMap::new(), optional: false, timeout_secs: None,
                },
            ],
            created_at: 0, updated_at: 0,
        }).unwrap();

        let run = create_workflow_run(base, &def).unwrap();
        assert_eq!(run.workflow_id, def.id);
        assert_eq!(run.steps.len(), 1);
        assert_eq!(run.steps[0].status, StepStatus::Pending);
    }

    #[test]
    fn test_ready_steps_with_def() {
        let dir = temp();
        let base = dir.path();

        let def = WorkflowDef {
            id: "wf1".to_string(), name: "w".to_string(),
            description: String::new(), tenant_id: "t1".to_string(),
            steps: vec![
                WorkflowStep {
                    step_id: "a".to_string(), name: "A".to_string(),
                    agent_id: "ag".to_string(), depends_on: vec![],
                    env: HashMap::new(), optional: false, timeout_secs: None,
                },
                WorkflowStep {
                    step_id: "b".to_string(), name: "B".to_string(),
                    agent_id: "ag".to_string(), depends_on: vec!["a".to_string()],
                    env: HashMap::new(), optional: false, timeout_secs: None,
                },
            ],
            created_at: 0, updated_at: 0,
        };

        let run = create_workflow_run(base, &def).unwrap();

        // Initially only "a" is ready (no deps)
        let ready = ready_steps_with_def(&run, &def);
        assert_eq!(ready, vec!["a"], "Only root step should be ready initially");
    }
}

#[cfg(test)]
mod arch_selector_tests {
    use crate::arch_selector::*;
    use crate::orchestration::{WorkflowDef, WorkflowStep};
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn temp() -> TempDir { tempfile::tempdir().unwrap() }

    fn make_step(id: &str, agent: &str, deps: Vec<&str>, optional: bool, timeout: Option<u64>) -> WorkflowStep {
        WorkflowStep {
            step_id:      id.to_string(),
            name:         id.to_string(),
            agent_id:     agent.to_string(),
            depends_on:   deps.into_iter().map(|s| s.to_string()).collect(),
            env:          HashMap::new(),
            optional,
            timeout_secs: timeout,
        }
    }

    fn make_def(tenant: &str, steps: Vec<WorkflowStep>) -> WorkflowDef {
        WorkflowDef {
            id:          "wf-test".to_string(),
            name:        "Test Workflow".to_string(),
            description: String::new(),
            tenant_id:   tenant.to_string(),
            steps,
            created_at:  0,
            updated_at:  0,
        }
    }

    // ── analyse_dag tests ─────────────────────────────────────────────────────

    #[test]
    fn test_empty_workflow_gives_default_metrics() {
        let def = make_def("t1", vec![]);
        let m = analyse_dag(&def);
        assert_eq!(m.step_count, 0);
        assert_eq!(m.distinct_agents, 0);
        assert_eq!(m.max_parallel_width, 0);
    }

    #[test]
    fn test_single_step_dag() {
        let def = make_def("t1", vec![make_step("s1", "agent-a", vec![], false, Some(60))]);
        let m = analyse_dag(&def);
        assert_eq!(m.step_count, 1);
        assert_eq!(m.distinct_agents, 1);
        assert_eq!(m.max_parallel_width, 1);
        assert_eq!(m.critical_path_length, 1);
        assert_eq!(m.critical_path, vec!["s1"]);
        assert!(!m.has_branching);
        assert!(!m.has_fan_in);
        assert!(m.has_timeout_constraints);
        assert_eq!(m.optional_ratio, 0.0);
    }

    #[test]
    fn test_linear_chain_dag() {
        // a → b → c
        let def = make_def("t1", vec![
            make_step("a", "agent", vec![], false, None),
            make_step("b", "agent", vec!["a"], false, None),
            make_step("c", "agent", vec!["b"], false, None),
        ]);
        let m = analyse_dag(&def);
        assert_eq!(m.step_count, 3);
        assert_eq!(m.max_parallel_width, 1);   // no parallelism
        assert_eq!(m.critical_path_length, 3);
        assert!(!m.has_branching);
        assert!(!m.has_fan_in);
    }

    #[test]
    fn test_parallel_dag_width() {
        // root → [a, b, c] → join
        let def = make_def("t1", vec![
            make_step("root",  "ag", vec![], false, None),
            make_step("a",     "ag", vec!["root"], false, None),
            make_step("b",     "ag", vec!["root"], false, None),
            make_step("c",     "ag", vec!["root"], false, None),
            make_step("join",  "ag", vec!["a", "b", "c"], false, None),
        ]);
        let m = analyse_dag(&def);
        assert_eq!(m.step_count, 5);
        assert_eq!(m.max_parallel_width, 3, "Three steps at level 1 should give width 3");
        assert!(m.has_branching, "root has 3 successors");
        assert!(m.has_fan_in,    "join has 3 predecessors");
        assert_eq!(m.critical_path_length, 3, "root → a|b|c → join");
    }

    #[test]
    fn test_distinct_agents_count() {
        let def = make_def("t1", vec![
            make_step("s1", "scraper",    vec![], false, None),
            make_step("s2", "analyzer",   vec!["s1"], false, None),
            make_step("s3", "reporter",   vec!["s2"], false, None),
            make_step("s4", "scraper",    vec!["s3"], false, None), // reuse
        ]);
        let m = analyse_dag(&def);
        assert_eq!(m.distinct_agents, 3); // scraper, analyzer, reporter
    }

    #[test]
    fn test_optional_ratio() {
        let def = make_def("t1", vec![
            make_step("s1", "a", vec![], false, None),
            make_step("s2", "a", vec![], true,  None),
            make_step("s3", "a", vec![], true,  None),
            make_step("s4", "a", vec![], false, None),
        ]);
        let m = analyse_dag(&def);
        assert!((m.optional_ratio - 0.5).abs() < 1e-6, "2/4 optional = 0.5");
    }

    // ── score / architecture selection tests ──────────────────────────────────

    #[test]
    fn test_single_step_selects_deterministic() {
        let dir = temp();
        let def = make_def("t1", vec![
            make_step("s1", "agent-a", vec![], false, Some(30)),
        ]);
        let selector = ArchitectureSelector::new(dir.path().to_path_buf());
        let decision  = selector.select(&def, "us-east-1");

        assert_eq!(decision.architecture, ArchitectureType::Deterministic,
            "Single step with timeout should always be deterministic");
        assert!(decision.confidence > 0.4, "Confidence should be meaningful");
        assert!(!decision.reasoning.is_empty());
    }

    #[test]
    fn test_multi_agent_parallel_selects_multiagent() {
        let dir = temp();
        let def = make_def("t1", vec![
            make_step("fetch",    "scraper",  vec![], true, None),
            make_step("analyze",  "llm",      vec![], true, None),
            make_step("store",    "db-agent", vec![], true, None),
            make_step("notify",   "mailer",   vec!["fetch", "analyze", "store"], false, None),
        ]);
        let selector = ArchitectureSelector::new(dir.path().to_path_buf());
        let decision  = selector.select(&def, "us-east-1");

        assert_eq!(decision.architecture, ArchitectureType::MultiAgent,
            "4 distinct agents with parallelism should select multi-agent");
        assert_eq!(decision.config.max_concurrency, 3);
        assert!(!decision.config.fail_fast);
    }

    #[test]
    fn test_single_agent_multi_step_selects_singleagent() {
        let dir = temp();
        // 3-step sequential chain, all same agent, no parallelism
        let def = make_def("t1", vec![
            make_step("step1", "worker", vec![], false, None),
            make_step("step2", "worker", vec!["step1"], false, None),
            make_step("step3", "worker", vec!["step2"], false, None),
        ]);
        let selector = ArchitectureSelector::new(dir.path().to_path_buf());
        let decision  = selector.select(&def, "us-east-1");

        assert_eq!(decision.architecture, ArchitectureType::SingleAgent,
            "Same agent, 3 sequential steps should be single-agent");
        assert_eq!(decision.config.max_concurrency, 1);
    }

    #[test]
    fn test_config_critical_path_populated() {
        let dir = temp();
        let def = make_def("t1", vec![
            make_step("a", "agent", vec![], false, None),
            make_step("b", "agent", vec!["a"], false, None),
            make_step("c", "agent", vec!["b"], false, None),
        ]);
        let selector = ArchitectureSelector::new(dir.path().to_path_buf());
        let decision  = selector.select(&def, "us-east-1");

        assert_eq!(decision.config.critical_path.len(), 3);
        assert_eq!(decision.config.phase_count, 3); // 3 topological levels
    }

    #[test]
    fn test_suggested_timeout_sums_critical_path() {
        let dir = temp();
        let def = make_def("t1", vec![
            make_step("a", "agent", vec![], false, Some(100)),
            make_step("b", "agent", vec!["a"], false, Some(200)),
        ]);
        let selector = ArchitectureSelector::new(dir.path().to_path_buf());
        let decision  = selector.select(&def, "us-east-1");

        assert_eq!(decision.config.suggested_total_timeout_secs, Some(300));
    }

    #[test]
    fn test_governance_skip_candidates_populated() {
        use crate::policy::{TenantPolicy, save_policy};
        let dir = temp();
        let base = dir.path();

        // Block "risky-agent" via policy
        let policy = TenantPolicy {
            tenant_id:     "t1".to_string(),
            blocked_agents: vec!["risky-agent".to_string()],
            ..Default::default()
        };
        save_policy(base, &policy).unwrap();

        // Workflow with a risky but optional step
        let def = make_def("t1", vec![
            make_step("safe",  "safe-agent",  vec![], false, None),
            make_step("risky", "risky-agent", vec![], true,  None), // optional
        ]);
        let selector = ArchitectureSelector::new(base.to_path_buf());
        let decision  = selector.select(&def, "us-east-1");

        assert!(!decision.governance.all_agents_allowed);
        assert!(decision.governance.blocked_agents.contains(&"risky-agent".to_string()));
        assert!(decision.config.governance_skip_candidates.contains(&"risky".to_string()),
            "Optional blocked step should be a skip candidate");
    }

    // ── quick_classify tests ──────────────────────────────────────────────────

    #[test]
    fn test_quick_classify_single_tool_sequential() {
        let req = QuickClassifyRequest {
            tenant_id:        "t1".to_string(),
            tool_count:       1,
            parallel_branches: 0,
            error_tolerance:  0,
            governance_strict: false,
        };
        let resp = quick_classify(&req);
        assert_eq!(resp.architecture, ArchitectureType::Deterministic);
        assert!(!resp.one_line.is_empty());
    }

    #[test]
    fn test_quick_classify_many_tools_parallel() {
        let req = QuickClassifyRequest {
            tenant_id:        "t1".to_string(),
            tool_count:       6,
            parallel_branches: 4,
            error_tolerance:  3,
            governance_strict: false,
        };
        let resp = quick_classify(&req);
        assert_eq!(resp.architecture, ArchitectureType::MultiAgent);
        assert!(resp.confidence > 0.3);
    }

    #[test]
    fn test_quick_classify_two_tools_single_branch() {
        let req = QuickClassifyRequest {
            tenant_id:        "t1".to_string(),
            tool_count:       2,
            parallel_branches: 1,
            error_tolerance:  1,
            governance_strict: false,
        };
        let resp = quick_classify(&req);
        assert_eq!(resp.architecture, ArchitectureType::SingleAgent);
    }

    #[test]
    fn test_quick_classify_strict_governance_nudges_deterministic() {
        let req = QuickClassifyRequest {
            tenant_id:        "t1".to_string(),
            tool_count:       1,
            parallel_branches: 0,
            error_tolerance:  0,
            governance_strict: true,
        };
        let resp = quick_classify(&req);
        // With governance_strict + single tool + sequential → deterministic
        assert_eq!(resp.architecture, ArchitectureType::Deterministic);
    }
}
