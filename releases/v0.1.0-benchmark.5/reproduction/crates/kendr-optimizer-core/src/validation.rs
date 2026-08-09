use std::collections::{BTreeMap, BTreeSet};

use kendr_optimizer_contracts::{ContentEnvelope, ContentPart, MessageRole, VerificationCheck};

use crate::engine::{Candidate, OptimizeError};
use crate::engines::{expand_context_repetitions, expand_pytest_folds, expand_repeated_lines};
use crate::protected::protected_artifacts;

pub(crate) fn validate_envelope(content: &ContentEnvelope) -> Result<(), OptimizeError> {
    let mut ids = BTreeSet::new();
    for message in &content.messages {
        if message.id.trim().is_empty() {
            return Err(OptimizeError::InvalidEnvelope(
                "message IDs must not be empty".to_owned(),
            ));
        }
        if !ids.insert(message.id.as_str()) {
            return Err(OptimizeError::InvalidEnvelope(format!(
                "duplicate message ID: {}",
                message.id
            )));
        }
    }

    let mut tool_names = BTreeSet::new();
    for tool in &content.tools {
        if tool.name.trim().is_empty() {
            return Err(OptimizeError::InvalidEnvelope(
                "tool names must not be empty".to_owned(),
            ));
        }
        if !tool_names.insert(tool.name.as_str()) {
            return Err(OptimizeError::InvalidEnvelope(format!(
                "duplicate tool name: {}",
                tool.name
            )));
        }
    }
    Ok(())
}

pub(crate) fn verify_candidate(
    engine_id: &str,
    before: &ContentEnvelope,
    candidate: &Candidate,
) -> Vec<VerificationCheck> {
    let mut checks = Vec::new();
    checks.push(check(
        "valid_envelope",
        validate_envelope(&candidate.content).is_ok(),
        "message and tool identifiers remain valid",
    ));

    checks.push(check(
        "output_contract_unchanged",
        before.output_contract == candidate.content.output_contract,
        "the response/output contract is immutable",
    ));

    let calls_before = immutable_call_fingerprints(before);
    let calls_after = immutable_call_fingerprints(&candidate.content);
    checks.push(check(
        "tool_calls_unchanged",
        calls_before == calls_after,
        "tool call IDs, names, and arguments are immutable",
    ));

    let exact_parts_before = exact_part_fingerprints(before);
    let exact_parts_after = exact_part_fingerprints(&candidate.content);
    checks.push(check(
        "typed_exact_parts_unchanged",
        exact_parts_before == exact_parts_after,
        "code, JSON, and image references are immutable",
    ));

    let tools_valid = if engine_id == "tool-selector" {
        candidate.content.tools.iter().all(|candidate_tool| {
            before
                .tools
                .iter()
                .find(|tool| tool.name == candidate_tool.name)
                == Some(candidate_tool)
        }) && before
            .tools
            .iter()
            .filter(|tool| tool.required)
            .all(|required| {
                candidate
                    .content
                    .tools
                    .iter()
                    .any(|tool| tool.name == required.name)
            })
    } else {
        before.tools == candidate.content.tools
    };
    checks.push(check(
        "tool_surface_safe",
        tools_valid,
        "tool definitions are unchanged; selectors may only remove optional tools",
    ));

    let exact_reconstruction = candidate
        .reconstruction
        .as_ref()
        .is_some_and(|reconstructed| reconstructed == before);
    if candidate.reconstruction.is_some() {
        checks.push(check(
            "exact_reconstruction",
            exact_reconstruction,
            "the supplied recovery reconstruction exactly matches the input",
        ));
    }

    let typed_encoding = match engine_id {
        "context-repetition" | "repeat-lines" | "pytest-result-fold" => {
            let passed = typed_encoding_reconstructs(engine_id, before, candidate);
            checks.push(check(
                "typed_transform_proof",
                passed,
                "typed markers expand byte-for-byte to the model-visible input",
            ));
            passed
        }
        _ => false,
    };

    let artifacts_before = protected_artifacts(before);
    let artifacts_after = protected_artifacts(&candidate.content);
    let literal_artifacts_preserved = artifact_subset(&artifacts_before, &artifacts_after);
    let artifacts_preserved = literal_artifacts_preserved || typed_encoding;
    let (missing_values, missing_occurrences) = artifacts_before.iter().fold(
        (0usize, 0usize),
        |(value_count, occurrence_count), (artifact, expected_count)| {
            let observed_count = artifacts_after.get(artifact).copied().unwrap_or_default();
            if observed_count < *expected_count {
                (
                    value_count.saturating_add(1),
                    occurrence_count.saturating_add(expected_count - observed_count),
                )
            } else {
                (value_count, occurrence_count)
            }
        },
    );
    checks.push(VerificationCheck {
        name: "protected_artifacts".to_owned(),
        passed: artifacts_preserved,
        detail: Some(if literal_artifacts_preserved {
            "URLs, paths, numbers, negations, identifiers, errors, and preserve blocks remain model-visible with their required multiplicity"
                .to_owned()
        } else if typed_encoding {
            "protected artifact multiplicity is represented by independently verified typed markers"
                .to_owned()
        } else {
            format!(
                "missing or underrepresented protected artifacts: {missing_values} distinct value(s), {missing_occurrences} occurrence(s); raw values omitted from the receipt"
            )
        }),
    });

    checks
}

