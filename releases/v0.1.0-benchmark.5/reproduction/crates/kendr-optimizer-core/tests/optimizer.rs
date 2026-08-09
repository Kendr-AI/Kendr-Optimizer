use kendr_optimizer_contracts::{
    CacheSegment, ContentEnvelope, ContentPart, GenerationAction, HostCapabilities, Message,
    MessageRole, OptimizationPolicy, OptimizePhase, OptimizeRequest, OutcomeStatus, ProviderUsage,
    MoneyEstimate, RiskLevel, TargetContext, ToolDefinition, UsageObservation,
};
use kendr_optimizer_core::Optimizer;
use serde_json::json;

fn request(content: ContentEnvelope) -> OptimizeRequest {
    OptimizeRequest {
        schema_version: kendr_optimizer_contracts::SCHEMA_VERSION.to_owned(),
        phase: OptimizePhase::Request,
        request_id: "test-request".to_owned(),
        session_id: None,
        content,
        target: TargetContext::default(),
        generation: Default::default(),
        host_capabilities: HostCapabilities::default(),
        policy: OptimizationPolicy {
            min_gain_tokens: 1,
            min_gain_percent: 0.0,
            ..OptimizationPolicy::default()
        },
    }
}

fn message(id: &str, role: MessageRole, part: ContentPart) -> Message {
    Message {
        id: id.to_owned(),
        role,
        parent_id: None,
        turn_id: None,
        parts: vec![part],
        metadata: Default::default(),
    }
}

