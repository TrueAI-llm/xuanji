use crate::types::{TriggerEvent, TriggerSender};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::Json;
use axum::routing::MethodRouter;
use axum::Router;
use serde_json::Value;
use std::collections::HashMap;

/// Shared state for webhook handlers.
#[derive(Clone)]
pub struct WebhookState {
    pub workflow_name: String,
    pub sender: TriggerSender,
}

/// Register a webhook route on an axum Router.
pub fn register_webhook_route(
    router: Router,
    workflow_name: &str,
    path: &str,
    method: &str,
    sender: TriggerSender,
) -> Router {
    let state = WebhookState {
        workflow_name: workflow_name.to_string(),
        sender,
    };

    let route = match method.to_uppercase().as_str() {
        "GET" => MethodRouter::new().get(webhook_handler),
        "PUT" => MethodRouter::new().put(webhook_handler),
        "DELETE" => MethodRouter::new().delete(webhook_handler),
        _ => MethodRouter::new().post(webhook_handler), // default POST
    };

    router.route(path, route.with_state(state))
}

/// Handler for incoming webhook requests.
async fn webhook_handler(
    State(state): State<WebhookState>,
    headers: HeaderMap,
    body: Bytes,
) -> Json<Value> {
    // Extract headers as a simple map
    let headers_map: HashMap<String, String> = headers
        .iter()
        .map(|(name, val)| {
            (
                name.to_string(),
                val.to_str().unwrap_or("").to_string(),
            )
        })
        .collect();

    // Try to parse body as JSON, fallback to string
    let body_value: Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
        Value::String(String::from_utf8_lossy(&body).to_string())
    });

    let event = TriggerEvent {
        trigger_type: "webhook".to_string(),
        workflow_name: state.workflow_name.clone(),
        payload: serde_json::json!({
            "headers": headers_map,
            "body": body_value,
        }),
    };

    if state.sender.send(event).await.is_err() {
        return Json(serde_json::json!({
            "status": "error",
            "message": "trigger channel closed"
        }));
    }

    Json(serde_json::json!({
        "status": "ok",
        "workflow": state.workflow_name
    }))
}

/// Build a router with all webhook trigger routes registered.
pub fn build_webhook_router(
    routes: &[(String, String, String)], // (workflow_name, path, method)
    sender: TriggerSender,
) -> Router {
    let mut router = Router::new();
    for (workflow_name, path, method) in routes {
        router = register_webhook_route(router, workflow_name, path, method, sender.clone());
    }
    router
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_webhook_handler() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(16);
        let router = register_webhook_route(
            Router::new(),
            "test-workflow",
            "/deploy",
            "POST",
            tx,
        );

        let app = router;

        let req = Request::builder()
            .method("POST")
            .uri("/deploy")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"environment": "staging"}"#))
            .unwrap();

        let response = app.oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let event = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(event.trigger_type, "webhook");
        assert_eq!(event.workflow_name, "test-workflow");
        assert_eq!(event.payload["body"]["environment"], "staging");
    }
}
