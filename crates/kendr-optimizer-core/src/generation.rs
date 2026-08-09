use kendr_optimizer_contracts::{
    ContentPart, GenerationAction, GenerationRecommendation, MessageRole, OptimizeRequest,
    RiskLevel, VerbosityIntent,
};

use crate::engine::OptimizeError;
use crate::tokenizer::count_text;

const BREVITY_INSTRUCTION: &str = "Answer concisely. Preserve required facts, constraints, code, errors, negations, numbers, units, and output structure.";

pub(crate) fn recommend(
    request: &OptimizeRequest,
) -> Result<Option<GenerationRecommendation>, OptimizeError> {
    if !request.policy.enable_generation_policy {
        return Ok(None);
    }
    if request.policy.risk_ceiling < RiskLevel::Extractive {
        return Ok(Some(no_change(
            "Generation shortening is extractive and exceeds the configured risk ceiling.",
        )));
    }

    if request.content.output_contract.is_some()
        || request.generation.requested_verbosity == VerbosityIntent::Exact
    {
        return Ok(Some(no_change(
            "Structured or exact output was requested; no brevity control is recommended.",
        )));
    }

    if detailed_intent(request) {
        return Ok(Some(no_change(
            "The request explicitly asks for detail, so output brevity would conflict with user intent.",
        )));
    }

    let expected = request.generation.expected_output_tokens;
    let target = request.generation.target_output_tokens;
    let mut max_output_tokens = None;
    let mut verbosity = None;
    let mut predicted = None;

    if request.host_capabilities.can_set_max_output_tokens
        && let Some(target) = target
    {
        let does_reduce_limit = request
            .generation
            .current_max_output_tokens
            .is_none_or(|current| target < current);
        if does_reduce_limit {
            max_output_tokens = Some(target);
            predicted = expected.map(|value| value.saturating_sub(target));
        }
    }

    if request.host_capabilities.can_set_verbosity
        && matches!(
            request.generation.requested_verbosity,
            VerbosityIntent::Auto | VerbosityIntent::Concise
        )
        && expected.is_some_and(|tokens| tokens >= 256)
    {
        verbosity = Some(VerbosityIntent::Concise);
        predicted = Some(predicted.unwrap_or(0).max(expected.unwrap_or(0) / 5));
    }

    if max_output_tokens.is_some() || verbosity.is_some() {
        let net = predicted.unwrap_or(0) as i64;
        if target.is_some() || net >= request.policy.min_expected_output_saving_tokens as i64 {
            return Ok(Some(GenerationRecommendation {
                action: GenerationAction::SetHostControls,
                risk: RiskLevel::Extractive,
                max_output_tokens,
                verbosity,
                instruction: None,
                estimated_added_input_tokens: 0,
                estimated_output_reduction_tokens: predicted,
                expected_net_token_reduction: net,
                verified_savings: false,
                reason:
                    "Use host-native generation controls; predicted output reduction remains unverified until a paired observation."
                        .to_owned(),
            }));
        }
    }

    if request.host_capabilities.can_append_generation_policy
        && matches!(
            request.generation.requested_verbosity,
            VerbosityIntent::Auto | VerbosityIntent::Concise
        )
        && let Some(expected) = expected
    {
        let predicted_reduction = expected.saturating_mul(15) / 100;
        let instruction_tokens =
            count_text(BREVITY_INSTRUCTION, request.target.tokenizer_profile)? + 4;
        let net = predicted_reduction as i64 - instruction_tokens as i64;
        if net >= request.policy.min_expected_output_saving_tokens as i64 {
            return Ok(Some(GenerationRecommendation {
                action: GenerationAction::AppendInstruction,
                risk: RiskLevel::Extractive,
                max_output_tokens: None,
                verbosity: Some(VerbosityIntent::Concise),
                instruction: Some(BREVITY_INSTRUCTION.to_owned()),
                estimated_added_input_tokens: instruction_tokens,
                estimated_output_reduction_tokens: Some(predicted_reduction),
                expected_net_token_reduction: net,
                verified_savings: false,
                reason:
                    "The opt-in heuristic predicts that output reduction exceeds added input overhead; verify with paired runs."
                        .to_owned(),
            }));
        }
    }

    Ok(Some(no_change(
        "No host-supported output action passed intent, capability, and expected net-gain gates.",
    )))
}

fn detailed_intent(request: &OptimizeRequest) -> bool {
    if matches!(
        request.generation.requested_verbosity,
        VerbosityIntent::Detailed | VerbosityIntent::Exact
    ) {
        return true;
    }

    let latest = request
        .content
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            message
                .parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } | ContentPart::Document { text, .. } => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
                .to_ascii_lowercase()
        })
        .unwrap_or_default();

    [
        "in detail",
        "detailed",
        "thorough",
        "step by step",
        "comprehensive",
        "do not omit",
        "full explanation",
    ]
    .iter()
    .any(|phrase| latest.contains(phrase))
}

fn no_change(reason: &str) -> GenerationRecommendation {
    GenerationRecommendation {
        action: GenerationAction::NoChange,
        risk: RiskLevel::PassThrough,
        max_output_tokens: None,
        verbosity: None,
        instruction: None,
        estimated_added_input_tokens: 0,
        estimated_output_reduction_tokens: None,
        expected_net_token_reduction: 0,
        verified_savings: false,
        reason: reason.to_owned(),
    }
}
