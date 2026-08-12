use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, OptimizeRequest, RiskLevel,
};

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};
use crate::tokenizer::sha256_hex;

const MARKER_SENTINEL: &str = "[[kendr.pytest.fold";
const MARKER_PREFIX: &str = "[[kendr.pytest.fold:v1 ";
const MIN_RESULT_LINES: usize = 8;
const MIN_SEQUENCE_LINES: usize = 8;
const EDGE_LINES: usize = 2;
const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;
const MAX_LINES: usize = 50_000;
const MAX_LINE_BYTES: usize = 64 * 1024;
const MAX_NUMERIC_WIDTH: usize = 20;

/// Folds only pytest result lines that form an exactly reconstructable numeric sequence.
///
/// Arbitrary result lines, malformed pytest output, incomplete output, and content that already
/// contains a Kendr pytest marker are deliberately left unchanged.
pub(crate) struct PytestReduce;

impl Engine for PytestReduce {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "pytest-result-fold",
            "Folds exactly reconstructable sequential pytest result lines",
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
        let mut folded_sequences = 0usize;
        let mut omitted_lines = 0usize;

        for message in &mut next.messages {
            let mut changed = false;
            for part in &mut message.parts {
                let ContentPart::ToolResult { content, .. } = part else {
                    continue;
                };
                let Some(compaction) = compact_pytest_output(content) else {
                    continue;
                };

                folded_sequences += compaction.folded_sequences;
                omitted_lines += compaction.omitted_lines;
                *content = compaction.text;
                changed = true;
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
                "folded {folded_sequences} exact pytest sequence(s), replacing \
                 {omitted_lines} reconstructable result line(s)"
            ),
        );
        candidate.touched_message_ids = touched;
        candidate.reconstruction = Some(current.clone());
        Ok(Some(candidate))
    }
}

#[derive(Debug)]
struct Compaction {
    text: String,
    folded_sequences: usize,
    omitted_lines: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PytestStatus {
    Passed,
    Skipped,
    Xfail,
    Failed,
    Error,
    Xpass,
}

impl PytestStatus {
    fn marker_name(self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Skipped => "SKIPPED",
            Self::Xfail => "XFAIL",
            Self::Failed => "FAILED",
            Self::Error => "ERROR",
            Self::Xpass => "XPASS",
        }
    }

