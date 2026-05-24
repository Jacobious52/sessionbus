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
    CreateSessionRequest, PackProfile, UpdateSessionStatusRequest,
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
        .route("/dashboard", get(dashboard))
        .route("/api/dashboard", get(dashboard_api))
        .route("/sessions", post(create_session).get(list_sessions))
        .route("/sessions/:id", get(get_session))
        .route("/sessions/:id/status", post(update_session_status))
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

async fn dashboard() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    (headers, DASHBOARD_HTML)
}

#[tracing::instrument(skip(state))]
async fn dashboard_api(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let sessions = state.store.list_sessions().await?;
    let mut recent_artifacts = Vec::new();
    for session in sessions.iter().rev().take(8) {
        let mut artifacts = state.store.list_artifacts(&session.id).await?;
        recent_artifacts.append(&mut artifacts);
    }
    recent_artifacts.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    recent_artifacts.truncate(12);
    let events = state.store.list_events(None).await?;
    Ok(Json(json!({
        "service": "sessionbus-daemon",
        "tagline": "Never re-explain the same engineering task to multiple AI tools again.",
        "sessions": sessions,
        "recent_artifacts": recent_artifacts,
        "recent_events": events.into_iter().rev().take(25).collect::<Vec<_>>()
    })))
}

#[tracing::instrument(skip(state, request))]
async fn create_session(
    State(state): State<AppState>,
    Json(request): Json<CreateSessionRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let session = state.store.create_session(request).await?;
    Ok((StatusCode::CREATED, Json(session)))
}

const DASHBOARD_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>Sessionbus Dashboard</title>
  <style>
    :root {
      color-scheme: dark;
      --bg: #101114;
      --panel: #171a20;
      --panel-2: #20242c;
      --text: #f5f7fb;
      --muted: #aab2c0;
      --line: #303642;
      --accent: #7dd3fc;
      --good: #86efac;
      --warn: #fbbf24;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font: 14px/1.45 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: radial-gradient(circle at 20% 0%, #1b2a35 0, #101114 30rem);
      color: var(--text);
    }
    header {
      min-height: 44vh;
      padding: 48px max(24px, 8vw) 28px;
      display: flex;
      flex-direction: column;
      justify-content: flex-end;
      border-bottom: 1px solid var(--line);
    }
    h1 {
      margin: 0;
      font-size: clamp(42px, 7vw, 92px);
      line-height: 0.95;
      letter-spacing: 0;
    }
    .tagline {
      max-width: 760px;
      margin-top: 18px;
      color: var(--muted);
      font-size: 20px;
    }
    main {
      padding: 28px max(24px, 8vw) 56px;
      display: grid;
      gap: 18px;
      grid-template-columns: minmax(0, 1.2fr) minmax(280px, 0.8fr);
    }
    section {
      background: color-mix(in srgb, var(--panel) 92%, transparent);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 18px;
    }
    h2 { margin: 0 0 14px; font-size: 16px; }
    form {
      display: grid;
      gap: 10px;
      margin-bottom: 18px;
      padding: 12px;
      background: var(--panel-2);
      border: 1px solid var(--line);
      border-radius: 8px;
    }
    input, textarea, select, button {
      width: 100%;
      border: 1px solid var(--line);
      border-radius: 6px;
      background: #111318;
      color: var(--text);
      padding: 10px 12px;
      font: inherit;
    }
    textarea { min-height: 78px; resize: vertical; }
    button {
      cursor: pointer;
      background: var(--accent);
      border-color: var(--accent);
      color: #081018;
      font-weight: 700;
    }
    button.secondary {
      background: var(--panel-2);
      border-color: var(--line);
      color: var(--text);
    }
    button.danger {
      background: #2a171b;
      border-color: #7f1d1d;
      color: #fecaca;
    }
    .session, .event {
      display: grid;
      gap: 6px;
      padding: 12px 0;
      border-top: 1px solid var(--line);
    }
    .session:first-of-type, .event:first-of-type { border-top: 0; }
    .row {
      display: flex;
      gap: 10px;
      align-items: center;
      justify-content: space-between;
    }
    code {
      color: var(--accent);
      background: var(--panel-2);
      padding: 2px 6px;
      border-radius: 5px;
    }
    .status {
      color: var(--good);
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: .08em;
    }
    .muted { color: var(--muted); }
    .controls { grid-column: 1 / -1; }
    .operator-grid {
      display: grid;
      gap: 18px;
      grid-template-columns: minmax(0, 1fr) minmax(280px, .72fr);
    }
    .control-grid {
      display: grid;
      gap: 12px;
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .session-actions {
      display: flex;
      gap: 8px;
      flex-wrap: wrap;
      margin-top: 4px;
    }
    .session-actions button {
      width: auto;
      min-width: 104px;
      padding: 7px 10px;
    }
    .pack-toolbar {
      display: flex;
      gap: 10px;
      align-items: center;
      justify-content: space-between;
      margin: 14px 0 8px;
    }
    .pack-toolbar button {
      width: auto;
      min-width: 104px;
      padding: 8px 10px;
    }
    #pack-output {
      max-height: 260px;
      overflow: auto;
      white-space: pre-wrap;
      color: var(--muted);
      background: #0d0f13;
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 12px;
    }
    @media (max-width: 840px) {
      main { grid-template-columns: 1fr; }
      header { min-height: 36vh; }
      .control-grid { grid-template-columns: 1fr; }
      .operator-grid { grid-template-columns: 1fr; }
    }
  </style>
