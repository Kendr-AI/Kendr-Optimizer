use std::collections::BTreeMap;

use kendr_optimizer_contracts::{ContentEnvelope, ContentPart};
use once_cell::sync::Lazy;
use regex::Regex;

static URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\bhttps?://[^\s<>"']+"#).expect("valid URL regex"));
static WINDOWS_PATH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\b[A-Za-z]:\\(?:[^\\\r\n:*?"<>|]+\\?)+\b"#).expect("valid path regex")
});
static UNIX_PATH: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?:^|\s)(/[A-Za-z0-9._~+@%=-]+(?:/[A-Za-z0-9._~+@%=-]+)+)"#)
        .expect("valid path regex")
});
static NUMBER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b-?\d+(?:\.\d+)?(?:e[+-]?\d+)?(?:ms|s|m|h|kb|mb|gb|tb|%|px|usd|eur|inr)?\b"#)
        .expect("valid number regex")
});
static IDENTIFIER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\b(?:[A-Fa-f0-9]{12,}|[A-Z][A-Z0-9_]{5,})\b"#).unwrap());
static NEGATION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)\b(?:no|not|never|without|mustn't|cannot|can't|don't|do not)\b"#).unwrap()
});
static PRESERVE_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?s)KENDR_PRESERVE_BEGIN.*?KENDR_PRESERVE_END"#).unwrap());
static ANSI_ESCAPE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])"#).expect("valid ANSI regex"));

pub(crate) fn protected_artifacts(content: &ContentEnvelope) -> BTreeMap<String, usize> {
    let mut artifacts = BTreeMap::new();
    for message in &content.messages {
        for part in &message.parts {
            match part {
                ContentPart::Text { text }
                | ContentPart::Document { text, .. }
                | ContentPart::ToolResult { content: text, .. } => {
                    collect(text, &mut artifacts);
                }
                ContentPart::Code { text, .. } => {
                    add(format!("code:{text}"), &mut artifacts);
                }
                ContentPart::ToolCall {
                    id,
                    name,
                    arguments,
                } => {
                    add(format!("call:{id}:{name}:{arguments}"), &mut artifacts);
                }
                ContentPart::Json { value } => add(format!("json:{value}"), &mut artifacts),
                ContentPart::ImageReference { uri, .. } => {
                    add(format!("image:{uri}"), &mut artifacts)
                }
            }
        }
    }
    artifacts
}

fn collect(text: &str, artifacts: &mut BTreeMap<String, usize>) {
    let sanitized = ANSI_ESCAPE.replace_all(text, "");
    let text = sanitized.as_ref();
    for regex in [
        &*URL,
        &*WINDOWS_PATH,
        &*UNIX_PATH,
        &*NUMBER,
        &*IDENTIFIER,
        &*NEGATION,
        &*PRESERVE_BLOCK,
    ] {
        for capture in regex.find_iter(text) {
            add(capture.as_str().to_owned(), artifacts);
        }
    }

    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if ["error", "fatal", "panic", "exception", "failed"]
            .iter()
            .any(|needle| lower.contains(needle))
        {
            add(format!("diagnostic:{line}"), artifacts);
        }
    }
}

fn add(artifact: String, artifacts: &mut BTreeMap<String, usize>) {
    *artifacts.entry(artifact).or_insert(0) += 1;
}
