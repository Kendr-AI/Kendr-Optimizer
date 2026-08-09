use std::collections::BTreeSet;
use std::time::Instant;

use kendr_optimizer_contracts::{
    AttemptStatus, CacheImpact, ContentEnvelope, EngineAttempt, EngineDescriptor, MoneyEstimate,
    OptimizationReceipt, OptimizeOutcome, OptimizePhase, OptimizeRequest, OutcomeStatus,
    RecoveryCapsule, RecoveryRecord, RiskLevel, SCHEMA_VERSION, UsageObservation,
};

use crate::engine::{Candidate, Engine, OptimizeError};
use crate::engines::native_engines;
use crate::generation::recommend;
use crate::tokenizer::measure;
use crate::validation::{validate_envelope, verify_candidate};

pub struct Optimizer {
    engines: Vec<Box<dyn Engine>>,
}

impl Default for Optimizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Optimizer {
    pub fn new() -> Self {
        Self {
            engines: native_engines(),
        }
    }

    pub fn engines(&self) -> Vec<EngineDescriptor> {
        self.engines
            .iter()
            .map(|engine| engine.descriptor())
            .collect()
    }

    pub fn analyze(&self, request: &OptimizeRequest) -> Result<OptimizeOutcome, OptimizeError> {
        let mut shadow = request.clone();
        shadow.policy.shadow = true;
        self.optimize(&shadow)
    }

