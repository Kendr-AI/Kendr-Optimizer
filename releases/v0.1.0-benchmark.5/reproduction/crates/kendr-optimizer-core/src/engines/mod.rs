mod context_repetition;
mod history_dedup;
mod json_minify;
mod pytest_reduce;
mod repeat_lines;
mod terminal_clean;
mod text_normalize;
mod tool_output;
mod tool_selector;

use crate::engine::Engine;

pub(crate) fn native_engines() -> Vec<Box<dyn Engine>> {
    vec![
        Box::new(JsonMinify),
        Box::new(TerminalClean),
        Box::new(TextNormalize),
        Box::new(RepeatLines),
        Box::new(PytestReduce),
        Box::new(ContextRepetition),
        Box::new(HistoryDedup),
        Box::new(ToolOutput),
        Box::new(ToolSelector),
    ]
}

pub(crate) use context_repetition::{ContextRepetition, expand_context_repetitions};
pub(crate) use history_dedup::HistoryDedup;
pub(crate) use json_minify::JsonMinify;
pub(crate) use pytest_reduce::{PytestReduce, expand_pytest_folds};
pub(crate) use repeat_lines::{RepeatLines, expand_repeated_lines};
pub(crate) use terminal_clean::TerminalClean;
pub(crate) use text_normalize::TextNormalize;
pub(crate) use tool_output::ToolOutput;
pub(crate) use tool_selector::ToolSelector;