    fn is_foldable(self) -> bool {
        matches!(self, Self::Passed | Self::Skipped | Self::Xfail)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedResult<'a> {
    status: PytestStatus,
    numeric_value: u64,
    numeric_width: usize,
    prefix: &'a str,
    suffix: &'a str,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FoldableCounts {
    passed: usize,
    skipped: usize,
    xfailed: usize,
}

impl FoldableCounts {
    fn add(&mut self, status: PytestStatus) {
        match status {
            PytestStatus::Passed => self.passed += 1,
            PytestStatus::Skipped => self.skipped += 1,
            PytestStatus::Xfail => self.xfailed += 1,
            PytestStatus::Failed | PytestStatus::Error | PytestStatus::Xpass => {}
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FoldMarker {
    status: PytestStatus,
    omitted: usize,
    range_start: u64,
    range_end: u64,
    width: usize,
    prefix: String,
    suffix: String,
    sha256: String,
}

fn compact_pytest_output(input: &str) -> Option<Compaction> {
    if input.len() > MAX_INPUT_BYTES || input.contains(MARKER_SENTINEL) {
        return None;
    }

    let separator = detect_line_separator(input)?;
    let lines: Vec<&str> = input.split(separator).collect();
    if lines.len() > MAX_LINES || lines.iter().any(|line| line.len() > MAX_LINE_BYTES) {
        return None;
    }

    let mut parsed = Vec::with_capacity(lines.len());
    let mut actual_counts = FoldableCounts::default();
    let mut result_lines = 0usize;
    for line in &lines {
        let result = parse_result_line(line).ok()?;
        if let Some(result) = &result {
            result_lines += 1;
            actual_counts.add(result.status);
        }
        parsed.push(result);
    }
    if result_lines < MIN_RESULT_LINES {
        return None;
    }

    let mut summaries = lines.iter().filter_map(|line| parse_summary_counts(line));
    let summary = summaries.next()?;
    if summaries.next().is_some() || summary != actual_counts {
        return None;
    }

    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut folded_sequences = 0usize;
    let mut omitted_lines = 0usize;

    while index < lines.len() {
        let Some(first) = parsed[index]
            .as_ref()
            .filter(|result| result.status.is_foldable())
        else {
            output.push(lines[index].to_owned());
            index += 1;
            continue;
        };

        let mut end = index + 1;
        let mut previous_value = first.numeric_value;
        while end < lines.len() {
            let Some(next) = parsed[end].as_ref() else {
                break;
            };
            let Some(expected_value) = previous_value.checked_add(1) else {
                break;
            };
            if next.status != first.status
                || next.prefix != first.prefix
                || next.suffix != first.suffix
                || next.numeric_width != first.numeric_width
                || next.numeric_value != expected_value
            {
                break;
            }
            previous_value = next.numeric_value;
            end += 1;
        }

        let sequence_len = end - index;
        if sequence_len < MIN_SEQUENCE_LINES {
            output.extend(lines[index..end].iter().map(|line| (*line).to_owned()));
            index = end;
            continue;
        }

        let omitted_start = index + EDGE_LINES;
        let omitted_end = end - EDGE_LINES;
        let omitted = omitted_end - omitted_start;
        let range_start = parsed[omitted_start].as_ref()?.numeric_value;
        let range_end = parsed[omitted_end - 1].as_ref()?.numeric_value;
        let omitted_block = lines[omitted_start..omitted_end].join(separator);
        let marker = FoldMarker {
            status: first.status,
            omitted,
            range_start,
            range_end,
            width: first.numeric_width,
            prefix: first.prefix.to_owned(),
            suffix: first.suffix.to_owned(),
            sha256: sha256_hex(omitted_block.as_bytes()),
        }
        .render();

        output.extend(
            lines[index..omitted_start]
                .iter()
                .map(|line| (*line).to_owned()),
        );
        output.push(marker);
        output.extend(
            lines[omitted_end..end]
                .iter()
                .map(|line| (*line).to_owned()),
        );
        folded_sequences += 1;
        omitted_lines += omitted;
        index = end;
    }

    if folded_sequences == 0 {
        return None;
    }
    let text = output.join(separator);
    if text.len() >= input.len() || expand_pytest_folds(&text).as_deref() != Some(input) {
        return None;
    }

    Some(Compaction {
        text,
        folded_sequences,
        omitted_lines,
    })
}

/// Expands and verifies all Kendr pytest fold markers in `input`.
///
/// The helper is intentionally independent from the optimizer's recovery capsule. It returns
/// `None` for malformed markers, impossible ranges, digest mismatches, mixed line endings, or
/// expansions outside the same resource bounds used by the reducer. Input without markers is
/// returned unchanged.
pub(crate) fn expand_pytest_folds(input: &str) -> Option<String> {
    if input.len() > MAX_INPUT_BYTES {
        return None;
    }
    let separator = detect_line_separator(input)?;
    let lines: Vec<&str> = input.split(separator).collect();
    if lines.len() > MAX_LINES || lines.iter().any(|line| line.len() > MAX_LINE_BYTES) {
        return None;
    }

    let mut output = Vec::with_capacity(lines.len());
    let mut expanded_lines = 0usize;
    let mut expanded_bytes = 0usize;
    for line in lines {
        if !line.contains(MARKER_SENTINEL) {
            let separator_bytes = if output.is_empty() {
                0
            } else {
                separator.len()
            };
            expanded_bytes = expanded_bytes
                .checked_add(separator_bytes)?
                .checked_add(line.len())?;
            if expanded_bytes > MAX_INPUT_BYTES {
                return None;
            }
            output.push(line.to_owned());
            expanded_lines = expanded_lines.checked_add(1)?;
            continue;
        }
        if !line.starts_with(MARKER_PREFIX) {
            return None;
        }

        let marker = FoldMarker::parse(line)?;
        if marker.omitted == 0
            || marker.omitted > MAX_LINES
            || marker.width == 0
            || marker.width > MAX_NUMERIC_WIDTH
        {
            return None;
        }
        let expected_end = marker
            .range_start
            .checked_add(u64::try_from(marker.omitted).ok()?.checked_sub(1)?)?;
        if marker.range_end != expected_end {
            return None;
        }

        let restored_line_bytes = marker
            .prefix
            .len()
            .checked_add(marker.width)?
            .checked_add(marker.suffix.len())?;
        if restored_line_bytes > MAX_LINE_BYTES {
            return None;
        }
        let leading_separator_bytes = if output.is_empty() {
            0
        } else {
            separator.len()
        };
        let block_bytes = restored_line_bytes
            .checked_mul(marker.omitted)?
            .checked_add(
                separator
                    .len()
                    .checked_mul(marker.omitted.saturating_sub(1))?,
            )?
            .checked_add(leading_separator_bytes)?;
        expanded_bytes = expanded_bytes.checked_add(block_bytes)?;
        if expanded_bytes > MAX_INPUT_BYTES {
            return None;
        }

        let mut restored = Vec::with_capacity(marker.omitted);
        for value in marker.range_start..=marker.range_end {
            let number = format!("{value:0width$}", width = marker.width);
            if number.len() != marker.width {
                return None;
            }
            let restored_line = format!("{}{}{}", marker.prefix, number, marker.suffix);
            if restored_line.len() > MAX_LINE_BYTES {
                return None;
            }
            restored.push(restored_line);
        }
        let restored_block = restored.join(separator);
        if sha256_hex(restored_block.as_bytes()) != marker.sha256 {
            return None;
        }

        let sample = restored.first()?;
        let parsed = parse_result_line(sample).ok()??;
        if parsed.status != marker.status
            || parsed.numeric_value != marker.range_start
            || parsed.numeric_width != marker.width
            || parsed.prefix != marker.prefix
            || parsed.suffix != marker.suffix
        {
            return None;
        }

        expanded_lines = expanded_lines.checked_add(restored.len())?;
        if expanded_lines > MAX_LINES {
            return None;
        }
        output.extend(restored);
    }

    let expanded = output.join(separator);
    (expanded.len() <= MAX_INPUT_BYTES).then_some(expanded)
}

impl FoldMarker {
    fn render(&self) -> String {
        format!(
            "{MARKER_PREFIX}mode=exact-sequence status={} omitted={} range={}..{} width={} \
             prefix_hex={} suffix_hex={} sha256={}]]",
            self.status.marker_name(),
            self.omitted,
            self.range_start,
            self.range_end,
            self.width,
            hex_encode(self.prefix.as_bytes()),
            hex_encode(self.suffix.as_bytes()),
            self.sha256,
        )
    }

    fn parse(line: &str) -> Option<Self> {
        let body = line.strip_prefix(MARKER_PREFIX)?.strip_suffix("]]")?;
        let fields: Vec<&str> = body.split(' ').collect();
        if fields.len() != 8 || fields[0] != "mode=exact-sequence" {
            return None;
        }

        let status = match fields[1].strip_prefix("status=")? {
            "PASSED" => PytestStatus::Passed,
            "SKIPPED" => PytestStatus::Skipped,
            "XFAIL" => PytestStatus::Xfail,
            _ => return None,
        };
        let omitted = parse_canonical_usize(fields[2].strip_prefix("omitted=")?)?;
        let (range_start, range_end) = fields[3].strip_prefix("range=")?.split_once("..")?;
        let range_start = parse_canonical_u64(range_start)?;
        let range_end = parse_canonical_u64(range_end)?;
        let width = parse_canonical_usize(fields[4].strip_prefix("width=")?)?;
        let prefix = String::from_utf8(hex_decode(fields[5].strip_prefix("prefix_hex=")?)?).ok()?;
        let suffix = String::from_utf8(hex_decode(fields[6].strip_prefix("suffix_hex=")?)?).ok()?;
        let sha256 = fields[7].strip_prefix("sha256=")?;
        if sha256.len() != 64 || !sha256.bytes().all(is_lower_hex) {
            return None;
        }
        if prefix.contains(['\r', '\n', '\0']) || suffix.contains(['\r', '\n', '\0']) {
            return None;
        }
        if prefix.contains(MARKER_SENTINEL) || suffix.contains(MARKER_SENTINEL) {
            return None;
        }

        Some(Self {
            status,
            omitted,
            range_start,
            range_end,
            width,
            prefix,
            suffix,
            sha256: sha256.to_owned(),
        })
    }
}

fn parse_result_line(line: &str) -> Result<Option<ParsedResult<'_>>, ()> {
    const STATUSES: [(&str, PytestStatus); 6] = [
        (" PASSED", PytestStatus::Passed),
        (" SKIPPED", PytestStatus::Skipped),
        (" XFAIL", PytestStatus::Xfail),
        (" FAILED", PytestStatus::Failed),
        (" ERROR", PytestStatus::Error),
        (" XPASS", PytestStatus::Xpass),
    ];

    let node_hint = line.contains(".py::");
    let status_hint = STATUSES.iter().any(|(token, _)| line.contains(token));
    if !node_hint || !status_hint {
        return Ok(None);
    }

    let mut found: Option<(usize, &str, PytestStatus)> = None;
    for (token, status) in STATUSES {
        for (index, _) in line.match_indices(token) {
            let after = index + token.len();
            if after < line.len()
                && !line[after..]
                    .chars()
                    .next()
                    .is_some_and(char::is_whitespace)
            {
                continue;
            }
            if found.is_none_or(|(best, _, _)| index > best) {
                found = Some((index, token, status));
            }
        }
    }
    let Some((status_start, status_token, status)) = found else {
        return Err(());
    };

    let node_id = line[..status_start].trim_end();
    if node_id.is_empty()
        || node_id.starts_with(char::is_whitespace)
        || !node_id.contains(".py::")
        || node_id.chars().any(char::is_control)
    {
        return Err(());
    }
    let suffix_after_status = &line[status_start + status_token.len()..];
    if !valid_status_suffix(status, suffix_after_status) {
        return Err(());
    }

    // Failure, error, and unexpected-pass records are never candidates for
    // folding. Once their pytest shape is validated, leave them opaque and
    // model-visible; they do not need a numeric suffix to coexist with an
    // independently foldable run of successful tests.
    if !status.is_foldable() {
        return Ok(None);
    }

    let bytes = node_id.as_bytes();
    let Some(mut digit_end) = bytes
        .iter()
        .rposition(u8::is_ascii_digit)
        .map(|index| index + 1)
    else {
        return Err(());
    };
    while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
        digit_end += 1;
    }
    let mut digit_start = digit_end;
    while digit_start > 0 && bytes[digit_start - 1].is_ascii_digit() {
        digit_start -= 1;
    }
    let digits = &node_id[digit_start..digit_end];
    if digits.len() > MAX_NUMERIC_WIDTH {
        return Err(());
    }
    let numeric_value = digits.parse::<u64>().map_err(|_| ())?;

    Ok(Some(ParsedResult {
        status,
        numeric_value,
        numeric_width: digits.len(),
        prefix: &line[..digit_start],
        suffix: &line[digit_end..],
    }))
}

fn valid_status_suffix(status: PytestStatus, suffix: &str) -> bool {
    let mut remainder = suffix.trim();
    if remainder.ends_with(']') {
        let Some(open) = remainder.rfind('[') else {
            return false;
        };
        let progress = remainder[open + 1..remainder.len() - 1].trim();
        let Some(percent) = progress.strip_suffix('%') else {
            return false;
        };
        let Ok(percent) = percent.trim().parse::<u8>() else {
            return false;
        };
        if percent > 100 {
            return false;
        }
        remainder = remainder[..open].trim();
    }

    if remainder.is_empty() {
        return true;
    }
    matches!(
        status,
        PytestStatus::Skipped | PytestStatus::Xfail | PytestStatus::Xpass
    ) && remainder.starts_with('(')
        && remainder.ends_with(')')
        && !remainder.contains(['\r', '\n'])
}

fn parse_summary_counts(line: &str) -> Option<FoldableCounts> {
    let normalized = line
        .trim()
        .trim_matches(|character| matches!(character, '=' | '-'))
        .trim();
    let (counts, duration) = normalized.rsplit_once(" in ")?;
    let duration = duration
        .trim()
        .trim_matches(|character| matches!(character, '=' | '-'))
        .trim();
    let duration = duration.strip_suffix('s')?;
    if duration.is_empty()
        || duration
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value >= 0.0)
            .is_none()
    {
        return None;
    }

