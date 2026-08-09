use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, OptimizeRequest, RiskLevel,
};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};

static ANSI_ESCAPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])"#).expect("valid ANSI regex"));

pub(crate) struct TerminalClean;

impl Engine for TerminalClean {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "terminal-clean",
            "Removes terminal control sequences from tool results",
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
                let ContentPart::ToolResult { content, .. } = part else {
                    continue;
                };
                let cleaned = ANSI_ESCAPE.replace_all(content, "");
                if cleaned != content.as_str() {
                    *content = cleaned.into_owned();
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
            format!(
                "removed ANSI controls from {} tool result(s)",
                touched.len()
            ),
        );
        candidate.touched_message_ids = touched;
        Ok(Some(candidate))
    }
}
