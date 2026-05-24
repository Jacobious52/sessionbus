use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sessionbus_core::{
    build_context_pack, sha256_hex, AdapterRegistration, Artifact, BusEvent, CapabilityDescriptor,
    ContextPack, CreateArtifactRequest, CreateDecisionRequest, CreateSessionRequest, Decision,
    EventType, PackInput, PackProfile, RedactionPolicy, Session, SessionStatus,
};
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};
use std::path::Path;
use uuid::Uuid;

const INIT_SQL: &str = include_str!("../migrations/001_init.sql");
const SOURCE_STORE: &str = "sessionbus-store";

#[derive(Debug, Clone)]
pub struct SessionbusStore {
    pool: SqlitePool,
}

impl SessionbusStore {
    pub async fn open_url(database_url: &str) -> Result<Self> {
        let max_connections = if database_url == "sqlite::memory:" {
            1
        } else {
            5
        };
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect(database_url)
            .await
            .with_context(|| format!("connect SQLite database at {database_url}"))?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub async fn open_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create data directory {}", parent.display()))?;
        }
        let url = format!("sqlite://{}?mode=rwc", path.display());
        Self::open_url(&url).await
    }

    pub async fn in_memory() -> Result<Self> {
        Self::open_url("sqlite::memory:").await
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<()> {
        for statement in INIT_SQL.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&self.pool).await?;
            }
        }
        Ok(())
    }

    pub async fn create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        if request.title.trim().is_empty() {
            return Err(anyhow!("session title cannot be empty"));
        }
        let now = Utc::now();
        let session = Session {
            id: new_id("ses"),
            title: request.title.trim().to_string(),
            status: SessionStatus::Active,
            workspace: request.workspace,
            summary: request.summary,
            created_at: now,
            updated_at: now,
        };
        sqlx::query(
            "INSERT INTO sessions (id, title, status, workspace_json, summary, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&session.id)
        .bind(&session.title)
        .bind(session.status.to_string())
        .bind(optional_json(&session.workspace)?)
        .bind(&session.summary)
        .bind(ts(session.created_at))
        .bind(ts(session.updated_at))
        .execute(&self.pool)
        .await?;

        self.append_event(
            Some(session.id.clone()),
            EventType::SessionCreated,
            SOURCE_STORE,
            json!({ "session": session }),
        )
        .await?;
        Ok(session)
    }

    pub async fn list_sessions(&self) -> Result<Vec<Session>> {
        let rows = sqlx::query(
            "SELECT id, title, status, workspace_json, summary, created_at, updated_at
             FROM sessions ORDER BY updated_at DESC, id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(session_from_row).collect()
    }

    pub async fn get_session(&self, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query(
            "SELECT id, title, status, workspace_json, summary, created_at, updated_at
             FROM sessions WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(session_from_row).transpose()
    }

    pub async fn update_session_status(
        &self,
        session_id: &str,
        status: SessionStatus,
    ) -> Result<Session> {
        self.ensure_session(session_id).await?;
        sqlx::query("UPDATE sessions SET status = ?, updated_at = ? WHERE id = ?")
            .bind(status.to_string())
            .bind(ts(Utc::now()))
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        let session = self
            .get_session(session_id)
            .await?
            .ok_or_else(|| anyhow!("session not found: {session_id}"))?;
        self.append_event(
            Some(session_id.to_string()),
            EventType::SessionUpdated,
            SOURCE_STORE,
            json!({ "session": session }),
        )
        .await?;
        Ok(session)
    }

    pub async fn add_artifact(
        &self,
        session_id: &str,
        request: CreateArtifactRequest,
    ) -> Result<Artifact> {
        self.ensure_session(session_id).await?;
        let now = Utc::now();
        let body = request.body;
        let content_hash = body
            .as_deref()
            .map(|value| format!("sha256:{}", sha256_hex(value)));
        let content_ref = if request.snapshot {
            content_hash.clone()
        } else {
            None
        };
        let artifact = Artifact {
            id: new_id("art"),
            session_id: session_id.to_string(),
            kind: request.kind,
            title: request.title,
            uri: request.uri,
            content_hash,
            content_ref,
            body,
            metadata: normalize_json(request.metadata),
            created_at: now,
        };

        sqlx::query(
            "INSERT INTO artifacts
             (id, session_id, kind, title, uri, content_hash, content_ref, body, metadata_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&artifact.id)
        .bind(&artifact.session_id)
        .bind(artifact.kind.to_string())
        .bind(&artifact.title)
        .bind(&artifact.uri)
        .bind(&artifact.content_hash)
        .bind(&artifact.content_ref)
        .bind(&artifact.body)
        .bind(serde_json::to_string(&artifact.metadata)?)
        .bind(ts(artifact.created_at))
        .execute(&self.pool)
        .await?;
        self.touch_session(session_id).await?;
        self.append_event(
            Some(session_id.to_string()),
            EventType::ArtifactAdded,
            SOURCE_STORE,
            json!({ "artifact": artifact }),
        )
        .await?;
        Ok(artifact)
    }

    pub async fn list_artifacts(&self, session_id: &str) -> Result<Vec<Artifact>> {
        let rows = sqlx::query(
            "SELECT id, session_id, kind, title, uri, content_hash, content_ref, body, metadata_json, created_at
             FROM artifacts WHERE session_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(artifact_from_row).collect()
    }

    pub async fn add_decision(
        &self,
        session_id: &str,
        request: CreateDecisionRequest,
    ) -> Result<Decision> {
        self.ensure_session(session_id).await?;
        if request.text.trim().is_empty() {
            return Err(anyhow!("decision text cannot be empty"));
        }
        let decision = Decision {
            id: new_id("dec"),
            session_id: session_id.to_string(),
            text: request.text.trim().to_string(),
            rationale: request.rationale,
            created_at: Utc::now(),
        };
        sqlx::query(
            "INSERT INTO decisions (id, session_id, text, rationale, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&decision.id)
        .bind(&decision.session_id)
        .bind(&decision.text)
        .bind(&decision.rationale)
        .bind(ts(decision.created_at))
        .execute(&self.pool)
        .await?;
        self.touch_session(session_id).await?;
        self.append_event(
            Some(session_id.to_string()),
            EventType::DecisionRecorded,
            SOURCE_STORE,
            json!({ "decision": decision }),
        )
        .await?;
        Ok(decision)
    }

    pub async fn list_decisions(&self, session_id: &str) -> Result<Vec<Decision>> {
        let rows = sqlx::query(
            "SELECT id, session_id, text, rationale, created_at
             FROM decisions WHERE session_id = ? ORDER BY created_at ASC, id ASC",
        )
        .bind(session_id)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(decision_from_row).collect()
    }

    pub async fn pack_session(
        &self,
        session_id: &str,
        profile: PackProfile,
    ) -> Result<ContextPack> {
        let session = self
            .get_session(session_id)
            .await?
            .ok_or_else(|| anyhow!("session not found: {session_id}"))?;
        let artifacts = self.list_artifacts(session_id).await?;
        let decisions = self.list_decisions(session_id).await?;
        let pack = build_context_pack(
            PackInput {
                session,
                artifacts,
                decisions,
            },
            profile,
            &RedactionPolicy::default(),
        )?;
        sqlx::query(
            "INSERT INTO packs (id, session_id, profile, markdown, json_body, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(new_id("pack"))
        .bind(&pack.session_id)
        .bind(pack.profile.to_string())
        .bind(&pack.markdown)
        .bind(serde_json::to_string(&pack.json)?)
        .bind(ts(pack.created_at))
        .execute(&self.pool)
        .await?;
        self.append_event(
            Some(session_id.to_string()),
            EventType::ContextPacked,
            SOURCE_STORE,
            json!({ "profile": profile, "pack_created_at": pack.created_at }),
        )
        .await?;
        Ok(pack)
    }

    pub async fn register_adapter(
        &self,
        descriptor: CapabilityDescriptor,
    ) -> Result<AdapterRegistration> {
        if descriptor.adapter_id.trim().is_empty() {
            return Err(anyhow!("adapter_id cannot be empty"));
        }
        let now = Utc::now();
        let existing = self.get_adapter(&descriptor.adapter_id).await?;
        let registered_at = existing
            .as_ref()
            .map(|registration| registration.registered_at)
            .unwrap_or(now);
        let registration = AdapterRegistration {
            descriptor,
            registered_at,
            last_seen_at: now,
        };
        sqlx::query(
            "INSERT INTO adapters (adapter_id, descriptor_json, registered_at, last_seen_at)
             VALUES (?, ?, ?, ?)
             ON CONFLICT(adapter_id) DO UPDATE SET
               descriptor_json = excluded.descriptor_json,
               last_seen_at = excluded.last_seen_at",
        )
        .bind(&registration.descriptor.adapter_id)
        .bind(serde_json::to_string(&registration.descriptor)?)
        .bind(ts(registration.registered_at))
        .bind(ts(registration.last_seen_at))
        .execute(&self.pool)
        .await?;
        self.append_event(
            None,
            EventType::AdapterRegistered,
            &registration.descriptor.adapter_id,
            json!({ "adapter": registration }),
        )
        .await?;
        Ok(registration)
    }

    pub async fn get_adapter(&self, adapter_id: &str) -> Result<Option<AdapterRegistration>> {
        let row = sqlx::query(
            "SELECT descriptor_json, registered_at, last_seen_at FROM adapters WHERE adapter_id = ?",
        )
        .bind(adapter_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(adapter_from_row).transpose()
    }

    pub async fn list_events(&self, session_id: Option<&str>) -> Result<Vec<BusEvent>> {
        let rows = if let Some(session_id) = session_id {
            sqlx::query(
                "SELECT id, session_id, event_type, source, payload_json, created_at
                 FROM events WHERE session_id = ? OR session_id IS NULL
                 ORDER BY created_at ASC, id ASC",
            )
            .bind(session_id)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, session_id, event_type, source, payload_json, created_at
                 FROM events ORDER BY created_at ASC, id ASC",
            )
            .fetch_all(&self.pool)
            .await?
        };
        rows.into_iter().map(event_from_row).collect()
    }

    async fn append_event(
        &self,
        session_id: Option<String>,
        event_type: EventType,
        source: &str,
        payload: Value,
    ) -> Result<BusEvent> {
        let event = BusEvent {
            id: new_id("evt"),
            session_id,
            event_type,
            source: source.to_string(),
            payload,
            created_at: Utc::now(),
        };
        sqlx::query(
            "INSERT INTO events (id, session_id, event_type, source, payload_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&event.id)
        .bind(&event.session_id)
        .bind(event_type_string(event.event_type)?)
        .bind(&event.source)
        .bind(serde_json::to_string(&event.payload)?)
        .bind(ts(event.created_at))
        .execute(&self.pool)
        .await?;
        Ok(event)
    }

    async fn ensure_session(&self, session_id: &str) -> Result<()> {
        if self.get_session(session_id).await?.is_some() {
            Ok(())
        } else {
            Err(anyhow!("session not found: {session_id}"))
        }
    }

    async fn touch_session(&self, session_id: &str) -> Result<()> {
        sqlx::query("UPDATE sessions SET updated_at = ? WHERE id = ?")
            .bind(ts(Utc::now()))
            .bind(session_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn session_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Session> {
    let status_raw: String = row.try_get("status")?;
    let status = serde_json::from_str(&format!("\"{}\"", status_raw))?;
    let workspace_json: Option<String> = row.try_get("workspace_json")?;
    Ok(Session {
        id: row.try_get("id")?,
        title: row.try_get("title")?,
        status,
        workspace: optional_from_json(workspace_json)?,
        summary: row.try_get("summary")?,
        created_at: parse_ts(row.try_get::<String, _>("created_at")?)?,
        updated_at: parse_ts(row.try_get::<String, _>("updated_at")?)?,
    })
}

fn artifact_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Artifact> {
    let kind_raw: String = row.try_get("kind")?;
    let kind = serde_json::from_str(&format!("\"{}\"", kind_raw))?;
    let metadata_json: String = row.try_get("metadata_json")?;
    Ok(Artifact {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        kind,
        title: row.try_get("title")?,
        uri: row.try_get("uri")?,
        content_hash: row.try_get("content_hash")?,
        content_ref: row.try_get("content_ref")?,
        body: row.try_get("body")?,
        metadata: serde_json::from_str(&metadata_json)?,
        created_at: parse_ts(row.try_get::<String, _>("created_at")?)?,
    })
}

fn decision_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Decision> {
    Ok(Decision {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        text: row.try_get("text")?,
        rationale: row.try_get("rationale")?,
        created_at: parse_ts(row.try_get::<String, _>("created_at")?)?,
    })
}

fn event_from_row(row: sqlx::sqlite::SqliteRow) -> Result<BusEvent> {
    let event_type_raw: String = row.try_get("event_type")?;
    let event_type = serde_json::from_str(&format!("\"{}\"", event_type_raw))?;
    let payload_json: String = row.try_get("payload_json")?;
    Ok(BusEvent {
        id: row.try_get("id")?,
        session_id: row.try_get("session_id")?,
        event_type,
        source: row.try_get("source")?,
        payload: serde_json::from_str(&payload_json)?,
        created_at: parse_ts(row.try_get::<String, _>("created_at")?)?,
    })
}

fn adapter_from_row(row: sqlx::sqlite::SqliteRow) -> Result<AdapterRegistration> {
    let descriptor_json: String = row.try_get("descriptor_json")?;
    Ok(AdapterRegistration {
        descriptor: serde_json::from_str(&descriptor_json)?,
        registered_at: parse_ts(row.try_get::<String, _>("registered_at")?)?,
        last_seen_at: parse_ts(row.try_get::<String, _>("last_seen_at")?)?,
    })
}

fn optional_json<T: serde::Serialize>(value: &Option<T>) -> Result<Option<String>> {
    value
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(Into::into)
}

fn optional_from_json<T: serde::de::DeserializeOwned>(value: Option<String>) -> Result<Option<T>> {
    value
        .map(|raw| serde_json::from_str(&raw))
        .transpose()
        .map_err(Into::into)
}

fn normalize_json(value: Value) -> Value {
    if value.is_null() {
        json!({})
    } else {
        value
    }
}

fn event_type_string(event_type: EventType) -> Result<String> {
    let value = serde_json::to_value(event_type)?;
    value
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("event type did not serialize to a string"))
}

fn ts(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn parse_ts(value: String) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(&value)?.with_timezone(&Utc))
}

fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4().simple())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sessionbus_core::{
        AdapterCapability, AdapterProtocol, ArtifactKind, CreateArtifactRequest,
        CreateDecisionRequest, CreateSessionRequest, WorkspaceInfo,
    };

    #[tokio::test]
    async fn stores_sessions_artifacts_decisions_and_redacted_packs() {
        let store = SessionbusStore::in_memory().await.unwrap();
        let session = store
            .create_session(CreateSessionRequest {
                title: "Fix flaky deploy".to_string(),
                workspace: Some(WorkspaceInfo {
                    root: "/tmp/repo".to_string(),
                    git_remote: None,
                    git_branch: Some("main".to_string()),
                    head: Some("abc123".to_string()),
                }),
                summary: Some("Track staging-only deploy failure.".to_string()),
            })
            .await
            .unwrap();
        store
            .add_artifact(
                &session.id,
                CreateArtifactRequest {
                    kind: ArtifactKind::TerminalOutput,
                    title: Some("deploy output".to_string()),
                    uri: None,
                    body: Some("TOKEN=super-secret\nstaging failed".to_string()),
                    metadata: json!({"exit_code": 1}),
                    snapshot: true,
                },
            )
            .await
            .unwrap();
        store
            .add_decision(
                &session.id,
                CreateDecisionRequest {
                    text: "Start from staging config.".to_string(),
                    rationale: None,
                },
            )
            .await
            .unwrap();

        let pack = store
            .pack_session(&session.id, PackProfile::ChatGpt)
            .await
            .unwrap();

        assert!(pack.markdown.contains("Start from staging config."));
        assert!(pack.markdown.contains("TOKEN=[REDACTED]"));
        assert!(!pack.markdown.contains("super-secret"));
        assert_eq!(store.list_events(Some(&session.id)).await.unwrap().len(), 4);
    }

    #[tokio::test]
    async fn adapter_registration_upserts_descriptor() {
        let store = SessionbusStore::in_memory().await.unwrap();
        let registration = store
            .register_adapter(CapabilityDescriptor {
                adapter_id: "terminal".to_string(),
                protocol: AdapterProtocol::NativeHttp,
                version: "0.1.0".to_string(),
                capabilities: vec![
                    AdapterCapability::WriteArtifact,
                    AdapterCapability::StreamUpdates,
                ],
                metadata: json!({"kind": "example"}),
            })
            .await
            .unwrap();

        assert_eq!(registration.descriptor.adapter_id, "terminal");
        let fetched = store.get_adapter("terminal").await.unwrap().unwrap();
        assert_eq!(fetched.descriptor.capabilities.len(), 2);
    }
}
