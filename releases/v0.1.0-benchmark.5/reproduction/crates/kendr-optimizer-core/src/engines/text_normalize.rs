use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, OptimizeRequest, RiskLevel,
};

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};

pub(crate) struct TextNormalize;

impl Engine for TextNormalize {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "text-normalize",
            "Collapses redundant blank lines outside fenced code without rewriting prose",
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
                let value = match part {
                    ContentPart::Text { text } | ContentPart::Document { text, .. } => text,
                    _ => continue,
                };
                let normalized = normalize_blank_lines(value);
                if normalized != *value {
                    *value = normalized;
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
                "normalized redundant blank lines in {} message(s)",
                touched.len()
            ),
        );
        candidate.touched_message_ids = touched;
        Ok(Some(candidate))
    }
}

fn normalize_blank_lines(input: &str) -> String {
    let had_trailing_newline = input.ends_with('\n');
    let mut output = Vec::new();
    let mut blank_run = 0usize;
    let mut fenced = false;

    for line in input.lines() {
        let trimmed = line.trim_start();
        if trimmed.as_bytes().starts_with(&[96, 96, 96]) || trimmed.starts_with("~~~") {
            fenced = !fenced;
            blank_run = 0;
            output.push(line.to_owned());
            continue;
        }

        if !fenced && line.trim().is_empty() {
            blank_run += 1;
            if blank_run <= 2 {
                output.push(String::new());
            }
        } else {
            blank_run = 0;
            output.push(line.to_owned());
        }
    }

    let mut normalized = output.join("\n");
    if had_trailing_newline {
        normalized.push('\n');
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::normalize_blank_lines;

    #[test]
    fn collapses_only_excess_blanks() {
        let input = "a\n\n\n\nb\n";
        let output = normalize_blank_lines(input);
        assert_eq!(output, "a\n\n\nb\n");
    }
}