fn typed_encoding_reconstructs(
    engine_id: &str,
    before: &ContentEnvelope,
    candidate: &Candidate,
) -> bool {
    let mut reconstructed = candidate.content.clone();
    let mut expanded_any = false;

    for (message_index, message) in reconstructed.messages.iter_mut().enumerate() {
        let Some(original_message) = before.messages.get(message_index) else {
            return false;
        };
        if original_message.id != message.id {
            return false;
        }
        for (part_index, part) in message.parts.iter_mut().enumerate() {
            let Some(original_part) = original_message.parts.get(part_index) else {
                return false;
            };
            // Authored marker-shaped text in an unchanged part is not a
            // transform proof and must never be interpreted as one. Only the
            // concrete part diff produced by this engine is eligible.
            if &*part == original_part {
                continue;
            }
            let expanded = match (engine_id, &*part) {
                (
                    "context-repetition",
                    ContentPart::Text { text } | ContentPart::Document { text, .. },
                ) => expand_context_repetitions(&message.id, part_index, text),
                ("repeat-lines", ContentPart::ToolResult { content, .. }) => {
                    expand_repeated_lines(content)
                }
                ("pytest-result-fold", ContentPart::ToolResult { content, .. })
                    if content.contains("[[kendr.pytest.fold") =>
                {
                    expand_pytest_folds(content)
                }
                _ => None,
            };

            if let Some(expanded) = expanded {
                match part {
                    ContentPart::Text { text } | ContentPart::Document { text, .. } => {
                        *text = expanded;
                    }
                    ContentPart::ToolResult { content, .. } => {
                        *content = expanded;
                    }
                    _ => return false,
                }
                expanded_any = true;
            }
        }
    }

    expanded_any && reconstructed == *before
}

fn check(name: &str, passed: bool, detail: &str) -> VerificationCheck {
    VerificationCheck {
        name: name.to_owned(),
        passed,
        detail: Some(detail.to_owned()),
    }
}

fn artifact_subset(expected: &BTreeMap<String, usize>, observed: &BTreeMap<String, usize>) -> bool {
    expected.iter().all(|(artifact, expected_count)| {
        observed
            .get(artifact)
            .is_some_and(|observed_count| observed_count >= expected_count)
    })
}

fn immutable_call_fingerprints(content: &ContentEnvelope) -> Vec<String> {
    content
        .messages
        .iter()
        .flat_map(|message| {
            message.parts.iter().filter_map(move |part| match part {
                ContentPart::ToolCall {
                    id,
                    name,
                    arguments,
                } => Some(format!(
                    "{}:{:?}:{id}:{name}:{}",
                    message.id, message.role, arguments
                )),
                ContentPart::ToolResult {
                    call_id,
                    name,
                    is_error,
                    ..
                } => Some(format!(
                    "{}:{:?}:{call_id}:{name:?}:{is_error}",
                    message.id, message.role
                )),
                _ => None,
            })
        })
        .collect()
}

fn exact_part_fingerprints(content: &ContentEnvelope) -> Vec<String> {
    content
        .messages
        .iter()
        .flat_map(|message| {
            message
                .parts
                .iter()
                .enumerate()
                .filter_map(move |(index, part)| match part {
                    ContentPart::Code { .. }
                    | ContentPart::Json { .. }
                    | ContentPart::ImageReference { .. } => {
                        Some(format!("{}:{index}:{part:?}", message.id))
                    }
                    _ => None,
                })
        })
        .collect()
}

#[allow(dead_code)]
fn _role_is_protocol_sensitive(role: MessageRole) -> bool {
    matches!(
        role,
        MessageRole::System | MessageRole::Developer | MessageRole::Tool
    )
}

#[cfg(test)]
mod tests {
    use kendr_optimizer_contracts::{ContentEnvelope, ContentPart, Message, MessageRole};

    use super::verify_candidate;
    use crate::engine::Candidate;

