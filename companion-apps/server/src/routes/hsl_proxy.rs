use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue};
use axum::response::Json;
use serde::Deserialize;
use serde_json::Value;
use tracing::error;

use crate::app::AppState;
use crate::error::AppError;

// ---------------------------------------------------------------------------
// Request type
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ProxyRequest {
    pub query: String,
    #[serde(default)]
    pub variables: Value,
    /// Optional GraphQL `operationName` — required when a document contains
    /// multiple operations; Digitransit-specific queries are single-operation
    /// today but this keeps the proxy spec-compliant for future use.
    #[serde(default)]
    pub operation_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn handler(
    State(state): State<AppState>,
    Json(body): Json<ProxyRequest>,
) -> Result<Json<Value>, AppError> {
    let mut digitransit_body = serde_json::json!({
        "query": body.query,
        "variables": body.variables,
    });
    if let Some(op) = &body.operation_name {
        digitransit_body["operationName"] = serde_json::Value::String(op.clone());
    }

    let mut req_headers = HeaderMap::new();
    req_headers.insert(
        "content-type",
        HeaderValue::from_static("application/json"),
    );
    req_headers.insert(
        "digitransit-subscription-key",
        HeaderValue::from_str(&state.config.hsl_subscription_key).map_err(|e| {
            error!("Invalid HSL_SUBSCRIPTION_KEY: {e}");
            AppError::internal(format!("Invalid subscription key: {e}"))
        })?,
    );

    let response = state
        .client
        .post(&state.config.digitransit_endpoint)
        .headers(req_headers)
        .json(&digitransit_body)
        .send()
        .await
        .map_err(|e| {
            error!("Upstream request failed: {e}");
            AppError::bad_gateway(format!("Upstream request failed: {e}"))
        })?;

    let status = response.status();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("(none)")
        .to_string();
    let content_encoding = response
        .headers()
        .get("content-encoding")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("(none)")
        .to_string();
    let response_text = response.text().await.map_err(|e| {
        error!("Failed to read upstream response body (HTTP {status}): {e}");
        AppError::bad_gateway(format!("Failed to read upstream response: {e}"))
    })?;
    let response_json: Value = serde_json::from_str(&response_text).map_err(|e| {
        error!(
            "Failed to parse upstream response (HTTP {status}, type={content_type}, encoding={content_encoding}, body={:.200}): {e}",
            response_text
        );
        AppError::bad_gateway(format!(
            "Upstream returned HTTP {status} with unparseable body"
        ))
    })?;

    if !status.is_success() {
        error!("Upstream returned {status}: {response_json}");
        return Err(AppError::BadGateway {
            detail: response_json,
        });
    }

    Ok(Json(response_json))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    use crate::app;

    fn test_json_request(body: &'static str) -> Request<Body> {
        Request::builder()
            .method("POST")
            .uri("/api/hsl/routing")
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap()
    }

    #[tokio::test]
    async fn missing_query_field_returns_422() {
        let app = app::build_router_for_tests();
        let response = app
            .oneshot(test_json_request(r#"{"variables":{}}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn empty_body_returns_422() {
        let app = app::build_router_for_tests();
        let response = app
            .oneshot(test_json_request(r#"{}"#))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn non_json_body_returns_400() {
        let app = app::build_router_for_tests();
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/hsl/routing")
                    .header("content-type", "application/json")
                    .body(Body::from("not json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