    pub fn optimize(&self, request: &OptimizeRequest) -> Result<OptimizeOutcome, OptimizeError> {
        if request.schema_version != SCHEMA_VERSION {
            return Err(OptimizeError::UnsupportedSchema(
                request.schema_version.clone(),
            ));
        }
        validate_envelope(&request.content)?;

        let receipt_started = Instant::now();
        let original_content = request.content.clone();
        let baseline = measure(&original_content, request.target.tokenizer_profile)?;
        let planning_started = Instant::now();
        let generation_recommendation = recommend(request)?;
        let request_id = if request.request_id.trim().is_empty() {
            format!("anonymous-{}", &baseline.serialized_sha256[..12])
        } else {
            request.request_id.clone()
        };

        if request.phase == OptimizePhase::OutputObservation {
            return Ok(no_op_outcome(
                request,
                request_id,
                baseline,
                receipt_started,
                "Post-generation output has already been billed. Record it with observe instead of rewriting it.",
            ));
        }

        let mut working = original_content.clone();
        let mut attempts = Vec::new();
        let mut accepted = 0usize;
        let mut needs_recovery = false;
        let mut cache_invalidated = false;
        let mut warnings = base_warnings(request);
        if generation_recommendation
            .as_ref()
            .is_some_and(|recommendation| {
                recommendation.action != kendr_optimizer_contracts::GenerationAction::NoChange
            })
        {
            warnings.push(
                "Generation-policy savings are predicted, not verified; use paired observations."
                    .to_owned(),
            );
        }

        if !request.policy.enabled_engines.is_empty() {
            let known: BTreeSet<String> = self
                .engines
                .iter()
                .map(|engine| engine.descriptor().id)
                .collect();
            for requested in &request.policy.enabled_engines {
                if !known.contains(requested) {
                    warnings.push(format!("unknown engine requested: {requested}"));
                }
            }
        }

        for engine in &self.engines {
            let descriptor = engine.descriptor();
            if !request.policy.enabled_engines.is_empty()
                && !request.policy.enabled_engines.contains(&descriptor.id)
            {
                continue;
            }

            if planning_started.elapsed().as_millis() as u64 >= request.policy.latency_budget_ms {
                attempts.push(empty_attempt(
                    &descriptor,
                    AttemptStatus::TimedOut,
                    "global latency budget exhausted before this engine",
                ));
                continue;
            }

            if descriptor.risk > request.policy.risk_ceiling {
                attempts.push(empty_attempt(
                    &descriptor,
                    AttemptStatus::Rejected,
                    "engine risk exceeds the configured risk ceiling",
                ));
                continue;
            }

            let engine_started = Instant::now();
            let candidate = match engine.propose(request, &working) {
                Ok(Some(candidate)) => candidate,
                Ok(None) => {
                    attempts.push(EngineAttempt {
                        engine_id: descriptor.id,
                        engine_version: descriptor.version,
                        status: AttemptStatus::NoCandidate,
                        risk: descriptor.risk,
                        reversible: descriptor.reversible,
                        token_delta: 0,
                        byte_delta: 0,
                        elapsed_micros: elapsed_micros(engine_started),
                        reason: "content or policy did not produce a candidate".to_owned(),
                        verification: Vec::new(),
                    });
                    continue;
                }
                Err(error) => {
                    attempts.push(EngineAttempt {
                        engine_id: descriptor.id,
                        engine_version: descriptor.version,
                        status: AttemptStatus::Reverted,
                        risk: descriptor.risk,
                        reversible: descriptor.reversible,
                        token_delta: 0,
                        byte_delta: 0,
                        elapsed_micros: elapsed_micros(engine_started),
                        reason: format!("engine failed open: {error}"),
                        verification: Vec::new(),
                    });
                    continue;
                }
            };

            // Candidate generation is deliberately cheap to reject. Avoid a
            // full tokenizer pass for engines that do not apply to this
            // content surface; it consumed the shared latency budget and could
            // prevent a later applicable engine from running.
            let before = measure(&working, request.target.tokenizer_profile)?;
            let after = measure(&candidate.content, request.target.tokenizer_profile)?;
            let token_delta = before.tokens as i64 - after.tokens as i64;
            let byte_delta = before.bytes as i64 - after.bytes as i64;
            let gain_percent = percent(token_delta, before.tokens);
            let mut verification = verify_candidate(&descriptor.id, &working, &candidate);

            let cache_touched = protected_cache_touched(request, &candidate);
            verification.push(kendr_optimizer_contracts::VerificationCheck {
                name: "cache_prefix".to_owned(),
                passed: !request.policy.preserve_cache_prefix || !cache_touched,
                detail: Some(if cache_touched {
                    "candidate touches a declared cache segment or the tool prefix".to_owned()
                } else {
                    "declared cache segments remain unchanged".to_owned()
                }),
            });

            let net_positive = token_delta >= request.policy.min_gain_tokens
                && gain_percent >= request.policy.min_gain_percent;
            verification.push(kendr_optimizer_contracts::VerificationCheck {
                name: "net_positive".to_owned(),
                passed: net_positive,
                detail: Some(format!(
                    "signed delta={token_delta} tokens, gain={gain_percent:.2}%"
                )),
            });

            let valid = verification.iter().all(|check| check.passed);
            if valid {
                needs_recovery |=
                    candidate.reconstruction.is_some() || descriptor.risk >= RiskLevel::Recoverable;
                cache_invalidated |= cache_touched && !request.policy.preserve_cache_prefix;
                working = candidate.content;
                accepted += 1;
            }

            attempts.push(EngineAttempt {
                engine_id: descriptor.id,
                engine_version: descriptor.version,
                status: if valid {
                    if request.policy.shadow {
                        AttemptStatus::Shadow
                    } else {
                        AttemptStatus::Applied
                    }
                } else if verification
                    .iter()
                    .any(|check| check.name != "net_positive" && !check.passed)
                {
                    AttemptStatus::Reverted
                } else {
                    AttemptStatus::Rejected
                },
                risk: descriptor.risk,
                reversible: descriptor.reversible,
                token_delta,
                byte_delta,
                elapsed_micros: elapsed_micros(engine_started),
                reason: if valid {
                    candidate.explanation
                } else {
                    failed_reason(&verification)
                },
                verification,
            });
        }

        let hypothetical = measure(&working, request.target.tokenizer_profile)?;
        let hypothetical_delta = baseline.tokens as i64 - hypothetical.tokens as i64;
        let hypothetical_percent = percent(hypothetical_delta, baseline.tokens);
        let portfolio_positive = hypothetical_delta >= request.policy.min_gain_tokens
            && hypothetical_percent >= request.policy.min_gain_percent;

        let (status, content, optimized, no_op_reason) = if accepted == 0 {
            let generation_only =
                generation_recommendation
                    .as_ref()
                    .is_some_and(|recommendation| {
                        recommendation.action
                            != kendr_optimizer_contracts::GenerationAction::NoChange
                    });
            (
                OutcomeStatus::Skipped,
                original_content.clone(),
                baseline.clone(),
                Some(if generation_only {
                    "No input transform passed the gates. An output generation recommendation was returned separately and remains unverified until observation."
                        .to_owned()
                } else {
                    "No engine produced a candidate that passed policy, safety, cache, and net-gain gates."
                        .to_owned()
                }),
            )
        } else if request.policy.shadow {
            warnings.push(
                "Shadow mode returned the original content; optimized measurement is hypothetical."
                    .to_owned(),
            );
            (
                OutcomeStatus::Shadow,
                original_content.clone(),
                hypothetical,
                None,
            )
        } else if !portfolio_positive {
            warnings.push(format!(
                "The candidate portfolio was reverted because its whole-envelope delta was only {hypothetical_delta} tokens ({hypothetical_percent:.2}%)."
            ));
            (
                OutcomeStatus::Reverted,
                original_content.clone(),
                baseline.clone(),
                Some("Whole-envelope net gain was below the configured threshold.".to_owned()),
            )
        } else {
            (OutcomeStatus::Applied, working, hypothetical, None)
        };

        let token_delta = baseline.tokens as i64 - optimized.tokens as i64;
        let byte_delta = baseline.bytes as i64 - optimized.bytes as i64;
        let estimated_input_reduction_percent = percent(token_delta, baseline.tokens);
        let estimated_input_cost_reduction =
            request
                .target
                .pricing
                .as_ref()
                .map(|pricing| {
                    MoneyEstimate {
                amount: round_money(
                    token_delta as f64 * pricing.input_per_million / 1_000_000.0,
                ),
                currency: pricing.currency.clone(),
                basis:
                    "local serialized-input estimate; not provider billing and not verified savings"
                        .to_owned(),
            }
                });

        let recovery = if status == OutcomeStatus::Applied && needs_recovery {
            Some(RecoveryCapsule {
                request_id: request_id.clone(),
                original_sha256: baseline.serialized_sha256.clone(),
                records: vec![RecoveryRecord {
                    engine_id: "portfolio".to_owned(),
                    scope: "envelope".to_owned(),
                    marker: optimized.serialized_sha256.clone(),
                    original: serde_json::to_value(&original_content)?,
                }],
            })
        } else {
            None
        };

        Ok(OptimizeOutcome {
            content,
            receipt: OptimizationReceipt {
                schema_version: kendr_optimizer_contracts::RECEIPT_VERSION.to_owned(),
                request_id,
                status,
                original: baseline,
                optimized,
                token_delta,
                byte_delta,
                estimated_input_reduction_percent,
                estimated_input_cost_reduction,
                verified_savings: false,
                cache_impact: if cache_invalidated {
                    CacheImpact::Invalidated
                } else if !request.target.cache_segments.is_empty() && accepted > 0 {
                    CacheImpact::PrefixPreserved
                } else {
                    CacheImpact::None
                },
                total_latency_micros: elapsed_micros(receipt_started),
                attempts,
                warnings,
                no_op_reason,
            },
            generation_recommendation,
            recovery,
        })
    }

