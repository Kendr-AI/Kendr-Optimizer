use std::collections::BTreeMap;

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};
use crate::tokenizer::sha256_hex;
use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, MessageRole, OptimizeRequest, RiskLevel,
};

pub(crate) struct HistoryDedup;

impl Engine for HistoryDedup {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "history-dedup",
            "Replaces exact old text replays with stable references to earlier messages",
            RiskLevel::Recoverable,
            true,
            false,
        )
    }

    fn propose(
        &self,
        request: &OptimizeRequest,
        current: &ContentEnvelope,
    ) -> Result<Option<Candidate>, OptimizeError> {
        if !request.host_capabilities.can_restore_references {
            return Ok(None);
        }

        let cutoff = current
            .messages
            .len()
            .saturating_sub(request.policy.preserve_recent_messages);
        if cutoff < 2 {
            return Ok(None);
        }

        let mut next = current.clone();
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        let mut touched = Vec::new();

        for message in next.messages.iter_mut().take(cutoff) {
            if !matches!(message.role, MessageRole::User | MessageRole::Assistant)
                || !text_only(&message.parts)
            {
                continue;
            }
            let serialized = serde_json::to_vec(&message.parts)?;
            let digest = sha256_hex(&serialized);
            let key = format!("{:?}:{digest}", message.role);
            if let Some(original_id) = seen.get(&key) {
                message.parts = vec![ContentPart::Text {
                    text: format!("⟦kendr.reference message_id={original_id} sha256={digest}⟧"),
                }];
                touched.push(message.id.clone());
            } else {
                seen.insert(key, message.id.clone());
            }
        }

        if touched.is_empty() {
            return Ok(None);
        }
        let mut candidate = Candidate::new(
            next,
            format!("referenced {} exact historical replay(s)", touched.len()),
        );
        candidate.touched_message_ids = touched;
        candidate.reconstruction = Some(current.clone());
        Ok(Some(candidate))
    }
}

fn text_only(parts: &[ContentPart]) -> bool {
    !parts.is_empty()
        && parts.iter().all(|part| {
            matches!(
                part,
                ContentPart::Text { .. } | ContentPart::Document { .. }
            )
        })
}