#[test]
fn minifies_embedded_json_and_reports_signed_whole_envelope_delta() {
    let content = ContentEnvelope {
        messages: vec![message(
            "tool-1",
            MessageRole::Tool,
            ContentPart::ToolResult {
                call_id: "call-1".to_owned(),
                name: Some("read_json".to_owned()),
                content:
                    "{\n  \"name\": \"kendr\",\n  \"values\": [\n    1,\n    2,\n    3\n  ]\n}"
                        .to_owned(),
                is_error: false,
            },
        )],
        ..ContentEnvelope::default()
    };

    let outcome = Optimizer::new().optimize(&request(content)).unwrap();
    assert_eq!(outcome.receipt.status, OutcomeStatus::Applied);
    assert!(outcome.receipt.token_delta > 0);
    assert!(!outcome.receipt.verified_savings);
    let ContentPart::ToolResult { content, .. } = &outcome.content.messages[0].parts[0] else {
        panic!("expected tool result");
    };
    assert_eq!(content, r#"{"name":"kendr","values":[1,2,3]}"#);
}

#[test]
fn a_small_prompt_is_an_explicit_no_op() {
    let mut input = request(ContentEnvelope {
        messages: vec![message(
            "user-1",
            MessageRole::User,
            ContentPart::Text {
                text: "hello".to_owned(),
            },
        )],
        ..ContentEnvelope::default()
    });
    input.policy.min_gain_tokens = 8;

    let outcome = Optimizer::new().optimize(&input).unwrap();
    assert_eq!(outcome.receipt.status, OutcomeStatus::Skipped);
    assert_eq!(outcome.receipt.token_delta, 0);
    assert!(outcome.receipt.no_op_reason.is_some());
    assert_eq!(outcome.content, input.content);
}

#[test]
fn declared_cache_segments_are_immutable_by_default() {
    let content = ContentEnvelope {
        messages: vec![message(
            "cached-tool",
            MessageRole::Tool,
            ContentPart::ToolResult {
                call_id: "call-1".to_owned(),
                name: None,
                content: "{\n  \"large\": [\n    1,\n    2,\n    3,\n    4\n  ]\n}".to_owned(),
                is_error: false,
            },
        )],
        ..ContentEnvelope::default()
    };
    let mut input = request(content.clone());
    input.target.cache_segments = vec![CacheSegment {
        id: "prefix".to_owned(),
        message_ids: vec!["cached-tool".to_owned()],
    }];

    let outcome = Optimizer::new().optimize(&input).unwrap();
    assert_eq!(outcome.receipt.status, OutcomeStatus::Skipped);
    assert_eq!(outcome.content, content);
    assert!(outcome.receipt.attempts.iter().any(|attempt| {
        attempt
            .verification
            .iter()
            .any(|check| check.name == "cache_prefix" && !check.passed)
    }));
}

#[test]
fn recoverable_compaction_restores_the_original_envelope_exactly() {
    let repeated = std::iter::repeat_n("unchanged diagnostic line", 30)
        .collect::<Vec<_>>()
        .join("\n");
    let content = ContentEnvelope {
        messages: vec![message(
            "tool-1",
            MessageRole::Tool,
            ContentPart::ToolResult {
                call_id: "call-1".to_owned(),
                name: Some("exec".to_owned()),
                content: repeated,
                is_error: false,
            },
        )],
        ..ContentEnvelope::default()
    };
    let original = content.clone();
    let outcome = Optimizer::new().optimize(&request(content)).unwrap();
    assert_eq!(outcome.receipt.status, OutcomeStatus::Applied);
    let capsule = outcome.recovery.expect("recovery capsule");
    let restored = Optimizer::new().restore(&capsule).unwrap();
    assert_eq!(restored, original);
}

#[test]
fn typed_context_repetition_preserves_protected_artifact_multiplicity() {
    let paragraph = format!(
        "Use https://example.test/runbook on port 8443 and do not disable TLS. {}",
        "This exact operational constraint remains model-visible. ".repeat(12)
    );
    let repeated = std::iter::repeat_n(paragraph, 4)
        .collect::<Vec<_>>()
        .join("\n\n");
    let content = ContentEnvelope {
        messages: vec![message(
            "user-repetition",
            MessageRole::User,
            ContentPart::Text { text: repeated },
        )],
        ..ContentEnvelope::default()
    };
    let original = content.clone();

    let outcome = Optimizer::new().optimize(&request(content)).unwrap();

    assert_eq!(outcome.receipt.status, OutcomeStatus::Applied);
    let attempt = outcome
        .receipt
        .attempts
        .iter()
        .find(|attempt| attempt.engine_id == "context-repetition")
        .expect("context repetition attempt");
    assert!(
        attempt
            .verification
            .iter()
            .any(|check| { check.name == "typed_transform_proof" && check.passed })
    );
    assert!(
        attempt
            .verification
            .iter()
            .any(|check| { check.name == "protected_artifacts" && check.passed })
    );
    let restored = Optimizer::new()
        .restore(&outcome.recovery.expect("recovery capsule"))
        .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn typed_pytest_fold_keeps_failure_evidence_and_expands_exactly() {
    let mut lines = (0..40)
        .map(|index| format!("tests/test_worker.py::test_case_{index:03} PASSED [ 42%]"))
        .collect::<Vec<_>>();
    lines.extend([
        "tests/test_worker.py::test_tls_chain_040 FAILED [100%]".to_owned(),
        "E   AssertionError: status=526 request_id=req-7f9a".to_owned(),
        "E   endpoint=https://api.example.test/v2/charges".to_owned(),
        "1 failed, 40 passed in 1.25s".to_owned(),
    ]);
    let content = ContentEnvelope {
        messages: vec![message(
            "pytest-result",
            MessageRole::Tool,
            ContentPart::ToolResult {
                call_id: "call-pytest".to_owned(),
                name: Some("pytest".to_owned()),
                content: lines.join("\n"),
                is_error: true,
            },
        )],
        ..ContentEnvelope::default()
    };
    let original = content.clone();

    let outcome = Optimizer::new().optimize(&request(content)).unwrap();

    assert_eq!(outcome.receipt.status, OutcomeStatus::Applied);
    let attempt = outcome
        .receipt
        .attempts
        .iter()
        .find(|attempt| attempt.engine_id == "pytest-result-fold")
        .expect("pytest fold attempt");
    assert!(
        attempt
            .verification
            .iter()
            .any(|check| { check.name == "typed_transform_proof" && check.passed })
    );
    let ContentPart::ToolResult { content, .. } = &outcome.content.messages[0].parts[0] else {
        panic!("expected tool result");
    };
    assert!(content.contains("test_tls_chain_040 FAILED"));
    assert!(content.contains("status=526"));
    assert!(content.contains("[[kendr.pytest.fold:v1"));
    let restored = Optimizer::new()
        .restore(&outcome.recovery.expect("recovery capsule"))
        .unwrap();
    assert_eq!(restored, original);
}

#[test]
fn tool_selection_is_explicit_and_retry_gated() {
    let tools = vec![
        tool(
            "github_search",
            "Search GitHub repositories and code",
            false,
        ),
        tool("read_file", "Read a local file safely", true),
        tool("send_email", "Send an email to a recipient", false),
        tool("calendar_create", "Create a calendar event", false),
        tool("weather", "Look up weather forecasts", false),
        tool("stock_quote", "Fetch a stock market price", false),
    ];
    let content = ContentEnvelope {
        messages: vec![message(
            "user-1",
            MessageRole::User,
            ContentPart::Text {
                text: "Search GitHub repositories for a token optimizer".to_owned(),
            },
        )],
        tools,
        ..ContentEnvelope::default()
    };
    let mut input = request(content);
    input.policy.risk_ceiling = RiskLevel::Extractive;
    input.policy.enable_tool_selection = true;
    input.host_capabilities.can_narrow_tools = true;
    input.host_capabilities.can_retry_with_full_tools = true;

    let outcome = Optimizer::new().optimize(&input).unwrap();
    assert_eq!(outcome.receipt.status, OutcomeStatus::Applied);
    assert!(
        outcome
            .content
            .tools
            .iter()
            .any(|tool| tool.name == "github_search")
    );
    assert!(
        outcome
            .content
            .tools
            .iter()
            .any(|tool| tool.name == "read_file")
    );
    assert!(outcome.content.tools.len() < input.content.tools.len());
}

#[test]
fn provider_savings_are_verified_only_with_a_paired_baseline() {
    let optimizer = Optimizer::new();
    let unpaired = optimizer.observe(UsageObservation {
        request_id: "one".to_owned(),
        optimized: usage(80, 30),
        paired_baseline: None,
    });
    assert!(!unpaired.verified);

    let paired = optimizer.observe(UsageObservation {
        request_id: "two".to_owned(),
        optimized: usage(80, 30),
        paired_baseline: Some(usage(120, 40)),
    });
    assert!(paired.verified);
    assert_eq!(paired.input_token_delta, Some(40));
    assert_eq!(paired.output_token_delta, Some(10));
}

#[test]
fn paired_cost_reduction_is_not_verified_when_task_success_regresses() {
    let mut optimized = usage(80, 30);
    optimized.task_success = Some(false);
    let result = Optimizer::new().observe(UsageObservation {
        request_id: "quality-regression".to_owned(),
        optimized,
        paired_baseline: Some(usage(120, 40)),
    });

    assert!(result.paired_baseline_supplied);
    assert_eq!(result.quality_preserved, Some(false));
    assert!(!result.verified);
}

#[test]
fn paired_usage_increase_is_not_a_verified_saving() {
    let result = Optimizer::new().observe(UsageObservation {
        request_id: "usage-increase".to_owned(),
        optimized: usage(140, 50),
        paired_baseline: Some(usage(120, 40)),
    });

    assert_eq!(result.quality_preserved, Some(true));
    assert_eq!(result.input_token_delta, Some(-20));
    assert_eq!(result.output_token_delta, Some(-10));
    assert!(!result.verified);
}

#[test]
fn comparable_provider_cost_takes_precedence_over_token_reduction() {
    let mut baseline = usage(120, 40);
    baseline.total_cost = Some(MoneyEstimate {
        amount: 1.0,
        currency: "USD".to_owned(),
        basis: "provider".to_owned(),
    });
    let mut optimized = usage(80, 30);
    optimized.total_cost = Some(MoneyEstimate {
        amount: 1.2,
        currency: "USD".to_owned(),
        basis: "provider".to_owned(),
    });

    let result = Optimizer::new().observe(UsageObservation {
        request_id: "cost-increase".to_owned(),
        optimized,
        paired_baseline: Some(baseline),
    });

    assert_eq!(result.input_token_delta, Some(40));
    assert_eq!(result.total_cost_delta.unwrap().amount, -0.2);
    assert!(!result.verified);
}

#[test]
fn paired_failed_tasks_do_not_establish_quality_preservation() {
    let mut baseline = usage(120, 40);
    baseline.task_success = Some(false);
    let mut optimized = usage(80, 30);
    optimized.task_success = Some(false);

    let result = Optimizer::new().observe(UsageObservation {
        request_id: "both-failed".to_owned(),
        optimized,
        paired_baseline: Some(baseline),
    });

    assert_eq!(result.quality_preserved, Some(false));
    assert!(!result.verified);
}

#[test]
fn output_policy_uses_host_controls_only_after_break_even_gates() {
    let content = ContentEnvelope {
        messages: vec![message(
            "user-1",
            MessageRole::User,
            ContentPart::Text {
                text: "Summarize the findings.".to_owned(),
            },
        )],
        ..ContentEnvelope::default()
    };
    let mut input = request(content);
    input.generation.expected_output_tokens = Some(1000);
    input.policy.enable_generation_policy = true;
    input.policy.risk_ceiling = RiskLevel::Extractive;
    input.host_capabilities.can_set_verbosity = true;

    let outcome = Optimizer::new().optimize(&input).unwrap();
    let recommendation = outcome
        .generation_recommendation
        .expect("generation recommendation");
    assert_eq!(recommendation.action, GenerationAction::SetHostControls);
    assert_eq!(recommendation.estimated_added_input_tokens, 0);
    assert_eq!(recommendation.estimated_output_reduction_tokens, Some(200));
    assert!(!recommendation.verified_savings);
}

#[test]
fn output_policy_respects_explicit_detail_intent() {
    let content = ContentEnvelope {
        messages: vec![message(
            "user-1",
            MessageRole::User,
            ContentPart::Text {
                text: "Give me a thorough, detailed explanation.".to_owned(),
            },
        )],
        ..ContentEnvelope::default()
    };
    let mut input = request(content);
    input.generation.expected_output_tokens = Some(1000);
    input.policy.enable_generation_policy = true;
    input.policy.risk_ceiling = RiskLevel::Extractive;
    input.host_capabilities.can_set_verbosity = true;

    let outcome = Optimizer::new().optimize(&input).unwrap();
    assert_eq!(
        outcome.generation_recommendation.unwrap().action,
        GenerationAction::NoChange
    );
}

fn tool(name: &str, description: &str, required: bool) -> ToolDefinition {
    ToolDefinition {
        name: name.to_owned(),
        description: description.to_owned(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            }
        }),
        required,
        tags: Vec::new(),
        metadata: Default::default(),
    }
}

fn usage(input_tokens: u64, output_tokens: u64) -> ProviderUsage {
    ProviderUsage {
        input_tokens,
        output_tokens,
        cached_input_tokens: 0,
        total_cost: None,
        latency_ms: None,
        task_success: Some(true),
    }
}
