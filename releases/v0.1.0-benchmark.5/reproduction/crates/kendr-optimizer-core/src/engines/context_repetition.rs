use std::collections::BTreeMap;

use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, MessageRole, OptimizeRequest, RiskLevel,
};

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};
use crate::tokenizer::sha256_hex;

/// The marker is deliberately ASCII, compact, low-entropy, and readable by a
/// model. `source` is the one-based ordinal of an earlier exact unit in the
/// same message part; `copies` is the number represented at this position.
/// The verifier expands every marker and compares the complete envelope
/// byte-for-byte with the pre-transform input, so visible hashes are neither a
/// security boundary nor useful token overhead.
///
/// `[[kendr.repeat unit=<paragraph|line> source=<ordinal> copies=<count>]]`
/// `[[kendr.repeat unit=block paragraphs=<count> bytes=<source-bytes> additional_copies=<count>]]`
/// `[[kendr.repeat unit=sentence bytes=<source-bytes> additional_copies=<count>]]`
const MARKER_PREFIX: &str = "[[kendr.repeat ";
const BLOCK_MARKER_PREFIX: &str = "[[kendr.repeat unit=block ";
const LINE_MARKER_PREFIX: &str = "[[kendr.repeat unit=line ";
const PARAGRAPH_MARKER_PREFIX: &str = "[[kendr.repeat unit=paragraph ";
const SENTENCE_MARKER_PREFIX: &str = "[[kendr.repeat unit=sentence ";
const MARKER_SUFFIX: &str = "]]";
const BLOCK_MARKER_BOUNDARY: &str = "\n";
const SENTENCE_MARKER_BOUNDARY: &str = " ";
const MIN_INPUT_BYTES: usize = 128;
const MIN_SAVING_BYTES: usize = 16;
const MAX_PART_BYTES: usize = 2 * 1024 * 1024;
const MAX_UNITS: usize = 8_192;

pub(crate) struct ContextRepetition;

impl Engine for ContextRepetition {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "context-repetition",
            "References exact repeated text/document blocks, paragraphs, lines, and sentence runs",
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
        let mut repeated_paragraphs = 0usize;
        let mut repeated_lines = 0usize;
        let mut repeated_sentences = 0usize;

