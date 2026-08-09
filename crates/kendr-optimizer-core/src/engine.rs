use kendr_optimizer_contracts::{ContentEnvelope, EngineDescriptor, OptimizeRequest, RiskLevel};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OptimizeError {
    #[error("unsupported schema version: {0}")]
    UnsupportedSchema(String),
    #[error("invalid envelope: {0}")]
    InvalidEnvelope(String),
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("tokenizer failed: {0}")]
    Tokenizer(String),
    #[error("recovery capsule is invalid: {0}")]
    InvalidRecovery(String),
}

pub(crate) trait Engine: Send + Sync {
    fn descriptor(&self) -> EngineDescriptor;

    fn propose(
        &self,
        request: &OptimizeRequest,
        current: &ContentEnvelope,
    ) -> Result<Option<Candidate>, OptimizeError>;
}

#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub content: ContentEnvelope,
    pub explanation: String,
    pub touched_message_ids: Vec<String>,
    pub touches_tools: bool,
    pub reconstruction: Option<ContentEnvelope>,
}

impl Candidate {
    pub fn new(content: ContentEnvelope, explanation: impl Into<String>) -> Self {
        Self {
            content,
            explanation: explanation.into(),
            touched_message_ids: Vec::new(),
            touches_tools: false,
            reconstruction: None,
        }
    }
}

pub(crate) fn descriptor(
    id: &str,
    summary: &str,
    risk: RiskLevel,
    reversible: bool,
    cache_safe: bool,
) -> EngineDescriptor {
    EngineDescriptor {
        id: id.to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        summary: summary.to_owned(),
        risk,
        reversible,
        cache_safe,
    }
}