    pub fn restore(&self, capsule: &RecoveryCapsule) -> Result<ContentEnvelope, OptimizeError> {
        let record = capsule
            .records
            .iter()
            .find(|record| record.scope == "envelope")
            .ok_or_else(|| {
                OptimizeError::InvalidRecovery(
                    "no complete envelope record exists in the capsule".to_owned(),
                )
            })?;
        let restored: ContentEnvelope = serde_json::from_value(record.original.clone())?;
        validate_envelope(&restored)?;
        let measured = measure(
            &restored,
            kendr_optimizer_contracts::TokenizerProfile::Approximate,
        )?;
        if measured.serialized_sha256 != capsule.original_sha256 {
            return Err(OptimizeError::InvalidRecovery(
                "restored envelope digest does not match the capsule".to_owned(),
            ));
        }
        Ok(restored)
    }

    pub fn observe(
        &self,
        observation: UsageObservation,
    ) -> kendr_optimizer_contracts::ObservedSavings {
        observation.compare()
    }
}

fn no_op_outcome(
    request: &OptimizeRequest,
    request_id: String,
    baseline: kendr_optimizer_contracts::TokenMeasurement,
    started: Instant,
    reason: &str,
) -> OptimizeOutcome {
    OptimizeOutcome {
        content: request.content.clone(),
        receipt: OptimizationReceipt {
            schema_version: kendr_optimizer_contracts::RECEIPT_VERSION.to_owned(),
            request_id,
            status: OutcomeStatus::Skipped,
            original: baseline.clone(),
            optimized: baseline,
            token_delta: 0,
            byte_delta: 0,
            estimated_input_reduction_percent: 0.0,
            estimated_input_cost_reduction: None,
            verified_savings: false,
            cache_impact: CacheImpact::None,
            total_latency_micros: elapsed_micros(started),
            attempts: Vec::new(),
            warnings: vec![reason.to_owned()],
            no_op_reason: Some(reason.to_owned()),
        },
        generation_recommendation: None,
        recovery: None,
    }
}

