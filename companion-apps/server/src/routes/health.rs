use axum::response::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
}

pub async fn handler() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".into(),
    })
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::app;

    #[tokio::test]
    async fn returns_ok() {
        let app = app::build_router_for_tests();
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["status"], "ok");
    }
}
