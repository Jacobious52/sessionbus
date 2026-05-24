use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fmt, str::FromStr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Active,
    Blocked,
    Done,
    Archived,
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Archived => "archived",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceInfo {
    pub root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_remote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub status: SessionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateSessionRequest {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<WorkspaceInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    File,
    GitDiff,
    TerminalOutput,
    StackTrace,
    Url,
    ChatSnippet,
    ToolInvocation,
    TestResult,
    Note,
    Ticket,
}

impl fmt::Display for ArtifactKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::File => "file",
            Self::GitDiff => "git_diff",
            Self::TerminalOutput => "terminal_output",
            Self::StackTrace => "stack_trace",
            Self::Url => "url",
            Self::ChatSnippet => "chat_snippet",
            Self::ToolInvocation => "tool_invocation",
            Self::TestResult => "test_result",
            Self::Note => "note",
            Self::Ticket => "ticket",
        };
        f.write_str(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    pub id: String,
    pub session_id: String,
    pub kind: ArtifactKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateArtifactRequest {
    pub kind: ArtifactKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub snapshot: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Decision {
    pub id: String,
    pub session_id: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateDecisionRequest {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum EventType {
    #[serde(rename = "session.created")]
    SessionCreated,
    #[serde(rename = "session.updated")]
    SessionUpdated,
    #[serde(rename = "artifact.added")]
    ArtifactAdded,
    #[serde(rename = "decision.recorded")]
    DecisionRecorded,
    #[serde(rename = "context.packed")]
    ContextPacked,
    #[serde(rename = "adapter.registered")]
    AdapterRegistered,
    #[serde(rename = "adapter.capability_changed")]
    AdapterCapabilityChanged,
    #[serde(rename = "session.exported")]
    SessionExported,
    #[serde(rename = "session.imported")]
    SessionImported,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BusEvent {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(rename = "type")]
    pub event_type: EventType,
    pub source: String,
    pub payload: Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AdapterProtocol {
    NativeHttp,
    Acp,
    Mcp,
    Stdio,
    Filesystem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AdapterCapability {
    ImportContext,
    ExportContext,
    StreamUpdates,
    ReadWorkspace,
    WriteArtifact,
    ToolCalls,
    SessionResume,
    SessionObserve,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CapabilityDescriptor {
    pub adapter_id: String,
    pub protocol: AdapterProtocol,
    pub version: String,
    pub capabilities: Vec<AdapterCapability>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterRegistration {
    pub descriptor: CapabilityDescriptor,
    pub registered_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum PackProfile {
    #[serde(rename = "chatgpt")]
    ChatGpt,
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "cursor")]
    Cursor,
    #[serde(rename = "acp")]
    Acp,
    #[serde(rename = "generic")]
    Generic,
}

impl PackProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChatGpt => "chatgpt",
            Self::Claude => "claude",
            Self::Cursor => "cursor",
            Self::Acp => "acp",
            Self::Generic => "generic",
        }
    }
}

impl fmt::Display for PackProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PackProfile {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "chatgpt" | "chat-gpt" => Ok(Self::ChatGpt),
            "claude" => Ok(Self::Claude),
            "cursor" => Ok(Self::Cursor),
            "acp" => Ok(Self::Acp),
            "generic" | "markdown" => Ok(Self::Generic),
            other => Err(format!("unknown pack profile: {other}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ContextPack {
    pub session_id: String,
    pub profile: PackProfile,
    pub markdown: String,
    pub json: Value,
    pub created_at: DateTime<Utc>,
}
