use axum::{routing::get, Json, Router};
use serde_json::json;
use std::sync::Arc;

use jarvis_rs::agent_core::build_runtime_manifest;
use jarvis_rs::automation::{AutomationBackend, LocalAutomationBackend};
use jarvis_rs::config::AppConfig;
use jarvis_rs::logging;

#[derive(Clone)]
struct AppState {
    manifest: Arc<jarvis_rs::agent_core::RuntimeManifest>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    logging::init();

    let config = AppConfig::from_env();
    let automation = LocalAutomationBackend::from_config(&config);
    let capabilities = automation.capabilities();
    let runtime_manifest = build_runtime_manifest(
        &config,
        &capabilities,
        &capabilities.setup_items,
        "runtime-server",
        "runtime-server",
    );
    let bind =
        std::env::var("JARVIS_AGENT_CORE_BIND").unwrap_or_else(|_| "127.0.0.1:8788".to_string());
    let app = Router::new()
        .route("/health", get(health))
        .route("/manifest", get(manifest_route))
        .with_state(AppState {
            manifest: Arc::new(runtime_manifest),
        });
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({
        "ok": true,
        "service": "jarvis-agent-core"
    }))
}

async fn manifest_route(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> Json<jarvis_rs::agent_core::RuntimeManifest> {
    Json((*state.manifest).clone())
}
