use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use serde_json::json;

/// Unified error type for the server.
///
/// Implements `IntoResponse` so handlers can return `Result<T, AppError>` directly.
/// Axum extracts the status code and JSON body automatically.
#[derive(Debug)]
pub enum AppError {
    /// Upstream API returned a non-2xx status.
    BadGateway { detail: serde_json::Value },
    /// Internal server error (config, unexpected failures).
    Internal(String),
    // Future: add Unprocessable variant here for semantic validation.
}

impl AppError {
    pub fn bad_gateway(detail: impl Into<String>) -> Self {
        AppError::BadGateway {
            detail: json!({ "error": detail.into() }),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        AppError::Internal(msg.into())
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, body) = match self {
            AppError::BadGateway { detail } => (StatusCode::BAD_GATEWAY, detail),
            AppError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, json!({ "error": msg })),
        };
        (status, Json(body)).into_response()
    }
}