</head>
<body>
  <header>
    <div class="status">Local-first AI workflow continuity</div>
    <h1>Sessionbus</h1>
    <div class="tagline">Never re-explain the same engineering task to multiple AI tools again.</div>
  </header>
  <main>
    <section class="controls">
      <h2>Control Surface</h2>
      <div class="control-grid">
        <form id="start-form">
          <strong>Start Session</strong>
          <input name="title" placeholder="Fix flaky deploy" required />
          <textarea name="summary" placeholder="Optional task summary"></textarea>
          <button type="submit">Start</button>
        </form>
        <form id="note-form">
          <strong>Add Note</strong>
          <select name="session_id" class="session-select"></select>
          <textarea name="text" placeholder="What should future tools know?" required></textarea>
          <button type="submit">Add Note</button>
        </form>
        <form id="pack-form">
          <strong>Render Pack</strong>
          <select name="session_id" class="session-select"></select>
          <select name="profile">
            <option value="generic">Generic</option>
            <option value="chatgpt">ChatGPT</option>
            <option value="claude">Claude</option>
            <option value="cursor">Cursor</option>
            <option value="acp">ACP</option>
          </select>
          <button type="submit">Render</button>
        </form>
      </div>
      <div class="pack-toolbar">
        <span class="muted" id="pack-status">Choose a session and render a pack.</span>
        <button type="button" id="copy-pack" class="secondary">Copy Pack</button>
      </div>
      <pre id="pack-output"></pre>
    </section>
    <div class="operator-grid" style="grid-column: 1 / -1;">
      <section>
        <h2>Sessions</h2>
        <div id="sessions" class="muted">Loading sessions...</div>
      </section>
      <section>
        <h2>Recent Artifacts</h2>
        <div id="artifacts" class="muted">Loading artifacts...</div>
      </section>
    </div>
    <section style="grid-column: 1 / -1;">
      <h2>Recent Events</h2>
      <div id="events" class="muted">Loading events...</div>
    </section>
  </main>
  <script>
    async function load() {
      const data = await fetch('/api/dashboard').then((response) => response.json());
      window.sessionbusDashboard = data;
      hydrateSessionSelects(data.sessions);
      const sessions = document.querySelector('#sessions');
      sessions.innerHTML = data.sessions.map((session) => `
        <div class="session">
          <div class="row"><strong>${escapeHtml(session.title)}</strong><span class="status">${escapeHtml(session.status)}</span></div>
          <div><code>${escapeHtml(session.id)}</code></div>
          <div class="muted">${escapeHtml(session.workspace?.root || 'No workspace')}</div>
          <div class="session-actions">
            <button type="button" class="secondary" data-render-session="${escapeHtml(session.id)}">Render Pack</button>
            <button type="button" class="danger" data-close-session="${escapeHtml(session.id)}">Close</button>
          </div>
        </div>
      `).join('') || '<div class="muted">No sessions yet.</div>';
      const artifacts = document.querySelector('#artifacts');
      artifacts.innerHTML = data.recent_artifacts.map((artifact) => `
        <div class="event">
          <div class="row"><strong>${escapeHtml(artifact.title || 'untitled')}</strong><code>${escapeHtml(artifact.kind)}</code></div>
          <div class="muted">${escapeHtml(artifact.session_id)} · ${escapeHtml(artifact.created_at)}</div>
        </div>
      `).join('') || '<div class="muted">No artifacts yet.</div>';
      const events = document.querySelector('#events');
      events.innerHTML = data.recent_events.map((event) => `
        <div class="event">
          <div><code>${escapeHtml(event.type)}</code></div>
          <div class="muted">${escapeHtml(event.session_id || 'global')} · ${escapeHtml(event.created_at)}</div>
        </div>
      `).join('') || '<div class="muted">No events yet.</div>';
    }
    function hydrateSessionSelects(sessions) {
      for (const select of document.querySelectorAll('.session-select')) {
        const current = select.value;
        select.innerHTML = sessions.map((session) => `
          <option value="${escapeHtml(session.id)}">${escapeHtml(session.title)} (${escapeHtml(session.status)})</option>
        `).join('');
        if (current) select.value = current;
      }
    }
    document.querySelector('#start-form').addEventListener('submit', async (event) => {
      event.preventDefault();
      const form = new FormData(event.currentTarget);
      await fetch('/sessions', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          title: form.get('title'),
          summary: form.get('summary') || undefined
        })
      });
      event.currentTarget.reset();
      await load();
    });
    document.querySelector('#note-form').addEventListener('submit', async (event) => {
      event.preventDefault();
      const form = new FormData(event.currentTarget);
      await fetch(`/sessions/${encodeURIComponent(form.get('session_id'))}/artifacts`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({
          kind: 'note',
          title: 'dashboard note',
          body: form.get('text'),
          metadata: { source: 'dashboard' },
          snapshot: true
        })
      });
      event.currentTarget.reset();
      await load();
    });
    document.querySelector('#pack-form').addEventListener('submit', async (event) => {
      event.preventDefault();
      const form = new FormData(event.currentTarget);
      const pack = await fetch(`/sessions/${encodeURIComponent(form.get('session_id'))}/pack`, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ profile: form.get('profile') })
      }).then((response) => response.json());
      document.querySelector('#pack-output').textContent = pack.markdown;
      document.querySelector('#pack-status').textContent = `Rendered ${form.get('profile')} pack`;
      await load();
    });
    document.querySelector('#sessions').addEventListener('click', async (event) => {
      const renderSession = event.target?.dataset?.renderSession;
      const closeSession = event.target?.dataset?.closeSession;
      if (renderSession) {
        const pack = await fetch(`/sessions/${encodeURIComponent(renderSession)}/pack`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ profile: 'generic' })
        }).then((response) => response.json());
        document.querySelector('#pack-output').textContent = pack.markdown;
        document.querySelector('#pack-status').textContent = 'Rendered generic pack';
        await load();
      }
      if (closeSession) {
        await fetch(`/sessions/${encodeURIComponent(closeSession)}/status`, {
          method: 'POST',
          headers: { 'content-type': 'application/json' },
          body: JSON.stringify({ status: 'done' })
        });
        await load();
      }
    });
    document.querySelector('#copy-pack').addEventListener('click', async () => {
      const text = document.querySelector('#pack-output').textContent;
      if (!text.trim()) return;
      await navigator.clipboard.writeText(text);
      document.querySelector('#pack-status').textContent = 'Copied pack';
    });
    function escapeHtml(value) {
      return String(value).replace(/[&<>"']/g, (char) => ({
        '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'
      })[char]);
    }
    load().catch((error) => {
      document.querySelector('#sessions').textContent = error.message;
    });
  </script>
</body>
</html>"#;

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
async fn update_session_status(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(request): Json<UpdateSessionStatusRequest>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(
        state
            .store
            .update_session_status(&id, request.status)
            .await?,
    ))
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
