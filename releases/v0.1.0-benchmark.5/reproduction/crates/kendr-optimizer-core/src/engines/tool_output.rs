use std::collections::BTreeSet;

use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, OptimizeRequest, RiskLevel,
};

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};

pub(crate) struct ToolOutput;

impl Engine for ToolOutput {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "tool-output-prune",
            "Extracts diagnostics and boundary context from oversized tool results",
            RiskLevel::Extractive,
            true,
            false,
        )
    }

    fn propose(
        &self,
        request: &OptimizeRequest,
        current: &ContentEnvelope,
    ) -> Result<Option<Candidate>, OptimizeError> {
        if !request.policy.enable_lossy_tool_output {
            return Ok(None);
        }

        let mut next = current.clone();
        let mut touched = Vec::new();

        for message in &mut next.messages {
            let mut changed = false;
            for part in &mut message.parts {
                let ContentPart::ToolResult { content, .. } = part else {
                    continue;
                };
                if content.chars().count() <= request.policy.max_tool_result_chars {
                    continue;
                }
                let compact = select_diagnostic_lines(content);
                if compact.len() < content.len() {
                    *content = compact;
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
                "extracted diagnostic context from {} oversized tool result(s)",
                touched.len()
            ),
        );
        candidate.touched_message_ids = touched;
        candidate.reconstruction = Some(current.clone());
        Ok(Some(candidate))
    }
}

fn select_diagnostic_lines(input: &str) -> String {
    const BOUNDARY: usize = 24;
    let lines: Vec<&str> = input.lines().collect();
    if lines.len() <= BOUNDARY * 2 {
        return input.to_owned();
    }

    let mut keep = BTreeSet::new();
    keep.extend(0..BOUNDARY.min(lines.len()));
    keep.extend(lines.len().saturating_sub(BOUNDARY)..lines.len());

    for (index, line) in lines.iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        if [
            "error",
            "fatal",
            "panic",
            "exception",
            "failed",
            "warning",
            "assert",
            "caused by",
        ]
        .iter()
        .any(|needle| lower.contains(needle))
        {
            keep.insert(index);
            if index > 0 {
                keep.insert(index - 1);
            }
            if index + 1 < lines.len() {
                keep.insert(index + 1);
            }
        }
    }

    let omitted = lines.len().saturating_sub(keep.len());
    let mut output = Vec::with_capacity(keep.len() + 1);
    let mut previous: Option<usize> = None;
    for index in keep {
        if let Some(last) = previous
            && index > last + 1
        {
            output.push(format!("⟦kendr.omitted lines={}⟧", index - last - 1));
        }
        output.push(lines[index].to_owned());
        previous = Some(index);
    }
    output.push(format!(
        "⟦kendr.tool_result_summary total_lines={} omitted_lines={} recovery=available⟧",
        lines.len(),
        omitted
    ));
    output.join("\n")
}