    let mut parsed = FoldableCounts::default();
    let mut saw_foldable = false;
    let mut saw_passed = false;
    let mut saw_skipped = false;
    let mut saw_xfailed = false;
    for segment in counts.split(',') {
        let mut words = segment
            .trim()
            .trim_matches(|character| matches!(character, '=' | '-'))
            .split_whitespace();
        let count = words.next()?.parse::<usize>().ok()?;
        let label = words.next()?;
        match label {
            "passed" => {
                if saw_passed {
                    return None;
                }
                parsed.passed = count;
                saw_passed = true;
                saw_foldable = true;
            }
            "skipped" => {
                if saw_skipped {
                    return None;
                }
                parsed.skipped = count;
                saw_skipped = true;
                saw_foldable = true;
            }
            "xfailed" => {
                if saw_xfailed {
                    return None;
                }
                parsed.xfailed = count;
                saw_xfailed = true;
                saw_foldable = true;
            }
            _ => {}
        }
    }
    saw_foldable.then_some(parsed)
}

fn detect_line_separator(input: &str) -> Option<&'static str> {
    if input.contains('\0') || !input.contains('\n') {
        return None;
    }
    let bytes = input.as_bytes();
    let has_crlf = input.contains("\r\n");
    if has_crlf {
        for (index, byte) in bytes.iter().enumerate() {
            if (*byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
                || (*byte == b'\n'
                    && index.checked_sub(1).and_then(|at| bytes.get(at)) != Some(&b'\r'))
            {
                return None;
            }
        }
        Some("\r\n")
    } else if input.contains('\r') {
        None
    } else {
        Some("\n")
    }
}

fn parse_canonical_usize(value: &str) -> Option<usize> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return None;
    }
    value.parse().ok()
}