        for message in &mut next.messages {
            if matches!(
                message.role,
                MessageRole::System | MessageRole::Developer | MessageRole::Tool
            ) {
                continue;
            }
            let mut changed = false;
            for (part_index, part) in message.parts.iter_mut().enumerate() {
                let text = match part {
                    ContentPart::Text { text } | ContentPart::Document { text, .. } => text,
                    ContentPart::Code { .. }
                    | ContentPart::Json { .. }
                    | ContentPart::ImageReference { .. }
                    | ContentPart::ToolCall { .. }
                    | ContentPart::ToolResult { .. } => continue,
                };

                let Some(compaction) = compact_context_repetitions(&message.id, part_index, text)
                else {
                    continue;
                };
                debug_assert!(compaction.text.len() < text.len());
                *text = compaction.text;
                repeated_paragraphs += compaction.repeated_paragraphs;
                repeated_lines += compaction.repeated_lines;
                repeated_sentences += compaction.repeated_sentences;
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
                "referenced {repeated_paragraphs} exact repeated paragraph(s), {repeated_lines} exact repeated line(s), and {repeated_sentences} exact repeated sentence(s) in {} message(s)",
                touched.len()
            ),
        );
        candidate.touched_message_ids = touched;
        candidate.reconstruction = Some(current.clone());
        Ok(Some(candidate))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Compaction {
    text: String,
    repeated_paragraphs: usize,
    repeated_lines: usize,
    repeated_sentences: usize,
}

/// Expands a compacted text/document part only when every typed marker is
/// canonical and refers to an earlier exact source unit. The core compares the
/// result byte-for-byte with the original part; malformed, spoofed, mixed-kind,
/// or forward-reference markers fail closed.
pub(crate) fn expand_context_repetitions(
    _message_id: &str,
    _part_index: usize,
    input: &str,
) -> Option<String> {
    if input.len() > MAX_PART_BYTES || !input.contains(MARKER_PREFIX) {
        return None;
    }

    let mut expanded = input.to_owned();
    let mut changed = false;
    if expanded.contains(BLOCK_MARKER_PREFIX) {
        // Prefix-block compaction is deliberately exclusive. Mixing marker
        // grammars would make the source-byte span depend on expansion order.
        if expanded.contains(LINE_MARKER_PREFIX)
            || expanded.contains(PARAGRAPH_MARKER_PREFIX)
            || expanded.contains(SENTENCE_MARKER_PREFIX)
        {
            return None;
        }
        return expand_block_marker(&expanded);
    }
    if expanded.contains(SENTENCE_MARKER_PREFIX) {
        expanded = expand_sentence_markers(&expanded)?;
        changed = true;
    }
    if expanded.contains(LINE_MARKER_PREFIX) {
        expanded = expand_line_markers(&expanded)?;
        changed = true;
    }
    if expanded.contains(PARAGRAPH_MARKER_PREFIX) {
        expanded = expand_paragraph_markers(&expanded)?;
        changed = true;
    }
    changed.then_some(expanded)
}

fn compact_context_repetitions(
    _message_id: &str,
    _part_index: usize,
    input: &str,
) -> Option<Compaction> {
    if input.len() < MIN_INPUT_BYTES
        || input.len() > MAX_PART_BYTES
        || input.contains(MARKER_PREFIX)
    {
        return None;
    }

    // A single prefix-block marker can be substantially cheaper than one
    // marker per short paragraph. Keep it as an exclusive candidate so its
    // byte-span proof remains simple and independently reversible.
    let mut best =
        compact_repeated_prefix_block(input).map(|(text, repeated_paragraphs)| Compaction {
            text,
            repeated_paragraphs,
            repeated_lines: 0,
            repeated_sentences: 0,
        });

    // Apply coarse paragraph references first, then fold exact adjacent line
    // runs that remain inside unique paragraphs. Expansion reverses those
    // layers and must reproduce the original bytes.
    let mut text = input.to_owned();
    let mut repeated_paragraphs = 0usize;
    let mut repeated_lines = 0usize;
    let mut repeated_sentences = 0usize;
    if let Some((compacted, count)) = compact_paragraphs(&text) {
        text = compacted;
        repeated_paragraphs = count;
    }
    if let Some((compacted, count)) = compact_line_runs(&text) {
        text = compacted;
        repeated_lines = count;
    }
    if let Some((compacted, count)) = compact_sentence_runs(&text) {
        text = compacted;
        repeated_sentences = count;
    }

    if repeated_paragraphs > 0 || repeated_lines > 0 || repeated_sentences > 0 {
        let layered = Compaction {
            text,
            repeated_paragraphs,
            repeated_lines,
            repeated_sentences,
        };
        if best
            .as_ref()
            .is_none_or(|current| layered.text.len() < current.text.len())
        {
            best = Some(layered);
        }
    }

    best.filter(|candidate| candidate.text.len() + MIN_SAVING_BYTES <= input.len())
}

fn compact_repeated_prefix_block(input: &str) -> Option<(String, usize)> {
    let segments = paragraph_segments(input)?;
    let mut candidates = Vec::new();
    let mut cursor = 0usize;
    let mut paragraphs = 0usize;

    // Exact periods are considered only at paragraph starts. That keeps the
    // search bounded and prevents arbitrary substring compression.
    for segment in segments {
        if segment.kind == SegmentKind::Paragraph {
            if cursor > 0 && cursor <= input.len() / 2 {
                candidates.push((cursor, paragraphs));
            }
            paragraphs += 1;
        }
        cursor = cursor.checked_add(segment.text.len())?;
    }

    let prefix_matches = prefix_match_lengths(input.as_bytes());
    let mut best: Option<(usize, usize, usize, usize)> = None;
    for (period_bytes, paragraphs_per_block) in candidates {
        if paragraphs_per_block == 0 || !input.is_char_boundary(period_bytes) {
            continue;
        }

        let repeats = 1 + prefix_matches[period_bytes] / period_bytes;
        if repeats < 2 {
            continue;
        }

        let omitted_copies = repeats - 1;
        let marker = block_marker(period_bytes, paragraphs_per_block, omitted_copies);
        let repeated_end = period_bytes.checked_mul(repeats)?;
        let output_len = period_bytes
            .checked_add(marker.len())?
            .checked_add(input.len().checked_sub(repeated_end)?)?;
        if output_len + MIN_SAVING_BYTES > input.len() {
            continue;
        }

        let omitted_paragraphs = paragraphs_per_block.checked_mul(omitted_copies)?;

        if best
            .as_ref()
            .is_none_or(|(_, _, _, current_len)| output_len < *current_len)
        {
            best = Some((period_bytes, repeats, omitted_paragraphs, output_len));
        }
    }

    let (period_bytes, repeats, omitted_paragraphs, output_len) = best?;
    let paragraphs_per_block = omitted_paragraphs / (repeats - 1);
    let marker = block_marker(period_bytes, paragraphs_per_block, repeats - 1);
    let repeated_end = period_bytes.checked_mul(repeats)?;
    let mut output = String::with_capacity(output_len);
    output.push_str(&input[..period_bytes]);
    output.push_str(&marker);
    output.push_str(&input[repeated_end..]);
    Some((output, omitted_paragraphs))
}

/// Computes the exact common-prefix length for every byte offset in linear
/// time (the Z algorithm), avoiding one full input rescan per paragraph.
fn prefix_match_lengths(input: &[u8]) -> Vec<usize> {
    let mut matches = vec![0usize; input.len()];
    if input.is_empty() {
        return matches;
    }
    matches[0] = input.len();
    let mut left = 0usize;
    let mut right = 0usize;

    for index in 1..input.len() {
        if index < right {
            matches[index] = (right - index).min(matches[index - left]);
        }
        while index + matches[index] < input.len()
            && input[matches[index]] == input[index + matches[index]]
        {
            matches[index] += 1;
        }
        if index + matches[index] > right {
            left = index;
            right = index + matches[index];
        }
    }
    matches
}

fn expand_block_marker(input: &str) -> Option<String> {
    let marker_start = input.find(BLOCK_MARKER_PREFIX)?;
    if input[..marker_start].contains(MARKER_PREFIX) {
        return None;
    }
    let marker_tail = &input[marker_start..];
    let marker_syntax_end = marker_start
        .checked_add(marker_tail.find(MARKER_SUFFIX)?)?
        .checked_add(MARKER_SUFFIX.len())?;
    if !input[marker_syntax_end..].starts_with(BLOCK_MARKER_BOUNDARY) {
        return None;
    }
    let marker_end = marker_syntax_end.checked_add(BLOCK_MARKER_BOUNDARY.len())?;
    if input[marker_end..].contains(MARKER_PREFIX) {
        return None;
    }

    let parsed = parse_block_marker(&input[marker_start..marker_syntax_end])?;
    let source_start = marker_start.checked_sub(parsed.bytes)?;
    if !input.is_char_boundary(source_start) || !input.is_char_boundary(marker_start) {
        return None;
    }
    let source = &input[source_start..marker_start];
    if source.len() != parsed.bytes || source.contains(MARKER_PREFIX) {
        return None;
    }
    let source_paragraphs = paragraph_segments(source)?
        .iter()
        .filter(|segment| segment.kind == SegmentKind::Paragraph)
        .count();
    if source_paragraphs != parsed.paragraphs {
        return None;
    }

    let marker_len = marker_end.checked_sub(marker_start)?;
    let inserted_len = parsed.bytes.checked_mul(parsed.copies)?;
    let expanded_len = input
        .len()
        .checked_sub(marker_len)?
        .checked_add(inserted_len)?;
    if expanded_len > MAX_PART_BYTES {
        return None;
    }

    let repeated_source = source.repeat(parsed.copies);
    let mut output = String::with_capacity(expanded_len);
    output.push_str(&input[..marker_start]);
    output.push_str(&repeated_source);
    output.push_str(&input[marker_end..]);
    Some(output)
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedBlockMarker {
    bytes: usize,
    paragraphs: usize,
    copies: usize,
}

fn parse_block_marker(input: &str) -> Option<ParsedBlockMarker> {
    let body = input
        .strip_prefix(BLOCK_MARKER_PREFIX)?
        .strip_suffix(MARKER_SUFFIX)?;
    let fields: Vec<&str> = body.split_whitespace().collect();
    if fields.len() != 3 {
        return None;
    }

    let paragraphs = canonical_positive(fields[0].strip_prefix("paragraphs=")?)?;
    let bytes = canonical_positive(fields[1].strip_prefix("bytes=")?)?;
    let copies = canonical_positive(fields[2].strip_prefix("additional_copies=")?)?;
    if paragraphs > MAX_UNITS || bytes > MAX_PART_BYTES || copies > MAX_UNITS {
        return None;
    }
    let parsed = ParsedBlockMarker {
        bytes,
        paragraphs,
        copies,
    };
    (block_marker(bytes, paragraphs, copies).trim_end_matches(BLOCK_MARKER_BOUNDARY) == input)
        .then_some(parsed)
}

fn block_marker(bytes: usize, paragraphs: usize, copies: usize) -> String {
    format!(
        "{BLOCK_MARKER_PREFIX}paragraphs={paragraphs} bytes={bytes} additional_copies={copies}{MARKER_SUFFIX}{BLOCK_MARKER_BOUNDARY}"
    )
}

fn expand_sentence_markers(input: &str) -> Option<String> {
    let mut output = input.to_owned();
    let mut cursor = 0usize;
    let mut expanded = 0usize;

    while let Some(relative_start) = output[cursor..].find(SENTENCE_MARKER_PREFIX) {
        let marker_start = cursor.checked_add(relative_start)?;
        let marker_syntax_end = marker_start
            .checked_add(output[marker_start..].find(MARKER_SUFFIX)?)?
            .checked_add(MARKER_SUFFIX.len())?;
        if !output[marker_syntax_end..].starts_with(SENTENCE_MARKER_BOUNDARY) {
            return None;
        }
        let marker_end = marker_syntax_end.checked_add(SENTENCE_MARKER_BOUNDARY.len())?;
        let parsed = parse_sentence_marker(&output[marker_start..marker_syntax_end])?;
        let source_start = marker_start.checked_sub(parsed.bytes)?;
        if !output.is_char_boundary(source_start) || !output.is_char_boundary(marker_start) {
            return None;
        }

        let source = &output[source_start..marker_start];
        let source_without_spacing = source.trim_end_matches([' ', '\t']);
        if source.len() != parsed.bytes
            || source.contains(MARKER_PREFIX)
            || !source_without_spacing.ends_with(['.', '!', '?'])
        {
            return None;
        }

        let marker_len = marker_end.checked_sub(marker_start)?;
        let inserted_len = parsed.bytes.checked_mul(parsed.copies)?;
        let expanded_len = output
            .len()
            .checked_sub(marker_len)?
            .checked_add(inserted_len)?;
        if expanded_len > MAX_PART_BYTES {
            return None;
        }

        let repeated_source = source.repeat(parsed.copies);
        output.replace_range(marker_start..marker_end, &repeated_source);
        cursor = marker_start.checked_add(inserted_len)?;
        expanded += 1;
        if expanded > MAX_UNITS {
            return None;
        }
    }

    (expanded > 0).then_some(output)
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedSentenceMarker {
    bytes: usize,
    copies: usize,
}

fn parse_sentence_marker(input: &str) -> Option<ParsedSentenceMarker> {
    let body = input
        .strip_prefix(SENTENCE_MARKER_PREFIX)?
        .strip_suffix(MARKER_SUFFIX)?;
    let fields: Vec<&str> = body.split_whitespace().collect();
    if fields.len() != 2 {
        return None;
    }

    let bytes = canonical_positive(fields[0].strip_prefix("bytes=")?)?;
    let copies = canonical_positive(fields[1].strip_prefix("additional_copies=")?)?;
    if bytes > MAX_PART_BYTES || copies > MAX_UNITS {
        return None;
    }
    let parsed = ParsedSentenceMarker { bytes, copies };
    (sentence_marker(bytes, copies).trim_end_matches(SENTENCE_MARKER_BOUNDARY) == input)
        .then_some(parsed)
}

fn sentence_marker(bytes: usize, copies: usize) -> String {
    format!(
        "{SENTENCE_MARKER_PREFIX}bytes={bytes} additional_copies={copies}{MARKER_SUFFIX}{SENTENCE_MARKER_BOUNDARY}"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UnitKind {
    Paragraph,
    Line,
}

impl UnitKind {
    fn name(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Line => "line",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Segment<'a> {
    kind: SegmentKind,
    text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentKind {
    Paragraph,
    Separator,
}

#[derive(Debug, Clone, Copy)]
struct LineSpan {
    start: usize,
    content_end: usize,
    end: usize,
    blank: bool,
}

#[derive(Debug)]
struct SeenParagraph {
    exact: String,
    source_ordinal: usize,
}

fn compact_paragraphs(input: &str) -> Option<(String, usize)> {
    let segments = paragraph_segments(input)?;
    let paragraph_count = segments
        .iter()
        .filter(|segment| segment.kind == SegmentKind::Paragraph)
        .count();
    if !(2..=MAX_UNITS).contains(&paragraph_count) {
        return None;
    }

    let mut seen: BTreeMap<String, Vec<SeenParagraph>> = BTreeMap::new();
    let mut output = String::with_capacity(input.len());
    let mut ordinal = 0usize;
    let mut replaced = 0usize;

    for segment in segments {
        if segment.kind == SegmentKind::Separator {
            output.push_str(segment.text);
            continue;
        }

        ordinal += 1;
        let digest = sha256_hex(segment.text.as_bytes());
        let bucket = seen.entry(digest.clone()).or_default();
        let prior = bucket.iter_mut().find(|prior| prior.exact == segment.text);

        if let Some(prior) = prior {
            let marker = marker(UnitKind::Paragraph, prior.source_ordinal, 1);
            if marker.len() + MIN_SAVING_BYTES <= segment.text.len() {
                output.push_str(&marker);
                replaced += 1;
            } else {
                output.push_str(segment.text);
            }
        } else {
            bucket.push(SeenParagraph {
                exact: segment.text.to_owned(),
                source_ordinal: ordinal,
            });
            output.push_str(segment.text);
        }
    }

    (replaced > 0 && output.len() + MIN_SAVING_BYTES <= input.len()).then_some((output, replaced))
}

fn compact_line_runs(input: &str) -> Option<(String, usize)> {
    let lines: Vec<&str> = input.split('\n').collect();
    if lines.len() < 2 || lines.len() > MAX_UNITS {
        return None;
    }

    let mut output = Vec::with_capacity(lines.len());
    let mut index = 0usize;
    let mut omitted_total = 0usize;

    while index < lines.len() {
        let line = lines[index];
        let mut end = index + 1;
        while end < lines.len() && lines[end] == line {
            end += 1;
        }

        let run = end - index;
        if run >= 2 && !line.trim().is_empty() {
            let mut typed_marker = marker(UnitKind::Line, index + 1, run - 1);
            if line.ends_with('\r') {
                typed_marker.push('\r');
            }

            // Account for the newline between the retained source and marker.
            let original_bytes = run * line.len() + run.saturating_sub(1);
            let compact_bytes = line.len() + 1 + typed_marker.len();
            if compact_bytes + MIN_SAVING_BYTES <= original_bytes {
                output.push(line.to_owned());
                output.push(typed_marker);
                omitted_total += run - 1;
                index = end;
                continue;
            }
        }

        for original in &lines[index..end] {
            output.push((*original).to_owned());
        }
        index = end;
    }

    if omitted_total == 0 {
        return None;
    }
    let output = output.join("\n");
    (output.len() + MIN_SAVING_BYTES <= input.len()).then_some((output, omitted_total))
}

fn compact_sentence_runs(input: &str) -> Option<(String, usize)> {
    let lines = line_spans(input);
    if lines.len() > MAX_UNITS {
        return None;
    }

    let mut output = String::with_capacity(input.len());
    let mut omitted_total = 0usize;
    let mut sentence_count = 0usize;
    let mut in_fence = false;

    for line in lines {
        let content = &input[line.start..line.content_end];
        let ending = &input[line.content_end..line.end];
        let trimmed = content.trim_start();
        let fence_boundary = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        if fence_boundary {
            in_fence = !in_fence;
        }
        if in_fence || fence_boundary || content.contains(MARKER_PREFIX) {
            output.push_str(&input[line.start..line.end]);
            continue;
        }

        let sentences = sentence_segments(content);
        sentence_count = sentence_count.checked_add(sentences.len())?;
        if sentence_count > MAX_UNITS {
            return None;
        }

        let mut index = 0usize;
        while index < sentences.len() {
            let sentence = sentences[index];
            let mut end = index + 1;
            while end < sentences.len() && sentences[end] == sentence {
                end += 1;
            }

            let run = end - index;
            // Two copies can be deliberate rhetorical emphasis. Require at
            // least three exact adjacent sentences before representing the
            // repetition as a count.
            if run >= 3 && sentence.trim().len() >= 32 {
                let typed_marker = sentence_marker(sentence.len(), run - 1);
                let original_bytes = sentence.len().checked_mul(run)?;
                let compact_bytes = sentence.len().checked_add(typed_marker.len())?;
                if compact_bytes + MIN_SAVING_BYTES <= original_bytes {
                    output.push_str(sentence);
                    output.push_str(&typed_marker);
                    omitted_total += run - 1;
                    index = end;
                    continue;
                }
            }

            for original in &sentences[index..end] {
                output.push_str(original);
            }
            index = end;
        }
        output.push_str(ending);
    }

    (omitted_total > 0 && output.len() + MIN_SAVING_BYTES <= input.len())
        .then_some((output, omitted_total))
}

fn sentence_segments(input: &str) -> Vec<&str> {
    let bytes = input.as_bytes();
    let mut segments = Vec::new();
    let mut start = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        if !matches!(bytes[index], b'.' | b'!' | b'?') {
            index += 1;
            continue;
        }

        let after_punctuation = index + 1;
        if after_punctuation < bytes.len() && !matches!(bytes[after_punctuation], b' ' | b'\t') {
            index += 1;
            continue;
        }

        let mut end = after_punctuation;
        while end < bytes.len() && matches!(bytes[end], b' ' | b'\t') {
            end += 1;
        }
        segments.push(&input[start..end]);
        start = end;
        index = end;
    }

    if start < input.len() {
        segments.push(&input[start..]);
    }
    segments
}

fn paragraph_segments(input: &str) -> Option<Vec<Segment<'_>>> {
    let lines = line_spans(input);
    if lines.len() > MAX_UNITS {
        return None;
    }

    let mut segments = Vec::new();
    let mut paragraph_start = None;
    let mut separator_start = None;
    let mut last_content_end = 0usize;

    for line in lines {
        if line.blank {
            if let Some(start) = paragraph_start.take() {
                segments.push(Segment {
                    kind: SegmentKind::Paragraph,
                    text: &input[start..last_content_end],
                });
                separator_start = Some(last_content_end);
            } else if separator_start.is_none() {
                separator_start = Some(line.start);
            }
        } else {
            if let Some(start) = separator_start.take() {
                segments.push(Segment {
                    kind: SegmentKind::Separator,
                    text: &input[start..line.start],
                });
            }
            paragraph_start.get_or_insert(line.start);
            last_content_end = line.content_end;
        }
    }

    if let Some(start) = paragraph_start {
        segments.push(Segment {
            kind: SegmentKind::Paragraph,
            text: &input[start..last_content_end],
        });
        if last_content_end < input.len() {
            segments.push(Segment {
                kind: SegmentKind::Separator,
                text: &input[last_content_end..],
            });
        }
    } else if let Some(start) = separator_start {
        segments.push(Segment {
            kind: SegmentKind::Separator,
            text: &input[start..],
        });
    }

    Some(segments)
}

fn line_spans(input: &str) -> Vec<LineSpan> {
    let bytes = input.as_bytes();
    let mut spans = Vec::new();
    let mut start = 0usize;

    for (index, byte) in bytes.iter().enumerate() {
        if *byte != b'\n' {
            continue;
        }
        let content_end = if index > start && bytes[index - 1] == b'\r' {
            index - 1
        } else {
            index
        };
        spans.push(LineSpan {
            start,
            content_end,
            end: index + 1,
            blank: input[start..content_end].trim().is_empty(),
        });
        start = index + 1;
    }

    if start < input.len() {
        spans.push(LineSpan {
            start,
            content_end: input.len(),
            end: input.len(),
            blank: input[start..].trim().is_empty(),
        });
    }

    // `end` is part of the invariant checked here even though paragraph
    // segmentation only needs starts and content ends.
    debug_assert!(spans.windows(2).all(|pair| pair[0].end == pair[1].start));
    spans
}

fn expand_paragraph_markers(input: &str) -> Option<String> {
    let segments = paragraph_segments(input)?;
    let mut source_by_ordinal: BTreeMap<usize, String> = BTreeMap::new();
    let mut output = String::with_capacity(input.len());
    let mut ordinal = 0usize;
    let mut expanded = 0usize;

    for segment in segments {
        if segment.kind == SegmentKind::Separator {
            output.push_str(segment.text);
            continue;
        }
        ordinal += 1;

        if segment.text.contains(MARKER_PREFIX) {
            let parsed = parse_marker(segment.text)?;
            if parsed.kind != UnitKind::Paragraph
                || parsed.copies != 1
                || parsed.source_ordinal >= ordinal
            {
                return None;
            }
            let source = source_by_ordinal.get(&parsed.source_ordinal)?;
            output.push_str(source);
            if output.len() > MAX_PART_BYTES {
                return None;
            }
            expanded += 1;
        } else {
            source_by_ordinal.insert(ordinal, segment.text.to_owned());
            output.push_str(segment.text);
            if output.len() > MAX_PART_BYTES {
                return None;
            }
        }
    }

    (expanded > 0).then_some(output)
}

fn expand_line_markers(input: &str) -> Option<String> {
    let lines: Vec<&str> = input.split('\n').collect();
    if lines.len() > MAX_UNITS {
        return None;
    }

    let mut source_by_ordinal: BTreeMap<usize, String> = BTreeMap::new();
    let mut output = Vec::with_capacity(lines.len());
    let mut output_bytes = 0usize;
    let mut original_ordinal = 0usize;
    let mut expanded = 0usize;

    for line in lines {
        if line.contains(MARKER_PREFIX) {
            let marker_text = line.strip_suffix('\r').unwrap_or(line);
            let parsed = parse_marker(marker_text)?;
            if parsed.kind == UnitKind::Paragraph {
                original_ordinal += 1;
                source_by_ordinal.insert(original_ordinal, line.to_owned());
                output_bytes = output_bytes.checked_add(line.len().saturating_add(1))?;
                if output_bytes > MAX_PART_BYTES {
                    return None;
                }
                output.push(line.to_owned());
                continue;
            }
            if parsed.kind != UnitKind::Line || parsed.source_ordinal > original_ordinal {
                return None;
            }
            let source = source_by_ordinal.get(&parsed.source_ordinal)?;
            let expanded_ordinal = original_ordinal.checked_add(parsed.copies)?;
            if expanded_ordinal > MAX_UNITS {
                return None;
            }
            for _ in 0..parsed.copies {
                output_bytes = output_bytes
                    .checked_add(source.len())?
                    .checked_add(usize::from(!output.is_empty()))?;
                if output_bytes > MAX_PART_BYTES {
                    return None;
                }
                output.push(source.clone());
                original_ordinal += 1;
            }
            expanded += 1;
        } else {
            original_ordinal += 1;
            source_by_ordinal.insert(original_ordinal, line.to_owned());
            output_bytes = output_bytes
                .checked_add(line.len())?
                .checked_add(usize::from(!output.is_empty()))?;
            if output_bytes > MAX_PART_BYTES {
                return None;
            }
            output.push(line.to_owned());
        }
    }

    (expanded > 0).then(|| output.join("\n"))
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedMarker {
    kind: UnitKind,
    source_ordinal: usize,
    copies: usize,
}

fn parse_marker(input: &str) -> Option<ParsedMarker> {
    let body = input
        .strip_prefix(MARKER_PREFIX)?
        .strip_suffix(MARKER_SUFFIX)?;
    let fields: Vec<&str> = body.split_whitespace().collect();
    if fields.len() != 3 {
        return None;
    }

    let kind = match fields[0].strip_prefix("unit=")? {
        "paragraph" => UnitKind::Paragraph,
        "line" => UnitKind::Line,
        _ => return None,
    };
    let source_ordinal = canonical_positive(fields[1].strip_prefix("source=")?)?;
    let copies = canonical_positive(fields[2].strip_prefix("copies=")?)?;

    if source_ordinal > MAX_UNITS || copies > MAX_UNITS {
        return None;
    }
    let parsed = ParsedMarker {
        kind,
        source_ordinal,
        copies,
    };
    (marker(kind, source_ordinal, copies) == input).then_some(parsed)
}

fn canonical_positive(input: &str) -> Option<usize> {
    let value = input.parse::<usize>().ok()?;
    (value > 0 && value.to_string() == input).then_some(value)
}

fn marker(kind: UnitKind, source_ordinal: usize, copies: usize) -> String {
    format!(
        "{MARKER_PREFIX}unit={} source={source_ordinal} copies={copies}{MARKER_SUFFIX}",
        kind.name()
    )
}

#[cfg(test)]
mod tests {
    use kendr_optimizer_contracts::{
        ContentEnvelope, ContentPart, Message, MessageRole, OptimizePhase, OptimizeRequest,
        SCHEMA_VERSION,
    };
    use serde_json::json;

    use super::{
        ContextRepetition, MARKER_PREFIX, UnitKind, compact_context_repetitions,
        expand_context_repetitions, parse_marker,
    };
    use crate::engine::Engine;

    fn long_paragraph(label: &str) -> String {
        format!(
            "{label}: This paragraph deliberately contains enough exact UTF-8 text to make a typed repetition marker smaller than the source. It must remain byte-for-byte reconstructable without a language model or network call. {}",
            "bounded deterministic context ".repeat(5)
        )
    }

    #[test]
    fn short_or_unique_text_is_a_no_op() {
        assert!(compact_context_repetitions("m1", 0, "brief brief").is_none());
        assert!(compact_context_repetitions("m1", 0, &long_paragraph("only once")).is_none());
    }

    #[test]
    fn keeps_one_exact_paragraph_and_expands_byte_for_byte() {
        let repeated = long_paragraph("same");
        let input = format!("{repeated}\n\n{repeated}\n\n{repeated}\n");
        let compact = compact_context_repetitions("message-a", 2, &input).unwrap();

        assert_eq!(compact.repeated_paragraphs, 2);
        assert_eq!(compact.repeated_lines, 0);
        assert_eq!(compact.text.matches(&repeated).count(), 1);
        assert_eq!(compact.text.matches(MARKER_PREFIX).count(), 2);
        assert_eq!(
            expand_context_repetitions("message-a", 2, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn folds_a_repeated_prefix_block_into_one_marker() {
        let first = long_paragraph("first block paragraph");
        let second = long_paragraph("second block paragraph");
        let block = format!("{first}\n\n{second}\n\n");
        let input = format!("{}unique suffix", block.repeat(12));
        let compact = compact_context_repetitions("prefix-block", 0, &input).unwrap();

        assert_eq!(compact.text.matches("unit=block").count(), 1);
        assert_eq!(compact.repeated_paragraphs, 22);
        assert!(compact.text.len() * 3 < input.len());
        assert!(compact.text.contains("]]\nunique suffix"));
        assert_eq!(
            expand_context_repetitions("prefix-block", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn authored_redundant_prose_uses_a_single_exact_block_marker() {
        let corpus_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/corpus/authored/v1/cases.json");
        let corpus: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(corpus_path).unwrap()).unwrap();
        let input = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == "redundant-prose")
            .and_then(|case| case["text"].as_str())
            .unwrap();

        let compact = compact_context_repetitions("redundant-prose-payload", 0, input).unwrap();

        assert_eq!(compact.text.matches("unit=block").count(), 1);
        assert!(compact.text.len() * 4 < input.len());
        assert_eq!(
            expand_context_repetitions("redundant-prose-payload", 0, &compact.text).as_deref(),
            Some(input)
        );
    }

    #[test]
    fn large_periodic_input_uses_bounded_prefix_detection() {
        let paragraph = "A deterministic repeated paragraph has enough content for profitable exact compaction.";
        let input = std::iter::repeat_n(paragraph, 4_096)
            .collect::<Vec<_>>()
            .join("\n\n");

        let compact = compact_context_repetitions("large-periodic", 0, &input).unwrap();

        assert!(compact.text.contains("unit=block"));
        assert_eq!(
            expand_context_repetitions("large-periodic", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn compacts_profitable_adjacent_line_runs() {
        let line = "worker-17 emitted the same deterministic diagnostic payload ".repeat(5);
        let input = std::iter::repeat_n(line.as_str(), 12)
            .collect::<Vec<_>>()
            .join("\n");
        let compact = compact_context_repetitions("line-message", 0, &input).unwrap();

        assert_eq!(compact.repeated_lines, 11);
        assert_eq!(compact.text.matches(&line).count(), 1);
        assert_eq!(
            expand_context_repetitions("line-message", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn compacts_exact_adjacent_sentence_runs() {
        let sentence = "The staging workers were healthy and no production action was approved. ";
        let input = format!("Document 2\n{}unique tail", sentence.repeat(8));
        let compact = compact_context_repetitions("sentence-run", 0, &input).unwrap();

        assert_eq!(compact.repeated_sentences, 7);
        assert_eq!(compact.text.matches(sentence).count(), 1);
        assert!(compact.text.contains("unit=sentence"));
        assert!(compact.text.contains("additional_copies=7"));
        assert!(compact.text.contains("]] unique tail"));
        assert_eq!(
            expand_context_repetitions("sentence-run", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn sentence_markers_preserve_crlf_and_reject_noncanonical_counts() {
        let sentence = "The exact UTF-8 sentence remains stable across every represented copy. ";
        let input = format!("{}\r\ntrailing line\r\n", sentence.repeat(8));
        let compact = compact_context_repetitions("sentence-crlf", 0, &input).unwrap();

        assert_eq!(
            expand_context_repetitions("sentence-crlf", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
        let malformed = compact
            .text
            .replace("additional_copies=7", "additional_copies=07");
        assert!(expand_context_repetitions("sentence-crlf", 0, &malformed).is_none());
    }

    #[test]
    fn authored_rag_fixture_compacts_sentence_runs_and_expands_exactly() {
        let corpus_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/corpus/authored/v1/cases.json");
        let corpus: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(corpus_path).unwrap()).unwrap();
        let input = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == "rag-incident")
            .and_then(|case| case["text"].as_str())
            .unwrap();

        let compact = compact_context_repetitions("rag-incident-payload", 0, input).unwrap();

        assert!(compact.repeated_sentences > 0);
        assert!(compact.text.len() * 2 < input.len());
        assert_eq!(
            expand_context_repetitions("rag-incident-payload", 0, &compact.text).as_deref(),
            Some(input)
        );
    }

    #[test]
    fn sentence_runs_do_not_cross_fences_or_fold_two_copy_emphasis() {
        let sentence = "This exact instruction is intentionally emphasized for the operator. ";
        let fenced = format!("```text\n{}\n```", sentence.repeat(8));
        assert!(compact_context_repetitions("fenced", 0, &fenced).is_none());

        let emphasized = sentence.repeat(2);
        assert!(compact_context_repetitions("emphasis", 0, &emphasized).is_none());
    }

    #[test]
    fn ordinary_unit_words_do_not_poison_typed_marker_dispatch() {
        let sentence = "The configuration unit=block remains an ordinary literal sentence. ";
        let input = format!("{}tail", sentence.repeat(6));
        let compact = compact_context_repetitions("dispatch", 0, &input).unwrap();

        assert!(compact.text.contains("unit=sentence"));
        assert_eq!(
            expand_context_repetitions("dispatch", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn folds_repeated_comment_runs_inside_unique_blocks() {
        let mut blocks = Vec::new();
        for index in 0..30 {
            let mut block = vec![format!("// generated commentary block {index}")];
            block.extend(std::iter::repeat_n(
                "// ordinary implementation note".to_owned(),
                9,
            ));
            blocks.push(block.join("\n"));
        }
        let input = blocks.join("\n\n");

        let compact = compact_context_repetitions("code-context", 0, &input).unwrap();

        assert_eq!(compact.repeated_paragraphs, 0);
        assert_eq!(compact.repeated_lines, 240);
        assert_eq!(
            expand_context_repetitions("code-context", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn authored_code_fixture_compacts_and_expands_exactly() {
        let corpus_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../benchmarks/corpus/authored/v1/cases.json");
        let corpus: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(corpus_path).unwrap()).unwrap();
        let input = corpus["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|case| case["id"] == "code-context")
            .and_then(|case| case["text"].as_str())
            .unwrap();

        let compact = compact_context_repetitions("code-context-payload", 0, input).unwrap();

        assert!(compact.repeated_paragraphs > 0 || compact.repeated_lines > 0);
        assert_eq!(
            expand_context_repetitions("code-context-payload", 0, &compact.text).as_deref(),
            Some(input)
        );
    }

    #[test]
    fn handles_multilingual_unicode_without_changing_bytes() {
        let repeated = format!(
            "हिन्दी निर्देश — 日本語の文脈 — مرحباً بالعالم — emoji 🧭🔒. {}",
            "डेटा को बिल्कुल सुरक्षित रखें। ".repeat(12)
        );
        let input = format!("शीर्षक\n\n{repeated}\n\nअंतराल\n\n{repeated}\n");
        let compact = compact_context_repetitions("unicode-消息", 1, &input).unwrap();

        assert_eq!(
            expand_context_repetitions("unicode-消息", 1, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn rejects_marker_spoof_and_does_not_double_apply() {
        let repeated = long_paragraph("same");
        let spoof =
            format!("prefix {MARKER_PREFIX}not-a-marker suffix\n\n{repeated}\n\n{repeated}");
        assert!(compact_context_repetitions("m", 0, &spoof).is_none());

        let input = format!("{repeated}\n\n{repeated}");
        let once = compact_context_repetitions("m", 0, &input).unwrap();
        assert!(compact_context_repetitions("m", 0, &once.text).is_none());
        let tampered = once.text.replace("source=1", "source=9");
        assert!(expand_context_repetitions("m", 0, &tampered).is_none());
    }

    #[test]
    fn compacts_nonadjacent_multiline_blocks() {
        let repeated = format!(
            "first line of the block\nsecond line with exact content\n{}",
            "third line remains identical and sufficiently long. ".repeat(7)
        );
        let middle = long_paragraph("different middle paragraph");
        let input = format!("{repeated}\n\n{middle}\n\n{repeated}");
        let compact = compact_context_repetitions("blocks", 0, &input).unwrap();

        assert_eq!(compact.repeated_paragraphs, 1);
        assert!(compact.text.contains(&middle));
        assert_eq!(
            expand_context_repetitions("blocks", 0, &compact.text).as_deref(),
            Some(input.as_str())
        );
    }

    #[test]
    fn marker_parser_enforces_canonical_machine_grammar() {
        let repeated = long_paragraph("same");
        let input = format!("{repeated}\n\n{repeated}");
        let compact = compact_context_repetitions("grammar", 0, &input).unwrap();
        let marker = compact
            .text
            .split("\n\n")
            .find(|part| part.starts_with(MARKER_PREFIX))
            .unwrap();
        let parsed = parse_marker(marker).unwrap();

        assert_eq!(parsed.kind, UnitKind::Paragraph);
        assert_eq!(parsed.source_ordinal, 1);
        assert_eq!(parsed.copies, 1);
        assert!(parse_marker(&marker.replace("source=1", "source=01")).is_none());
        assert!(parse_marker(&marker.replace(" source=", "  source=")).is_none());
        assert!(parse_marker(&marker.replace(" source=", "\tsource=")).is_none());
    }

    #[test]
    fn engine_only_changes_text_and_document_and_supplies_reconstruction() {
        let repeated = long_paragraph("typed");
        let repeated_text = format!("{repeated}\n\n{repeated}");
        let original = ContentEnvelope {
            messages: vec![Message {
                id: "typed-message".to_owned(),
                role: MessageRole::User,
                parent_id: None,
                turn_id: None,
                parts: vec![
                    ContentPart::Text {
                        text: repeated_text.clone(),
                    },
                    ContentPart::Document {
                        media_type: Some("text/plain".to_owned()),
                        text: repeated_text.clone(),
                    },
                    ContentPart::Code {
                        language: Some("rust".to_owned()),
                        text: repeated_text.clone(),
                    },
                    ContentPart::Json {
                        value: json!({"unchanged": repeated_text}),
                    },
                    ContentPart::ToolCall {
                        id: "call-1".to_owned(),
                        name: "unchanged".to_owned(),
                        arguments: json!({"text": repeated}),
                    },
                    ContentPart::ToolResult {
                        call_id: "call-1".to_owned(),
                        name: Some("unchanged".to_owned()),
                        content: repeated_text,
                        is_error: false,
                    },
                ],
                metadata: Default::default(),
            }],
            ..ContentEnvelope::default()
        };
        let request = OptimizeRequest {
            schema_version: SCHEMA_VERSION.to_owned(),
            phase: OptimizePhase::Request,
            request_id: "request".to_owned(),
            session_id: None,
            content: original.clone(),
            target: Default::default(),
            generation: Default::default(),
            host_capabilities: Default::default(),
            policy: Default::default(),
        };

        let candidate = ContextRepetition
            .propose(&request, &original)
            .unwrap()
            .unwrap();
        assert_eq!(candidate.reconstruction.as_ref(), Some(&original));
        assert!(matches!(
            &candidate.content.messages[0].parts[0],
            ContentPart::Text { text } if text.contains(MARKER_PREFIX)
        ));
        assert!(matches!(
            &candidate.content.messages[0].parts[1],
            ContentPart::Document { text, .. } if text.contains(MARKER_PREFIX)
        ));
        assert_eq!(
            candidate.content.messages[0].parts[2],
            original.messages[0].parts[2]
        );
        assert_eq!(
            candidate.content.messages[0].parts[3],
            original.messages[0].parts[3]
        );
        assert_eq!(
            candidate.content.messages[0].parts[4],
            original.messages[0].parts[4]
        );
        assert_eq!(
            candidate.content.messages[0].parts[5],
            original.messages[0].parts[5]
        );
    }

    #[test]
    fn engine_excludes_protocol_sensitive_message_roles() {
        let repeated = long_paragraph("protocol instruction");
        let repeated_text = format!("{repeated}\n\n{repeated}\n\n{repeated}");

        for (index, role) in [
            MessageRole::System,
            MessageRole::Developer,
            MessageRole::Tool,
        ]
        .into_iter()
        .enumerate()
        {
            let original = ContentEnvelope {
                messages: vec![Message {
                    id: format!("protocol-{index}"),
                    role,
                    parent_id: None,
                    turn_id: None,
                    parts: vec![ContentPart::Text {
                        text: repeated_text.clone(),
                    }],
                    metadata: Default::default(),
                }],
                ..ContentEnvelope::default()
            };
            let request = OptimizeRequest {
                schema_version: SCHEMA_VERSION.to_owned(),
                phase: OptimizePhase::Request,
                request_id: format!("protocol-request-{index}"),
                session_id: None,
                content: original.clone(),
                target: Default::default(),
                generation: Default::default(),
                host_capabilities: Default::default(),
                policy: Default::default(),
            };

            assert!(
                ContextRepetition
                    .propose(&request, &original)
                    .unwrap()
                    .is_none()
            );
        }
    }
}
