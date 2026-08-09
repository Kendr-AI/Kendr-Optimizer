use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, OptimizeRequest, RiskLevel,
};
use serde_json::Value;

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};

pub(crate) struct JsonMinify;

impl Engine for JsonMinify {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "json-minify",
            "Minifies complete embedded JSON values while preserving their parsed value",
            RiskLevel::RepresentationSafe,
            true,
            false,
        )
    }

    fn propose(
        &self,
        _request: &OptimizeRequest,
        current: &ContentEnvelope,
    ) -> Result<Option<Candidate>, OptimizeError> {
        let mut next = current.clone();
        let mut touched = Vec::new();

        for message in &mut next.messages {
            let mut changed = false;
            for part in &mut message.parts {
                let text = match part {
                    ContentPart::Text { text }
                    | ContentPart::Document { text, .. }
                    | ContentPart::ToolResult { content: text, .. } => text,
                    _ => continue,
                };
                let trimmed = text.trim();
                if !(trimmed.starts_with('{') || trimmed.starts_with('[')) {
                    continue;
                }
                let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                    continue;
                };
                let compact = serde_json::to_string(&value)?;
                if compact.len() < text.len() {
                    *text = compact;
                    changed = true;
                }
            }
            if changed {
                touched.push(message.id.clone());
            }
        }

        if touched.is_empty() {
            return Ok(None);
        }
        let mut candidate = Candidate::new(
            next,
            format!("minified embedded JSON in {} message(s)", touched.len()),
        );
        candidate.touched_message_ids = touched;
        Ok(Some(candidate))
    }
}
