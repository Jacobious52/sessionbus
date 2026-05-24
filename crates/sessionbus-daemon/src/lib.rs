use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use sessionbus_core::{
    CapabilityDescriptor, ContextPack, CreateArtifactRequest, CreateDecisionRequest,
    CreateSessionRequest, PackProfile,
};
use sessionbus_store::SessionbusStore;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Clone)]
pub struct AppState {
    store: SessionbusStore,
}

pub fn router(store: SessionbusStore) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/sessions", post(create_session).get(list_sessions))
        .route("/sessions/:id", get(get_session))
        .route(
            "/sessions/:id/artifacts",
            post(add_artifact).get(list_artifacts),
        )
        .route(
            "/sessions/:id/decisions",
            post(add_decision).get(list_decisions),
        )
        .route("/sessions/:id/pack", post(pack_session))
        .route("/events", get(events))
        .route("/adapters/register", post(register_adapter))
        .with_state(AppState { store })
}

pub async fn serve(bind: SocketAddr, store: SessionbusStore) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(bind).await?;
    axum::serve(listener, router(store)).await?;
    Ok(())
}

pub fn default_db_path() -> PathBuf {
    if let Ok(path) = std::env::var("SESSIONBUS_DB") {
        return PathBuf::from(path);
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("sessionbus")
            .join("sessionbus.db");
    }
    PathBuf::from(".sessionbus").join("sessionbus.db")
}

async fn healthz() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "service": "sessionbus-daemon" }))
}

#[tracing::instrument(skip(state, request))]
async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let session = state.store.create_session(request).await?;
    Ok((StatusCode::CREATED, Json(session)))
}

#[tracing::instrument(skip(state))]
async fn list_sessions(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.store.list_sessions().await?))
}

#[tracing::instrument(skip(state))]
async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let session = state
        .store
        .get_session(&id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("session not found: {id}")))?;
    Ok(Json(session))
}

#[tracing::instrument(skip(state, request))]
async fn add_artifact(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateArtifactRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let artifact = state.store.add_artifact(&id, request).await?;
    Ok((StatusCode::CREATED, Json(artifact)))
}

#[tracing::instrument(skip(state))]
async fn list_artifacts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .store
        .get_session(&id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("session not found: {id}")))?;
    Ok(Json(state.store.list_artifacts(&id).await?))
}

#[tracing::instrument(skip(state, request))]
async fn add_decision(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<CreateDecisionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let decision = state.store.add_decision(&id, request).await?;
    Ok((StatusCode::CREATED, Json(decision)))
}

#[tracing::instrument(skip(state))]
async fn list_decisions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    state
        .store
        .get_session(&id)
        .await?
        .ok_or_else(|| ApiError::not_found(format!("session not found: {id}")))?;
    Ok(Json(state.store.list_decisions(&id).await?))
}

#[derive(Debug, Deserialize)]
struct PackRequest {
    profile: PackProfile,
}

#[tracing::instrument(skip(state, request))]
async fn pack_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<PackRequest>,
) -> Result<Json<ContextPack>, ApiError> {
    Ok(Json(state.store.pack_session(&id, request.profile).await?))
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    session_id: Option<String>,
}

#[tracing::instrument(skip(state))]
async fn events(
    State(state): State<AppState>,
    Query(query): Query<EventsQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let events = state.store.list_events(query.session_id.as_deref()).await?;
    let mut body = String::new();
    for event in events {
        body.push_str(&serde_json::to_string(&event)?);
        body.push('\n');
    }
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-ndjson"),
    );
    Ok((headers, body))
}

#[tracing::instrument(skip(state, descriptor))]
async fn register_adapter(
    State(state): State<AppState>,
    Json(descriptor): Json<CapabilityDescriptor>,
) -> Result<impl IntoResponse, ApiError> {
    let registration = state.store.register_adapter(descriptor).await?;
    Ok((StatusCode::CREATED, Json(registration)))
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message,
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        let message = error.to_string();
        let status = if message.contains("not found") {
            StatusCode::NOT_FOUND
        } else {
            StatusCode::BAD_REQUEST
        };
        Self { status, message }
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(error: serde_json::Error) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({
                "error": self.message,
                "status": self.status.as_u16()
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request},
    };
    use serde_json::{json, Value};
    use sessionbus_core::ArtifactKind;
    use tower::ServiceExt;

    #[tokio::test]
    async fn api_creates_artifacts_packs_and_exports_ndjson_events() {
        let store = SessionbusStore::in_memory().await.unwrap();
        let app = router(store);

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/sessions",
                json!({ "title": "Fix flaky deploy" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let session: Value = response_json(response).await;
        let session_id = session["id"].as_str().unwrap();

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                &format!("/sessions/{session_id}/artifacts"),
                json!({
                    "kind": ArtifactKind::Note,
                    "title": "staging clue",
                    "body": "TOKEN=super-secret\nOnly staging fails",
                    "snapshot": true
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                &format!("/sessions/{session_id}/pack"),
                json!({ "profile": "chatgpt" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let pack: Value = response_json(response).await;
        assert!(pack["markdown"]
            .as_str()
            .unwrap()
            .contains("TOKEN=[REDACTED]"));

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/events?session_id={session_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(body.contains("session.created"));
        assert!(body.contains("artifact.added"));
        assert!(body.contains("context.packed"));
    }

    fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