fn empty_attempt(
    descriptor: &EngineDescriptor,
    status: AttemptStatus,
    reason: &str,
) -> EngineAttempt {
    EngineAttempt {
        engine_id: descriptor.id.clone(),
        engine_version: descriptor.version.clone(),
        status,
        risk: descriptor.risk,
        reversible: descriptor.reversible,
        token_delta: 0,
        byte_delta: 0,
        elapsed_micros: 0,
        reason: reason.to_owned(),
        verification: Vec::new(),
    }
}

fn protected_cache_touched(request: &OptimizeRequest, candidate: &Candidate) -> bool {
    if request.target.cache_segments.is_empty() {
        return false;
    }
    if candidate.touches_tools {
        return true;
    }
    let protected: BTreeSet<&str> = request
        .target
        .cache_segments
        .iter()
        .flat_map(|segment| segment.message_ids.iter().map(String::as_str))
        .collect();
    candidate
        .touched_message_ids
        .iter()
        .any(|id| protected.contains(id.as_str()))
}

fn base_warnings(request: &OptimizeRequest) -> Vec<String> {
    let mut warnings = vec![
        "Preflight savings are local estimates. Call observe with a paired provider baseline to verify savings."
            .to_owned(),
    ];
    if request.target.tokenizer_profile == kendr_optimizer_contracts::TokenizerProfile::Approximate
    {
        warnings.push(
            "No tokenizer profile was supplied; token counts use a conservative character estimate."
                .to_owned(),
        );
    }
    if request.target.model.is_none() {
        warnings.push(
            "No model metadata was supplied; provider framing and cache billing are not modeled."
                .to_owned(),
        );
    }
    warnings
}

fn failed_reason(checks: &[kendr_optimizer_contracts::VerificationCheck]) -> String {
    let failed: Vec<&str> = checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| check.name.as_str())
        .collect();
    format!("candidate rejected by: {}", failed.join(", "))
}

fn percent(delta: i64, original: u64) -> f64 {
    if original == 0 {
        0.0
    } else {
        delta as f64 * 100.0 / original as f64
    }
}

fn elapsed_micros(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn round_money(amount: f64) -> f64 {
    (amount * 1_000_000_000_000.0).round() / 1_000_000_000_000.0
}
