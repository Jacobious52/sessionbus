use anyhow::{anyhow, Result};
use reqwest::StatusCode;
use serde_json::json;
use sessionbus_core::{
    Artifact, BusEvent, ContextPack, CreateArtifactRequest, CreateDecisionRequest,
    CreateSessionRequest, Decision, PackProfile, Session, SessionStatus,
    UpdateSessionStatusRequest,
};

#[derive(Clone)]
pub(crate) struct ApiClient {
    pub(crate) base: String,
    http: reqwest::Client,
}

impl ApiClient {
    pub(crate) fn new(base: String) -> Result<Self> {
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        })
    }

    pub(crate) async fn health(&self) -> Result<serde_json::Value> {
        let response = self
            .http
            .get(format!("{}/healthz", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    pub(crate) async fn create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        self.post("/sessions", &request).await
    }

    pub(crate) async fn list_sessions(&self) -> Result<Vec<Session>> {
        let response = self
            .http
            .get(format!("{}/sessions", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    pub(crate) async fn get_session(&self, session_id: &str) -> Result<Session> {
        let response = self
            .http
            .get(format!("{}/sessions/{session_id}", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    pub(crate) async fn list_artifacts(&self, session_id: &str) -> Result<Vec<Artifact>> {
        let response = self
            .http
            .get(format!("{}/sessions/{session_id}/artifacts", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    pub(crate) async fn list_decisions(&self, session_id: &str) -> Result<Vec<Decision>> {
        let response = self
            .http
            .get(format!("{}/sessions/{session_id}/decisions", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    pub(crate) async fn list_events(&self, session_id: Option<&str>) -> Result<Vec<BusEvent>> {
        let mut url = format!("{}/events", self.base);
        if let Some(session_id) = session_id {
            url.push_str("?session_id=");
            url.push_str(session_id);
        }
        let response = self.http.get(url).send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!("daemon returned {status}: {text}"));
        }
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(anyhow::Error::from))
            .collect()
    }

    pub(crate) async fn list_adapters(&self) -> Result<Vec<serde_json::Value>> {
        let response = self
            .http
            .get(format!("{}/adapters", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    pub(crate) async fn update_session_status(
        &self,
        session_id: &str,
        status: SessionStatus,
    ) -> Result<Session> {
        self.post(
            &format!("/sessions/{session_id}/status"),
            &UpdateSessionStatusRequest { status },
        )
        .await
    }

    pub(crate) async fn add_artifact(
        &self,
        session_id: &str,
        request: CreateArtifactRequest,
    ) -> Result<Artifact> {
        self.post(&format!("/sessions/{session_id}/artifacts"), &request)
            .await
    }

    pub(crate) async fn add_decision(
        &self,
        session_id: &str,
        request: CreateDecisionRequest,
    ) -> Result<Decision> {
        self.post(&format!("/sessions/{session_id}/decisions"), &request)
            .await
    }

    pub(crate) async fn pack(&self, session_id: &str, profile: PackProfile) -> Result<ContextPack> {
        self.post(
            &format!("/sessions/{session_id}/pack"),
            &json!({ "profile": profile }),
        )
        .await
    }

    async fn post<T, R>(&self, path: &str, request: &T) -> Result<R>
    where
        T: serde::Serialize + ?Sized,
        R: serde::de::DeserializeOwned,
    {
        let response = self
            .http
            .post(format!("{}{}", self.base, path))
            .json(request)
            .send()
            .await?;
        decode_response(response).await
    }
}

async fn decode_response<T>(response: reqwest::Response) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let status = response.status();
    if status == StatusCode::NO_CONTENT {
        return Err(anyhow!("empty response"));
    }
    let text = response.text().await?;
    if status.is_success() {
        Ok(serde_json::from_str(&text)?)
    } else {
        Err(anyhow!("daemon returned {status}: {text}"))
    }
}