    fn envelope(text: impl Into<String>) -> ContentEnvelope {
        ContentEnvelope {
            messages: vec![Message {
                id: "message-1".to_owned(),
                role: MessageRole::User,
                parent_id: None,
                turn_id: None,
                parts: vec![ContentPart::Text { text: text.into() }],
                metadata: Default::default(),
            }],
            ..ContentEnvelope::default()
        }
    }

    fn check_passed(
        engine_id: &str,
        before: &ContentEnvelope,
        candidate: &Candidate,
        check_name: &str,
    ) -> bool {
        verify_candidate(engine_id, before, candidate)
            .into_iter()
            .find(|check| check.name == check_name)
            .expect("verification check")
            .passed
    }

    #[test]
    fn duplicate_protected_artifacts_require_the_original_multiplicity() {
        let artifact = "https://example.test/resource";
        let before = envelope(format!("first: {artifact}\nsecond: {artifact}"));
        let mut candidate = Candidate::new(envelope(format!("only: {artifact}")), "test");
        candidate.reconstruction = Some(before.clone());

        assert!(check_passed(
            "test-engine",
            &before,
            &candidate,
            "exact_reconstruction"
        ));
        assert!(!check_passed(
            "test-engine",
            &before,
            &candidate,
            "protected_artifacts"
        ));
    }

    #[test]
    fn exact_reconstruction_does_not_excuse_a_model_visible_artifact_removal() {
        let before = envelope("Use https://example.test/resource for the request.");
        let mut candidate = Candidate::new(envelope("Use the resource for the request."), "test");
        candidate.reconstruction = Some(before.clone());

        assert!(check_passed(
            "test-engine",
            &before,
            &candidate,
            "exact_reconstruction"
        ));
        assert!(!check_passed(
            "test-engine",
            &before,
            &candidate,
            "protected_artifacts"
        ));
    }

    #[test]
    fn failed_artifact_checks_do_not_copy_raw_values_into_receipts() {
        let secret = "https://secret.example.test/customer/4917";
        let before = envelope(format!("Fetch {secret} twice: {secret}"));
        let candidate = Candidate::new(envelope("Fetch the customer record."), "test");

        let detail = verify_candidate("test-engine", &before, &candidate)
            .into_iter()
            .find(|check| check.name == "protected_artifacts")
            .and_then(|check| check.detail)
            .expect("protected-artifact detail");

        assert!(!detail.contains(secret));
        assert!(detail.contains("raw values omitted"));
    }

    #[test]
    fn an_untyped_repeat_marker_is_not_a_proof_of_protected_artifact_multiplicity() {
        let artifact = "https://example.test/resource";
        let before = envelope(
            std::iter::repeat_n(artifact, 4)
                .collect::<Vec<_>>()
                .join("\n"),
        );
        let marker = "⟦kendr.repeat omitted=3 sha256=\
                      0000000000000000000000000000000000000000000000000000000000000000⟧";
        let mut candidate = Candidate::new(envelope(format!("{artifact}\n{marker}")), "test");
        candidate.reconstruction = Some(before.clone());

        assert!(check_passed(
            "repeat-lines",
            &before,
            &candidate,
            "exact_reconstruction"
        ));
        assert!(!check_passed(
            "repeat-lines",
            &before,
            &candidate,
            "protected_artifacts"
        ));
    }

    #[test]
    fn typed_proof_ignores_authored_markers_in_unchanged_parts() {
        let paragraph = "This exact paragraph contains request_id=req-7f9a and enough text to represent a deliberate repeated unit safely.";
        let before = ContentEnvelope {
            messages: vec![Message {
                id: "message-1".to_owned(),
                role: MessageRole::User,
                parent_id: None,
                turn_id: None,
                parts: vec![
                    ContentPart::Text {
                        text: "Authored example: [[kendr.repeat unit=paragraph source=1 copies=1]]"
                            .to_owned(),
                    },
                    ContentPart::Text {
                        text: format!("{paragraph}\n\n{paragraph}"),
                    },
                ],
                metadata: Default::default(),
            }],
            ..ContentEnvelope::default()
        };
        let mut optimized = before.clone();
        optimized.messages[0].parts[1] = ContentPart::Text {
            text: format!("{paragraph}\n\n[[kendr.repeat unit=paragraph source=1 copies=1]]"),
        };
        let mut candidate = Candidate::new(optimized, "typed test");
        candidate.reconstruction = Some(before.clone());

        assert!(check_passed(
            "context-repetition",
            &before,
            &candidate,
            "typed_transform_proof"
        ));
        assert!(check_passed(
            "context-repetition",
            &before,
            &candidate,
            "protected_artifacts"
        ));
    }
}
