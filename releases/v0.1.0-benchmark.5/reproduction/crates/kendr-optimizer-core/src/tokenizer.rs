use kendr_optimizer_contracts::{
    ContentEnvelope, MeasurementConfidence, TokenMeasurement, TokenizerProfile,
};
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};
use tiktoken_rs::CoreBPE;

use crate::engine::OptimizeError;

static CL100K_BASE: Lazy<Result<CoreBPE, String>> =
    Lazy::new(|| tiktoken_rs::cl100k_base().map_err(|error| error.to_string()));
static O200K_BASE: Lazy<Result<CoreBPE, String>> =
    Lazy::new(|| tiktoken_rs::o200k_base().map_err(|error| error.to_string()));

pub(crate) fn measure(
    content: &ContentEnvelope,
    profile: TokenizerProfile,
) -> Result<TokenMeasurement, OptimizeError> {
    let serialized = serde_json::to_vec(content)?;
    let text = std::str::from_utf8(&serialized)
        .map_err(|error| OptimizeError::Tokenizer(error.to_string()))?;

    let (tokens, tokenizer, confidence) = match profile {
        TokenizerProfile::Cl100kBase => {
            let bpe = CL100K_BASE
                .as_ref()
                .map_err(|error| OptimizeError::Tokenizer(error.clone()))?;
            (
                bpe.encode_with_special_tokens(text).len() as u64,
                "cl100k_base/serde_json".to_owned(),
                MeasurementConfidence::ExactTokenizer,
            )
        }
        TokenizerProfile::O200kBase => {
            let bpe = O200K_BASE
                .as_ref()
                .map_err(|error| OptimizeError::Tokenizer(error.clone()))?;
            (
                bpe.encode_with_special_tokens(text).len() as u64,
                "o200k_base/serde_json".to_owned(),
                MeasurementConfidence::ExactTokenizer,
            )
        }
        TokenizerProfile::Approximate => {
            let character_estimate = text.chars().count().div_ceil(3) as u64;
            let lexical_floor = text.split_whitespace().count() as u64;
            (
                character_estimate.max(lexical_floor),
                "conservative_chars_per_3/serde_json".to_owned(),
                MeasurementConfidence::ConservativeEstimate,
            )
        }
    };

    Ok(TokenMeasurement {
        tokens,
        bytes: serialized.len() as u64,
        tokenizer,
        confidence,
        serialized_sha256: sha256_hex(&serialized),
    })
}

pub(crate) fn count_text(text: &str, profile: TokenizerProfile) -> Result<u64, OptimizeError> {
    match profile {
        TokenizerProfile::Cl100kBase => {
            let bpe = CL100K_BASE
                .as_ref()
                .map_err(|error| OptimizeError::Tokenizer(error.clone()))?;
            Ok(bpe.encode_with_special_tokens(text).len() as u64)
        }
        TokenizerProfile::O200kBase => {
            let bpe = O200K_BASE
                .as_ref()
                .map_err(|error| OptimizeError::Tokenizer(error.clone()))?;
            Ok(bpe.encode_with_special_tokens(text).len() as u64)
        }
        TokenizerProfile::Approximate => {
            Ok((text.chars().count().div_ceil(3) as u64)
                .max(text.split_whitespace().count() as u64))
        }
    }
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
