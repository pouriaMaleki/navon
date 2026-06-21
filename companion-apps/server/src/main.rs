//! HSL Digitransit routing proxy.
//!
//! Server that accepts the same GraphQL body the companion apps already produce,
//! injects the server-side `digitransit-subscription-key` header, forwards to
//! the Digitransit HSL API, and returns the response as-is.  End users never see
//! or enter an API key.
//!
//! ## Endpoints
//! - `GET  /health`          — liveness probe
//! - `POST /api/hsl/routing` — GraphQL proxy to Digitransit HSL Routing API v2
//!
//! ## Extending
//! - Add new handlers under `routes/` and mount them in `app::build_router`.
//! - Add new error variants to `error::AppError` for consistent JSON responses.
//! - Per-endpoint request/response types live alongside their handler.

mod app;
mod config;
mod error;
mod routes;

#[tokio::main]
async fn main() {
    // Check the required env var before anything else so the error is
    // impossible to miss in `docker logs`.
    if std::env::var("HSL_SUBSCRIPTION_KEY").is_err() {
        eprintln!(
            "FATAL: HSL_SUBSCRIPTION_KEY is not set. \
             Set it in .env or export it before starting the container."
        );
        std::process::exit(1);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hsl_proxy=info,tower_http=info".into()),
        )
        .init();

    let config = config::Config::from_env().expect("Failed to load config");
    app::run(config).await;
}