fn parse_canonical_u64(value: &str) -> Option<u64> {
    if value.is_empty() || (value.len() > 1 && value.starts_with('0')) {
        return None;
    }
    value.parse().ok()
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(HEX[usize::from(byte >> 4)]));
        encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    encoded
}

fn hex_decode(encoded: &str) -> Option<Vec<u8>> {
    if !encoded.len().is_multiple_of(2) || !encoded.bytes().all(is_lower_hex) {
        return None;
    }
    let mut decoded = Vec::with_capacity(encoded.len() / 2);
    for pair in encoded.as_bytes().chunks_exact(2) {
        decoded.push((hex_value(pair[0])? << 4) | hex_value(pair[1])?);
    }
    Some(decoded)
}

fn is_lower_hex(byte: u8) -> bool {
    byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use kendr_optimizer_contracts::{
        ContentEnvelope, ContentPart, Message, MessageRole, OptimizeRequest, RiskLevel,
    };

    use super::{PytestReduce, compact_pytest_output, expand_pytest_folds};
    use crate::engine::Engine;

    fn result_lines(
        prefix: &str,
        range: std::ops::RangeInclusive<u64>,
        status: &str,
        progress: &str,
    ) -> Vec<String> {
        range
            .map(|number| format!("{prefix}{number:03} {status} [{progress:>3}%]"))
            .collect()
    }

    fn joined(lines: Vec<String>) -> String {
        lines.join("\n")
    }

    #[test]
    fn folds_sequences_around_a_middle_failure_and_expands_exactly() {
        let mut lines = result_lines("tests/test_worker.py::test_case_", 0..=30, "PASSED", "42");
        lines.extend([
            "tests/test_payment.py::test_tls_chain FAILED [ 50%]".to_owned(),
            "E   AssertionError: expected status=200, actual status=526".to_owned(),
            "E   request_id=req-7f9a endpoint=https://api.example.test/v2/charges".to_owned(),
        ]);
        lines.extend(result_lines(
            "tests/test_worker.py::test_case_",
            32..=63,
            "PASSED",
            "80",
        ));
        lines.push("1 failed, 63 passed in 12.84s".to_owned());
        let input = joined(lines);

        let compact = compact_pytest_output(&input).expect("valid sequential pytest output");
        assert_eq!(compact.folded_sequences, 2);
        assert!(
            compact
                .text
                .contains("status=PASSED omitted=27 range=2..28 width=3")
        );
        assert!(
            compact
                .text
                .contains("status=PASSED omitted=28 range=34..61 width=3")
        );
        assert!(compact.text.contains("test_tls_chain FAILED [ 50%]"));
        assert!(compact.text.contains("actual status=526"));
        assert!(
            compact
                .text
                .contains("endpoint=https://api.example.test/v2/charges")
        );
        assert_eq!(
            expand_pytest_folds(&compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn preserves_a_failure_and_its_blocks_at_the_tail() {
        let mut lines = result_lines("tests/test_api.py::test_case_", 0..=40, "PASSED", "97");
        lines.extend([
            "tests/test_api.py::test_tls_chain_041 FAILED [100%]".to_owned(),
            "=================================== FAILURES ==================================="
                .to_owned(),
            "_______________________________ test_tls_chain _______________________________"
                .to_owned(),
            "E   AssertionError: status 526 != 200".to_owned(),
            "=========================== short test summary info ==========================="
                .to_owned(),
            "FAILED tests/test_api.py::test_tls_chain_041 - AssertionError".to_owned(),
            "1 failed, 41 passed in 4.20s".to_owned(),
        ]);
        let input = joined(lines);

        let compact = compact_pytest_output(&input).expect("valid sequential pytest output");
        assert!(compact.text.contains("test_tls_chain_041 FAILED [100%]"));
        assert!(compact.text.contains("FAILURES"));
        assert!(compact.text.contains("short test summary info"));
        assert!(compact.text.contains("1 failed, 41 passed in 4.20s"));
        assert_eq!(
            expand_pytest_folds(&compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn folds_sequential_skips_and_xfails_without_losing_counts_or_reasons() {
        let mut lines = result_lines("tests/test_os.py::test_skip_", 0..=19, "SKIPPED", "25");
        lines.extend(result_lines(
            "tests/test_os.py::test_known_bug_",
            20..=39,
            "XFAIL",
            "50",
        ));
        lines.extend(result_lines(
            "tests/test_os.py::test_ok_",
            40..=59,
            "PASSED",
            "75",
        ));
        lines.push("20 skipped, 20 xfailed, 20 passed in 2.00s".to_owned());
        let input = joined(lines);

        let compact = compact_pytest_output(&input).expect("valid sequential pytest output");
        assert!(
            compact
                .text
                .contains("status=SKIPPED omitted=16 range=2..17")
        );
        assert!(
            compact
                .text
                .contains("status=XFAIL omitted=16 range=22..37")
        );
        assert!(
            compact
                .text
                .contains("status=PASSED omitted=16 range=42..57")
        );
        assert!(
            compact
                .text
                .contains("20 skipped, 20 xfailed, 20 passed in 2.00s")
        );
        assert_eq!(
            expand_pytest_folds(&compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn arbitrary_or_non_sequential_node_ids_stay_unchanged() {
        let mut lines: Vec<String> = (0..30)
            .map(|number| {
                format!(
                    "tests/test_worker.py::test_case_{} PASSED [ 50%]",
                    number * 2
                )
            })
            .collect();
        lines.push("30 passed in 1.00s".to_owned());
        assert!(compact_pytest_output(&joined(lines)).is_none());
    }

    #[test]
    fn malformed_or_incomplete_pytest_output_fails_open() {
        let mut malformed =
            result_lines("tests/test_worker.py::test_case_", 0..=19, "PASSED", "50");
        malformed.push("tests/test_worker.py::test_case_020 PASSED [ nope%]".to_owned());
        malformed.push("21 passed in 1.00s".to_owned());
        assert!(compact_pytest_output(&joined(malformed)).is_none());

        let mut mismatched =
            result_lines("tests/test_worker.py::test_case_", 0..=19, "PASSED", "50");
        mismatched.push("21 passed in 1.00s".to_owned());
        assert!(compact_pytest_output(&joined(mismatched)).is_none());
    }

    #[test]
    fn marker_spoof_double_apply_and_digest_tampering_are_rejected() {
        let mut lines = result_lines("tests/test_worker.py::test_case_", 0..=39, "PASSED", "50");
        lines.push("40 passed in 1.00s".to_owned());
        let input = joined(lines);
        let compact = compact_pytest_output(&input).expect("valid sequential pytest output");

        assert!(compact_pytest_output(&compact.text).is_none());
        assert_eq!(
            expand_pytest_folds(&compact.text).as_deref(),
            Some(input.as_str())
        );

        let spoofed = format!("{input}\n[[kendr.pytest.fold:v1 user-supplied]]");
        assert!(compact_pytest_output(&spoofed).is_none());
        assert!(expand_pytest_folds(&spoofed).is_none());

        let tampered = compact.text.replacen("sha256=", "sha256=0", 1);
        assert!(expand_pytest_folds(&tampered).is_none());
    }

    #[test]
    fn preserves_crlf_and_trailing_newline_during_exact_expansion() {
        let mut lines = result_lines("tests/test_worker.py::test_case_", 0..=39, "PASSED", "50");
        lines.push("40 passed in 1.00s".to_owned());
        let input = format!("{}\r\n", lines.join("\r\n"));

        let compact = compact_pytest_output(&input).expect("valid CRLF pytest output");
        assert!(compact.text.contains("\r\n"));
        assert!(compact.text.ends_with("\r\n"));
        assert_eq!(
            expand_pytest_folds(&compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn engine_is_recoverable_and_supplies_the_original_envelope() {
        let mut lines = result_lines("tests/test_worker.py::test_case_", 0..=79, "PASSED", "50");
        lines.push("80 passed in 1.00s".to_owned());
        let content = joined(lines);
        let envelope = ContentEnvelope {
            messages: vec![Message {
                id: "tool-message".to_owned(),
                role: MessageRole::Tool,
                parent_id: None,
                turn_id: None,
                parts: vec![ContentPart::ToolResult {
                    call_id: "call-1".to_owned(),
                    name: Some("pytest".to_owned()),
                    content,
                    is_error: false,
                }],
                metadata: BTreeMap::new(),
            }],
            ..ContentEnvelope::default()
        };
        let request = OptimizeRequest {
            content: envelope.clone(),
            request_id: "pytest-reducer-test".to_owned(),
            ..serde_json::from_value(serde_json::json!({
                "schema_version": "kendr.optimize/v1",
                "phase": "tool_result",
                "content": {"messages": []}
            }))
            .expect("minimal request")
        };

        let descriptor = PytestReduce.descriptor();
        assert_eq!(descriptor.id, "pytest-result-fold");
        assert_eq!(descriptor.risk, RiskLevel::Recoverable);
        assert!(descriptor.reversible);
        assert!(!descriptor.cache_safe);

        let candidate = PytestReduce
            .propose(&request, &envelope)
            .expect("engine does not error")
            .expect("engine proposes exact fold");
        assert_eq!(candidate.touched_message_ids, ["tool-message"]);
        assert_eq!(candidate.reconstruction.as_ref(), Some(&envelope));
    }
}
