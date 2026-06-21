use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use reqwest::Client;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::config::Config;
use crate::routes;

// ---------------------------------------------------------------------------
// Shared application state
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub client: Client,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        // build() only fails when the TLS backend can't initialise —
        // unrecoverable; a panic here is deliberate.
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_secs))
            .build()
            .expect("Failed to create reqwest client (TLS backend unavailable)");
        AppState {
            config: Arc::new(config),
            client,
        }
    }

    /// Build state for unit tests — points to an unreachable upstream so tests
    /// exercise deserialization / error paths without hitting the network.
    #[cfg(test)]
    pub fn for_tests() -> Self {
        let config = Config {
            hsl_subscription_key: "test-key".into(),
            listen_addr: "0.0.0.0:3001".into(),
            digitransit_endpoint: "https://example.invalid/graphql".into(),
            request_timeout_secs: 5,
        };
        Self::new(config)
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(routes::health::handler))
        .route("/api/hsl/routing", post(routes::hsl_proxy::handler))
        .layer(DefaultBodyLimit::max(256 * 1024)) // 256 KiB
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Build a router with test state — injected into handler unit tests.
#[cfg(test)]
pub fn build_router_for_tests() -> Router {
    build_router(AppState::for_tests())
}

// ---------------------------------------------------------------------------
// Run
// ---------------------------------------------------------------------------

pub async fn run(config: Config) {
    let addr = config.listen_addr.clone();
    let state = AppState::new(config);
    let app = build_router(state);

    info!("Starting hsl-proxy on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind address");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server exited with error");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install Ctrl+C handler");
    info!("Shutting down");
}
