//! KendrOptimizer's network-free transformation core.
//!
//! The core accepts a typed envelope and returns an optimized envelope plus an
//! auditable receipt. It never holds provider credentials, chooses a model, or
//! forwards a request.

mod engine;
mod engines;
mod generation;
mod optimizer;
mod protected;
mod tokenizer;
mod validation;

pub use kendr_optimizer_contracts as contracts;
pub use optimizer::Optimizer;

pub use engine::OptimizeError;
