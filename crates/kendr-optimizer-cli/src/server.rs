use std::net::SocketAddr;
use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Router, serve};
use kendr_optimizer_contracts::{OptimizeRequest, RecoveryCapsule, UsageObservation};
use kendr_optimizer_core::{OptimizeError, Optimizer};
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::EnvFilter;

pub(crate) async fn run(
    bind: SocketAddr,
    optimizer: Optimizer,
) -> Result<(), Box<dyn std::error::Error>> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    let app = Router::new()
        .route("/healthz", get(health))
        .route("/v1/capabilities", get(capabilities))
        .route("/v1/engines", get(engines))
        .route("/v1/analyze", post(analyze))
        .route("/v1/optimize", post(optimize))
        .route("/v1/restore", post(restore))
        .route("/v1/observe", post(observe))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(optimizer));

    let listener = TcpListener::bind(bind).await?;
    info!(
        address = %listener.local_addr()?,
        "KendrOptimizer transform-only service listening; no provider egress is implemented"
    );
    serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn health() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "kendr-optimizer",
        "boundary": "transform_only",
        "provider_egress": false
    }))
}

async fn capabilities(State(optimizer): State<Arc<Optimizer>>) -> Json<Value> {
    Json(json!({
        "service": "kendr-optimizer",
        "contract_versions": ["kendr.optimize/v1", "kendr.receipt/v1"],
        "operations": ["analyze", "optimize", "restore", "observe"],
        "tokenizers": ["approximate", "cl100k_base", "o200k_base"],
        "engines": optimizer.engines(),
        "generation_recommendations": true,
        "provider_egress": false,
        "stores_provider_credentials": false,
        "inference_gateway": false
    }))
}

async fn engines(State(optimizer): State<Arc<Optimizer>>) -> Json<Value> {
    Json(json!({ "engines": optimizer.engines() }))
}

async fn analyze(
    State(optimizer): State<Arc<Optimizer>>,
    Json(request): Json<OptimizeRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::to_value(optimizer.analyze(&request)?)?))
}

async fn optimize(
    State(optimizer): State<Arc<Optimizer>>,
    Json(request): Json<OptimizeRequest>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::to_value(optimizer.optimize(&request)?)?))
}

async fn restore(
    State(optimizer): State<Arc<Optimizer>>,
    Json(capsule): Json<RecoveryCapsule>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::to_value(optimizer.restore(&capsule)?)?))
}

async fn observe(
    State(optimizer): State<Arc<Optimizer>>,
    Json(observation): Json<UsageObservation>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(serde_json::to_value(optimizer.observe(observation))?))
}

struct ApiError(String);

impl From<OptimizeError> for ApiError {
    fn from(error: OptimizeError) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({
                "error": {
                    "code": "optimization_rejected",
                    "message": self.0
                }
            })),
        )
            .into_response()
    }
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
