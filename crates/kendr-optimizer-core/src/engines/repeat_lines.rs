use crate::engine::{Candidate, Engine, OptimizeError, descriptor};
use crate::tokenizer::sha256_hex;
use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, OptimizeRequest, RiskLevel,
};

pub(crate) struct RepeatLines;

impl Engine for RepeatLines {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "repeat-lines",
            "Run-length encodes exact repeated lines in tool results",
            RiskLevel::Recoverable,
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
                let Some(compact) = compact_repeated_lines(content) else {
                    continue;
                };
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
                "encoded exact repeated-line runs in {} message(s)",
                touched.len()
            ),
        );
        candidate.touched_message_ids = touched;
        candidate.reconstruction = Some(current.clone());
        Ok(Some(candidate))
    }
}

fn compact_repeated_lines(input: &str) -> Option<String> {
    if input.contains("⟦kendr.repeat ") {
        return None;
    }

    let lines: Vec<&str> = input.split('\n').collect();
    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut compacted = false;

    while index < lines.len() {
        let line = lines[index];
        let mut end = index + 1;
        while end < lines.len() && lines[end] == line {
            end += 1;
        }
        let run = end - index;
        output.push(line.to_owned());
        if run >= 4 && !line.trim().is_empty() {
            let digest = sha256_hex(line.as_bytes());
            output.push(format!(
                "⟦kendr.repeat omitted={} sha256={}⟧",
                run - 1,
                digest
            ));
            compacted = true;
        } else {
            for _ in 1..run {
                output.push(line.to_owned());
            }
        }
        index = end;
    }

    compacted.then(|| output.join("\n"))
}

/// Expands only canonical markers emitted by this engine. The verifier uses
/// this to prove that a candidate's visible count marker represents an exact
/// run of the retained line rather than treating recovery as preservation.
pub(crate) fn expand_repeated_lines(input: &str) -> Option<String> {
    const PREFIX: &str = "⟦kendr.repeat omitted=";
    const DIGEST_FIELD: &str = " sha256=";
    const SUFFIX: &str = "⟧";

    // Verification must never turn a compact marker into an unbounded
    // allocation. These limits fail closed; hosts that need larger envelopes
    // should split them before optimization.
    const MAX_EXPANDED_LINES: usize = 1_000_000;
    const MAX_EXPANDED_BYTES: usize = 64 * 1024 * 1024;

    if !input.contains(PREFIX) {
        return None;
    }

    let mut output: Vec<String> = Vec::new();
    let mut output_bytes = 0usize;
    let mut expanded = 0usize;
    for line in input.split('\n') {
        if !line.contains(PREFIX) {
            let separator = usize::from(!output.is_empty());
            output_bytes = output_bytes
                .checked_add(separator)?
                .checked_add(line.len())?;
            if output.len() >= MAX_EXPANDED_LINES || output_bytes > MAX_EXPANDED_BYTES {
                return None;
            }
            output.push(line.to_owned());
            continue;
        }

        let body = line.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
        let (omitted_text, digest) = body.split_once(DIGEST_FIELD)?;
        let omitted = omitted_text.parse::<usize>().ok()?;
        if omitted == 0 || omitted.to_string() != omitted_text {
            return None;
        }
        if digest.len() != 64
            || !digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return None;
        }
        let source = output.last()?.clone();
        if source.trim().is_empty() {
            return None;
        }
        if sha256_hex(source.as_bytes()) != digest {
            return None;
        }
        let expanded_line_count = output.len().checked_add(omitted)?;
        let bytes_per_copy = source.len().checked_add(1)?;
        let added_bytes = omitted.checked_mul(bytes_per_copy)?;
        let expanded_byte_count = output_bytes.checked_add(added_bytes)?;
        if expanded_line_count > MAX_EXPANDED_LINES || expanded_byte_count > MAX_EXPANDED_BYTES {
            return None;
        }
        output.extend(std::iter::repeat_n(source, omitted));
        output_bytes = expanded_byte_count;
        expanded += 1;
    }

    (expanded > 0).then(|| output.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{compact_repeated_lines, expand_repeated_lines};

    #[test]
    fn compacts_only_non_empty_runs() {
        let original = "same\nsame\nsame\nsame\nend";
        let result = compact_repeated_lines(original).unwrap();
        assert!(result.contains("omitted=3"));
        assert!(result.ends_with("end"));
        assert_eq!(expand_repeated_lines(&result).as_deref(), Some(original));
    }

    #[test]
    fn expansion_rejects_spoofed_or_noncanonical_markers() {
        let spoof = "same\n⟦kendr.repeat omitted=03 sha256=0000000000000000000000000000000000000000000000000000000000000000⟧";
        assert!(expand_repeated_lines(spoof).is_none());
    }

    #[test]
    fn expansion_rejects_resource_exhaustion_markers_before_allocating() {
        let compact = compact_repeated_lines("same\nsame\nsame\nsame").unwrap();
        let oversized = compact.replacen("omitted=3", "omitted=1000001", 1);
        assert!(expand_repeated_lines(&oversized).is_none());
    }
}
