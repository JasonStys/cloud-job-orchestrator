//! File: Adds an explicit owned-loopback capability boundary and real native worker controls.
//! Functions: `router` preserves original routes; `guard` admits only the owned host/origin/capability.
//! Variables: public instance identity and pause/active counters are local observations, not user roles.

use crate::http::{self, AppState};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, Request, State},
    http::{HeaderValue, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use subtle::ConstantTimeEq;
use tokio::sync::watch;

/// Explicit native-owner state. The request capability deliberately has no derived Debug output.
#[derive(Clone)]
pub struct Boundary {
    authority: String,
    capability: [u8; 64],
    instance: String,
    paused: watch::Sender<bool>,
    active: Arc<AtomicUsize>,
}

impl Boundary {
    /// Bind public HTTP identity and the fixed-size private capability to one actual owned port.
    pub fn new(
        address: SocketAddr,
        capability: &str,
        instance: &str,
        paused: watch::Sender<bool>,
        active: Arc<AtomicUsize>,
    ) -> Result<Self, &'static str> {
        if address.ip() != IpAddr::V4(Ipv4Addr::LOCALHOST)
            || address.port() == 0
            || !hex_identity(capability)
            || !hex_identity(instance)
        {
            return Err("Native HTTP identity must be a fixed owned loopback address");
        }
        let mut token = [0_u8; 64];
        token.copy_from_slice(capability.as_bytes());
        Ok(Self {
            authority: address.to_string(),
            capability: token,
            instance: instance.to_owned(),
            paused,
            active,
        })
    }

    /// Clone the actual pause subscription; a pause request never cancels a running job.
    pub fn subscribe(&self) -> watch::Receiver<bool> {
        self.paused.subscribe()
    }
}

fn hex_identity(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// Add owner-local controls around the genuine original PostgreSQL/API implementation.
pub fn router(state: AppState, boundary: Boundary) -> Router {
    let status = boundary.clone();
    let mode = boundary.clone();
    let recovery_store = state.store.clone();
    http::router(state)
        .route("/native/status", get(move || {
            let status = status.clone();
            async move { Json(json!({
                "product": "cloud-job-orchestrator", "version": env!("CARGO_PKG_VERSION"),
                "instance": status.instance, "pause_requested": *status.paused.borrow(),
                "active_jobs": status.active.load(Ordering::Relaxed),
                "workers": 2, "notice": "Trusted OS-owner workspace; no authenticated enterprise actors"
            })) }
        }))
        .route("/native/workers", post(move |Json(input): Json<WorkerMode>| {
            let mode = mode.clone();
            async move {
                mode.paused.send_replace(input.paused);
                Json(json!({"pause_requested": input.paused,
                    "notice": "Current jobs finish before pause or shutdown; leases are not abandoned"}))
            }
        }))
        .route("/native/reap", post(move || {
            let store = recovery_store.clone();
            async move {
                store.reap().await.map_or_else(
                    |_| (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "Recovery failed; inspect the retained workflow"}))),
                    |count| (StatusCode::OK, Json(json!({"recovered": count}))),
                )
            }
        }))
        .layer(DefaultBodyLimit::max(1_048_576))
        .layer(middleware::from_fn_with_state(boundary, guard))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkerMode {
    paused: bool,
}

/// Enforce the exact owned host/origin; only public source assets are readable without capability.
async fn guard(State(boundary): State<Boundary>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let own_origin = format!("http://{}", boundary.authority);
    let valid_host = headers.get_all("host").iter().count() == 1
        && headers.get("host").and_then(|value| value.to_str().ok()) == Some(&boundary.authority);
    let valid_origin = !headers.contains_key("origin")
        || (headers.get_all("origin").iter().count() == 1
            && headers.get("origin").and_then(|value| value.to_str().ok()) == Some(&own_origin));
    let public_asset = matches!(*request.method(), Method::GET | Method::HEAD)
        && matches!(
            request.uri().path(),
            "/" | "/index.html" | "/styles.css" | "/app.js" | "/api.js" | "/model.js"
        );
    let token = headers
        .get("x-cjo-native")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let capability = headers.get_all("x-cjo-native").iter().count() == 1
        && hex_identity(token)
        && bool::from(boundary.capability.as_slice().ct_eq(token.as_bytes()));
    if !valid_host
        || !valid_origin
        || (!public_asset && !capability)
        || request.uri().authority().is_some()
    {
        return (StatusCode::FORBIDDEN, "Owned native request required").into_response();
    }
    let mut response = tokio::time::timeout(std::time::Duration::from_secs(10), next.run(request))
        .await
        .unwrap_or_else(|_| {
            (
                StatusCode::REQUEST_TIMEOUT,
                "Owned request exceeded its processing bound",
            )
                .into_response()
        });
    let headers = response.headers_mut();
    headers.insert("cache-control", HeaderValue::from_static("no-store"));
    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("content-security-policy", HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"));
    response
}

#[cfg(test)]
mod tests {
    use super::{Boundary, router};
    use crate::{PostgresStore, http::AppState};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use std::{
        net::SocketAddr,
        sync::{Arc, atomic::AtomicUsize},
    };
    use tokio::sync::watch;
    use tower::ServiceExt;

    fn application() -> axum::Router {
        let (paused, _) = watch::channel(true);
        let address = SocketAddr::from(([127, 0, 0, 1], 18234));
        #[allow(clippy::unwrap_used)]
        let boundary = Boundary::new(
            address,
            &"a".repeat(64),
            &"b".repeat(64),
            paused,
            Arc::new(AtomicUsize::new(0)),
        )
        .unwrap();
        router(
            AppState {
                store: PostgresStore::disconnected_test_store(),
            },
            boundary,
        )
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn exact_owner_status_and_pause_contract_without_any_database_work() {
        let request = Request::builder()
            .uri("/native/status")
            .header("host", "127.0.0.1:18234")
            .header("origin", "http://127.0.0.1:18234")
            .header("x-cjo-native", "a".repeat(64))
            .body(Body::empty())
            .unwrap();
        let response = application().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let raw = to_bytes(response.into_body(), 4096).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(value["active_jobs"], 0);
        assert_eq!(value["pause_requested"], true);
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert!(
            !String::from_utf8(raw.to_vec())
                .unwrap()
                .contains(&"a".repeat(64))
        );
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn foreign_host_origin_and_missing_wrong_or_duplicate_capability_are_refused() {
        for (host, origin, token, duplicate) in [
            (
                "outside.invalid",
                "http://127.0.0.1:18234",
                "a".repeat(64),
                false,
            ),
            (
                "127.0.0.1:18234",
                "https://outside.invalid",
                "a".repeat(64),
                false,
            ),
            (
                "127.0.0.1:18234",
                "http://127.0.0.1:18234",
                String::new(),
                false,
            ),
            (
                "127.0.0.1:18234",
                "http://127.0.0.1:18234",
                "f".repeat(64),
                false,
            ),
            (
                "127.0.0.1:18234",
                "http://127.0.0.1:18234",
                "a".repeat(64),
                true,
            ),
        ] {
            let mut builder = Request::builder()
                .uri("/native/status")
                .header("host", host)
                .header("origin", origin)
                .header("x-cjo-native", &token);
            if duplicate {
                builder = builder.header("x-cjo-native", &token);
            }
            let response = application()
                .oneshot(builder.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn authorized_preview_keeps_original_graph_validation_and_does_not_submit() {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/plans")
            .header("host", "127.0.0.1:18234")
            .header("x-cjo-native", "a".repeat(64))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"id":"bad","jobs":[],"dependencies":[]}"#))
            .unwrap();
        let response = application().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
