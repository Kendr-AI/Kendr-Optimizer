use std::collections::{BTreeMap, BTreeSet};

use kendr_optimizer_contracts::{
    ContentEnvelope, ContentPart, EngineDescriptor, MessageRole, OptimizeRequest, RiskLevel,
    ToolDefinition,
};

use crate::engine::{Candidate, Engine, OptimizeError, descriptor};

pub(crate) struct ToolSelector;

impl Engine for ToolSelector {
    fn descriptor(&self) -> EngineDescriptor {
        descriptor(
            "tool-selector",
            "Conservatively narrows optional tools using native lexical and schema relevance",
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
        if !request.policy.enable_tool_selection
            || !request.host_capabilities.can_narrow_tools
            || !request.host_capabilities.can_retry_with_full_tools
            || current.tools.len() <= 3
        {
            return Ok(None);
        }

        let query = latest_user_text(current);
        let query_terms = terms(&query);
        if query_terms.is_empty() {
            return Ok(None);
        }

        let mut scores = Vec::new();
        for tool in &current.tools {
            scores.push((score(tool, &query_terms), tool.name.clone()));
        }
        let best = scores.iter().map(|(score, _)| *score).max().unwrap_or(0);
        if best == 0 {
            return Ok(None);
        }

        let mut keep: BTreeSet<String> = current
            .tools
            .iter()
            .filter(|tool| tool.required || tool.tags.iter().any(|tag| tag == "always"))
            .map(|tool| tool.name.clone())
            .collect();
        for (tool_score, name) in &scores {
            if *tool_score >= 2 || *tool_score == best {
                keep.insert(name.clone());
            }
        }

        for (_, name) in scores.iter().filter(|(score, _)| *score == best).take(3) {
            keep.insert(name.clone());
        }
        expand_dependencies(&current.tools, &mut keep);

        if keep.is_empty() || keep.len() == current.tools.len() {
            return Ok(None);
        }

        let mut next = current.clone();
        next.tools.retain(|tool| keep.contains(&tool.name));
        let removed = current.tools.len() - next.tools.len();
        let mut candidate = Candidate::new(
            next,
            format!(
                "selected {} of {} tools; {} optional tool(s) can be restored on retry",
                keep.len(),
                current.tools.len(),
                removed
            ),
        );
        candidate.touches_tools = true;
        candidate.reconstruction = Some(current.clone());
        Ok(Some(candidate))
    }
}

fn latest_user_text(content: &ContentEnvelope) -> String {
    content
        .messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            message
                .parts
                .iter()
                .filter_map(|part| match part {
                    ContentPart::Text { text } | ContentPart::Document { text, .. } => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn score(tool: &ToolDefinition, query: &BTreeSet<String>) -> u32 {
    let name_terms = terms(&tool.name.replace(&['_', '-'][..], " "));
    let description_terms = terms(&tool.description);
    let schema_terms = terms(&tool.input_schema.to_string());
    let tag_terms = terms(&tool.tags.join(" "));

    query
        .iter()
        .map(|term| {
            let mut value = 0;
            if name_terms.contains(term) {
                value += 6;
            }
            if description_terms.contains(term) {
                value += 2;
            }
            if schema_terms.contains(term) {
                value += 1;
            }
            if tag_terms.contains(term) {
                value += 3;
            }
            value
        })
        .sum()
}

fn terms(value: &str) -> BTreeSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::to_lowercase)
        .filter(|term| term.len() >= 3)
        .filter(|term| {
            !matches!(
                term.as_str(),
                "the" | "and" | "for" | "with" | "this" | "that" | "from" | "into"
            )
        })
        .collect()
}

fn expand_dependencies(tools: &[ToolDefinition], keep: &mut BTreeSet<String>) {
    let tags: BTreeMap<&str, &[String]> = tools
        .iter()
        .map(|tool| (tool.name.as_str(), tool.tags.as_slice()))
        .collect();
    let mut changed = true;
    while changed {
        changed = false;
        let names: Vec<String> = keep.iter().cloned().collect();
        for name in names {
            let Some(tool_tags) = tags.get(name.as_str()) else {
                continue;
            };
            for dependency in tool_tags
                .iter()
                .filter_map(|tag| tag.strip_prefix("depends:"))
            {
                changed |= keep.insert(dependency.to_owned());
            }
        }
    }
}
