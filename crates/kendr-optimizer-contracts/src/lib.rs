//! Language-neutral contracts shared by the optimizer core and host adapters.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SCHEMA_VERSION: &str = "kendr.optimize/v1";
pub const RECEIPT_VERSION: &str = "kendr.receipt/v1";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OptimizePhase {
    Request,
    ToolResult,
    HistoryIngest,
    OutputObservation,
}

impl Default for OptimizePhase {
    fn default() -> Self {
        Self::Request
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    PassThrough,
    RepresentationSafe,
    Recoverable,
    Extractive,
    Learned,
}

impl Default for RiskLevel {
    fn default() -> Self {
        Self::Recoverable
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TokenizerProfile {
    Approximate,
    Cl100kBase,
    O200kBase,
}

impl Default for TokenizerProfile {
    fn default() -> Self {
        Self::Approximate
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MeasurementConfidence {
    ExactTokenizer,
    ConservativeEstimate,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OptimizeRequest {
    pub schema_version: String,
    pub phase: OptimizePhase,
    #[serde(default)]
    pub request_id: String,
    #[serde(default)]
    pub session_id: Option<String>,
    pub content: ContentEnvelope,
    #[serde(default)]
    pub target: TargetContext,
    #[serde(default)]
    pub generation: GenerationContext,
    #[serde(default)]
    pub host_capabilities: HostCapabilities,
    #[serde(default)]
    pub policy: OptimizationPolicy,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ContentEnvelope {
    #[serde(default)]
    pub messages: Vec<Message>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    #[serde(default)]
    pub output_contract: Option<Value>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Message {
    pub id: String,
    pub role: MessageRole,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub turn_id: Option<String>,
    pub parts: Vec<ContentPart>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Code {
        #[serde(default)]
        language: Option<String>,
        text: String,
    },
    Json {
        value: Value,
    },
    Document {
        #[serde(default)]
        media_type: Option<String>,
        text: String,
    },
    ImageReference {
        uri: String,
        #[serde(default)]
        alt: Option<String>,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: Value,
    },
    ToolResult {
        call_id: String,
        #[serde(default)]
        name: Option<String>,
        content: String,
        #[serde(default)]
        is_error: bool,
    },
}

impl ContentPart {
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Text { text }
            | Self::Code { text, .. }
            | Self::Document { text, .. }
            | Self::ToolResult { content: text, .. } => Some(text),
            Self::Json { .. } | Self::ImageReference { .. } | Self::ToolCall { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "empty_object")]
    pub input_schema: Value,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

fn empty_object() -> Value {
    Value::Object(Default::default())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TargetContext {
    #[serde(default)]
    pub tokenizer_profile: TokenizerProfile,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub context_limit: Option<u64>,
    #[serde(default)]
    pub pricing: Option<Pricing>,
    #[serde(default)]
    pub cache_segments: Vec<CacheSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pricing {
    pub input_per_million: f64,
    #[serde(default)]
    pub cached_input_per_million: Option<f64>,
    #[serde(default)]
    pub output_per_million: Option<f64>,
    #[serde(default = "default_currency")]
    pub currency: String,
}

fn default_currency() -> String {
    "USD".to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheSegment {
    pub id: String,
    #[serde(default)]
    pub message_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HostCapabilities {
    #[serde(default)]
    pub can_narrow_tools: bool,
    #[serde(default)]
    pub can_restore_references: bool,
    #[serde(default)]
    pub can_retry_with_full_tools: bool,
    #[serde(default)]
    pub streaming_output: bool,
    #[serde(default)]
    pub can_set_max_output_tokens: bool,
    #[serde(default)]
    pub can_set_verbosity: bool,
    #[serde(default)]
    pub can_append_generation_policy: bool,
}

impl Default for HostCapabilities {
    fn default() -> Self {
        Self {
            can_narrow_tools: false,
            can_restore_references: false,
            can_retry_with_full_tools: false,
            streaming_output: true,
            can_set_max_output_tokens: false,
            can_set_verbosity: false,
            can_append_generation_policy: false,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GenerationContext {
    #[serde(default)]
    pub current_max_output_tokens: Option<u64>,
    #[serde(default)]
    pub target_output_tokens: Option<u64>,
    #[serde(default)]
    pub expected_output_tokens: Option<u64>,
    #[serde(default)]
    pub requested_verbosity: VerbosityIntent,
    #[serde(default)]
    pub required_elements: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerbosityIntent {
    #[default]
    Auto,
    Concise,
    Standard,
    Detailed,
    Exact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptimizationPolicy {
    #[serde(default)]
    pub risk_ceiling: RiskLevel,
    #[serde(default = "default_min_gain_tokens")]
    pub min_gain_tokens: i64,
    #[serde(default = "default_min_gain_percent")]
    pub min_gain_percent: f64,
    #[serde(default = "default_latency_budget_ms")]
    pub latency_budget_ms: u64,
    #[serde(default = "default_true")]
    pub preserve_cache_prefix: bool,
    #[serde(default)]
    pub shadow: bool,
    #[serde(default = "default_recent_messages")]
    pub preserve_recent_messages: usize,
    #[serde(default = "default_tool_result_chars")]
    pub max_tool_result_chars: usize,
    #[serde(default)]
    pub enable_tool_selection: bool,
    #[serde(default)]
    pub enable_lossy_tool_output: bool,
    #[serde(default)]
    pub enable_generation_policy: bool,
    #[serde(default = "default_min_output_saving")]
    pub min_expected_output_saving_tokens: u64,
    #[serde(default)]
    pub enabled_engines: Vec<String>,
}

impl Default for OptimizationPolicy {
    fn default() -> Self {
        Self {
            risk_ceiling: RiskLevel::Recoverable,
            min_gain_tokens: default_min_gain_tokens(),
            min_gain_percent: default_min_gain_percent(),
            latency_budget_ms: default_latency_budget_ms(),
            preserve_cache_prefix: true,
            shadow: false,
            preserve_recent_messages: default_recent_messages(),
            max_tool_result_chars: default_tool_result_chars(),
            enable_tool_selection: false,
            enable_lossy_tool_output: false,
            enable_generation_policy: false,
            min_expected_output_saving_tokens: default_min_output_saving(),
            enabled_engines: Vec::new(),
        }
    }
}

fn default_min_gain_tokens() -> i64 {
    8
}

fn default_min_gain_percent() -> f64 {
    1.0
}

fn default_latency_budget_ms() -> u64 {
    25
}

fn default_recent_messages() -> usize {
    6
}

fn default_tool_result_chars() -> usize {
    24_000
}

fn default_min_output_saving() -> u64 {
    32
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EngineDescriptor {
    pub id: String,
    pub version: String,
    pub summary: String,
    pub risk: RiskLevel,
    pub reversible: bool,
    pub cache_safe: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    Applied,
    Skipped,
    Shadow,
    Reverted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Applied,
    NoCandidate,
    Rejected,
    Shadow,
    Reverted,
    TimedOut,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TokenMeasurement {
    pub tokens: u64,
    pub bytes: u64,
    pub tokenizer: String,
    pub confidence: MeasurementConfidence,
    pub serialized_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VerificationCheck {
    pub name: String,
    pub passed: bool,
    #[serde(default)]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineAttempt {
    pub engine_id: String,
    pub engine_version: String,
    pub status: AttemptStatus,
    pub risk: RiskLevel,
    pub reversible: bool,
    pub token_delta: i64,
    pub byte_delta: i64,
    pub elapsed_micros: u64,
    pub reason: String,
    #[serde(default)]
    pub verification: Vec<VerificationCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptimizationReceipt {
    #[serde(default = "receipt_version")]
    pub schema_version: String,
    pub request_id: String,
    pub status: OutcomeStatus,
    pub original: TokenMeasurement,
    pub optimized: TokenMeasurement,
    pub token_delta: i64,
    pub byte_delta: i64,
    pub estimated_input_reduction_percent: f64,
    #[serde(default)]
    pub estimated_input_cost_reduction: Option<MoneyEstimate>,
    pub verified_savings: bool,
    pub cache_impact: CacheImpact,
    pub total_latency_micros: u64,
    #[serde(default)]
    pub attempts: Vec<EngineAttempt>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub no_op_reason: Option<String>,
}

fn receipt_version() -> String {
    RECEIPT_VERSION.to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MoneyEstimate {
    pub amount: f64,
    pub currency: String,
    pub basis: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CacheImpact {
    None,
    PrefixPreserved,
    Unknown,
    Invalidated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryCapsule {
    pub request_id: String,
    pub original_sha256: String,
    #[serde(default)]
    pub records: Vec<RecoveryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RecoveryRecord {
    pub engine_id: String,
    pub scope: String,
    pub marker: String,
    pub original: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OptimizeOutcome {
    pub content: ContentEnvelope,
    pub receipt: OptimizationReceipt,
    #[serde(default)]
    pub generation_recommendation: Option<GenerationRecommendation>,
    #[serde(default)]
    pub recovery: Option<RecoveryCapsule>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GenerationAction {
    NoChange,
    SetHostControls,
    AppendInstruction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenerationRecommendation {
    pub action: GenerationAction,
    pub risk: RiskLevel,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    #[serde(default)]
    pub verbosity: Option<VerbosityIntent>,
    #[serde(default)]
    pub instruction: Option<String>,
    pub estimated_added_input_tokens: u64,
    #[serde(default)]
    pub estimated_output_reduction_tokens: Option<u64>,
    pub expected_net_token_reduction: i64,
    pub verified_savings: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageObservation {
    pub request_id: String,
    pub optimized: ProviderUsage,
    #[serde(default)]
    pub paired_baseline: Option<ProviderUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cached_input_tokens: u64,
    #[serde(default)]
    pub total_cost: Option<MoneyEstimate>,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub task_success: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservedSavings {
    pub request_id: String,
    pub verified: bool,
    pub paired_baseline_supplied: bool,
    #[serde(default)]
    pub quality_preserved: Option<bool>,
    #[serde(default)]
    pub input_token_delta: Option<i64>,
    #[serde(default)]
    pub output_token_delta: Option<i64>,
    #[serde(default)]
    pub total_cost_delta: Option<MoneyEstimate>,
    pub explanation: String,
}

impl UsageObservation {
    pub fn compare(self) -> ObservedSavings {
        let Some(baseline) = self.paired_baseline else {
            return ObservedSavings {
                request_id: self.request_id,
                verified: false,
                paired_baseline_supplied: false,
                quality_preserved: None,
                input_token_delta: None,
                output_token_delta: None,
                total_cost_delta: None,
                explanation:
                    "No paired baseline was supplied; provider usage is observed but savings are unverified."
                        .to_owned(),
            };
        };

        let input_token_delta =
            signed_u64_delta(baseline.input_tokens, self.optimized.input_tokens);
        let output_token_delta =
            signed_u64_delta(baseline.output_tokens, self.optimized.output_tokens);
        let total_cost_delta = match (baseline.total_cost, self.optimized.total_cost) {
            (Some(before), Some(after))
                if before.currency == after.currency
                    && before.amount.is_finite()
                    && after.amount.is_finite()
                    && before.amount >= 0.0
                    && after.amount >= 0.0 =>
            {
                Some(MoneyEstimate {
                    amount: round_money(before.amount - after.amount),
                    currency: before.currency,
                    basis: "paired provider-reported total cost".to_owned(),
                })
            }
            _ => None,
        };
        let quality_preserved = match (baseline.task_success, self.optimized.task_success) {
            (Some(_), Some(after)) => Some(after),
            _ => None,
        };
        let positive_net_saving = total_cost_delta.as_ref().map_or_else(
            || i128::from(input_token_delta) + i128::from(output_token_delta) > 0,
            |delta| delta.amount.is_finite() && delta.amount > 0.0,
        );
        let verified = quality_preserved == Some(true) && positive_net_saving;

        ObservedSavings {
            request_id: self.request_id,
            verified,
            paired_baseline_supplied: true,
            quality_preserved,
            input_token_delta: Some(input_token_delta),
            output_token_delta: Some(output_token_delta),
            total_cost_delta,
            explanation: if verified {
                "The supplied pair shows a positive net provider-usage or cost reduction and the optimized task succeeded."
                    .to_owned()
            } else if quality_preserved == Some(false) {
                "Paired usage was supplied, but the optimized task did not succeed; this is not a quality-preserving verified saving."
                    .to_owned()
            } else if quality_preserved.is_none() {
                "Paired usage was supplied, but no comparable task-success signal was supplied; the usage delta is not quality-verified."
                    .to_owned()
            } else {
                "Paired usage and a successful optimized task were supplied, but there was no positive net provider-usage or cost reduction."
                    .to_owned()
            },
        }
    }
}

fn signed_u64_delta(before: u64, after: u64) -> i64 {
    if before >= after {
        i64::try_from(before - after).unwrap_or(i64::MAX)
    } else {
        -i64::try_from(after - before).unwrap_or(i64::MAX)
    }
}

fn round_money(amount: f64) -> f64 {
    (amount * 1_000_000_000_000.0).round() / 1_000_000_000_000.0
}
