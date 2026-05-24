use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use reqwest::StatusCode;
use serde_json::json;
use sessionbus_core::{
    Artifact, ArtifactKind, ContextPack, CreateArtifactRequest, CreateDecisionRequest,
    CreateSessionRequest, Decision, PackProfile, RedactionPolicy, Session, WorkspaceInfo,
};
use sessionbus_daemon::{default_db_path, serve};
use sessionbus_store::SessionbusStore;
use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Parser)]
#[command(name = "aictx")]
#[command(about = "Portable engineering task context for fragmented AI workflows")]
struct Cli {
    #[arg(long, env = "SESSIONBUS_URL", default_value = "http://127.0.0.1:8765")]
    api: String,
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    Daemon {
        #[arg(long, default_value = "127.0.0.1:8765")]
        bind: SocketAddr,
        #[arg(long, env = "SESSIONBUS_DB")]
        db: Option<PathBuf>,
    },
    Status,
    Start {
        title: String,
        #[arg(long)]
        summary: Option<String>,
    },
    List,
    Show {
        #[arg(long)]
        session: Option<String>,
    },
    AddFile {
        path: PathBuf,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, default_value_t = true)]
        snapshot: bool,
    },
    Note {
        text: String,
        #[arg(long)]
        session: Option<String>,
    },
    Decision {
        text: String,
        #[arg(long)]
        rationale: Option<String>,
        #[arg(long)]
        session: Option<String>,
    },
    Pack {
        #[arg(long = "for", default_value = "generic")]
        profile: String,
        #[arg(long)]
        session: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
        format: OutputFormat,
    },
    Export {
        #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
        format: OutputFormat,
        #[arg(long = "for", default_value = "generic")]
        profile: String,
        #[arg(long)]
        session: Option<String>,
    },
    Resume {
        #[arg(long)]
        target: String,
        #[arg(long)]
        session: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        CommandKind::Daemon { bind, db } => {
            tracing_subscriber::fmt::init();
            let store = SessionbusStore::open_path(db.unwrap_or_else(default_db_path)).await?;
            serve(bind, store).await
        }
        other => {
            let client = ApiClient::new(cli.api)?;
            run_command(client, other).await
        }
    }
}

async fn run_command(client: ApiClient, command: CommandKind) -> Result<()> {
    match command {
        CommandKind::Daemon { .. } => unreachable!("daemon handled before client dispatch"),
        CommandKind::Status => {
            let health = client.health().await?;
            println!("api\t{}", client.base);
            println!(
                "service\t{}",
                health
                    .get("service")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
            );
            println!(
                "ok\t{}",
                health
                    .get("ok")
                    .and_then(|value| value.as_bool())
                    .unwrap_or(false)
            );
            Ok(())
        }
        CommandKind::Start { title, summary } => {
            let session = client
                .create_session(CreateSessionRequest {
                    title,
                    workspace: Some(detect_workspace()?),
                    summary,
                })
                .await?;
            write_active_session(&session.id)?;
            println!("{}", session.id);
            Ok(())
        }
        CommandKind::List => {
            for session in client.list_sessions().await? {
                println!("{}\t{}\t{}", session.id, session.status, session.title);
            }
            Ok(())
        }
        CommandKind::Show { session } => {
            let session_id = resolve_session(session)?;
            let session = client.get_session(&session_id).await?;
            let artifacts = client.list_artifacts(&session_id).await?;
            let decisions = client.list_decisions(&session_id).await?;
            print_session_show(&session, &artifacts, &decisions);
            Ok(())
        }
        CommandKind::AddFile {
            path,
            session,
            snapshot,
        } => {
            let session_id = resolve_session(session)?;
            let body = if snapshot {
                Some(
                    fs::read_to_string(&path)
                        .with_context(|| format!("read {}", path.display()))?,
                )
            } else {
                None
            };
            let artifact = client
                .add_artifact(
                    &session_id,
                    CreateArtifactRequest {
                        kind: ArtifactKind::File,
                        title: path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string()),
                        uri: Some(format!("file://{}", absolutize(&path)?.display())),
                        body,
                        metadata: json!({
                            "path": path.display().to_string(),
                            "snapshot": snapshot
                        }),
                        snapshot,
                    },
                )
                .await?;
            println!("{}", artifact.id);
            Ok(())
        }
        CommandKind::Note { text, session } => {
            let session_id = resolve_session(session)?;
            let artifact = client
                .add_artifact(
                    &session_id,
                    CreateArtifactRequest {
                        kind: ArtifactKind::Note,
                        title: Some("note".to_string()),
                        uri: None,
                        body: Some(text),
                        metadata: json!({}),
                        snapshot: true,
                    },
                )
                .await?;
            println!("{}", artifact.id);
            Ok(())
        }
        CommandKind::Decision {
            text,
            rationale,
            session,
        } => {
            let session_id = resolve_session(session)?;
            let decision = client
                .add_decision(&session_id, CreateDecisionRequest { text, rationale })
                .await?;
            println!("{}", decision.id);
            Ok(())
        }
        CommandKind::Pack {
            profile,
            session,
            format,
        }
        | CommandKind::Export {
            profile,
            session,
            format,
        } => {
            let session_id = resolve_session(session)?;
            let profile = parse_profile(&profile)?;
            let pack = client.pack(&session_id, profile).await?;
            print_pack(pack, format)
        }
        CommandKind::Resume { target, session } => {
            let session_id = resolve_session(session)?;
            let profile = parse_profile(&target).unwrap_or(PackProfile::Generic);
            let pack = client.pack(&session_id, profile).await?;
            println!(
                "# Resume target: {}\n\n{}",
                target.trim(),
                pack.markdown.trim_end()
            );
            Ok(())
        }
    }
}

#[derive(Clone)]
struct ApiClient {
    base: String,
    http: reqwest::Client,
}

impl ApiClient {
    fn new(base: String) -> Result<Self> {
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        })
    }

    async fn health(&self) -> Result<serde_json::Value> {
        let response = self
            .http
            .get(format!("{}/healthz", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    async fn create_session(&self, request: CreateSessionRequest) -> Result<Session> {
        self.post("/sessions", &request).await
    }

    async fn list_sessions(&self) -> Result<Vec<Session>> {
        let response = self
            .http
            .get(format!("{}/sessions", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    async fn get_session(&self, session_id: &str) -> Result<Session> {
        let response = self
            .http
            .get(format!("{}/sessions/{session_id}", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    async fn list_artifacts(&self, session_id: &str) -> Result<Vec<Artifact>> {
        let response = self
            .http
            .get(format!("{}/sessions/{session_id}/artifacts", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    async fn list_decisions(&self, session_id: &str) -> Result<Vec<Decision>> {
        let response = self
            .http
            .get(format!("{}/sessions/{session_id}/decisions", self.base))
            .send()
            .await?;
        decode_response(response).await
    }

    async fn add_artifact(
        &self,
        session_id: &str,
        request: CreateArtifactRequest,
    ) -> Result<sessionbus_core::Artifact> {
        self.post(&format!("/sessions/{session_id}/artifacts"), &request)
            .await
    }

    async fn add_decision(
        &self,
        session_id: &str,
        request: CreateDecisionRequest,
    ) -> Result<sessionbus_core::Decision> {
        self.post(&format!("/sessions/{session_id}/decisions"), &request)
            .await
    }

    async fn pack(&self, session_id: &str, profile: PackProfile) -> Result<ContextPack> {
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

fn parse_profile(value: &str) -> Result<PackProfile> {
    value.parse().map_err(anyhow::Error::msg)
}

fn print_pack(pack: ContextPack, format: OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Markdown => println!("{}", pack.markdown.trim_end()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&pack.json)?),
    }
    Ok(())
}

fn print_session_show(session: &Session, artifacts: &[Artifact], decisions: &[Decision]) {
    let redaction = RedactionPolicy::default();
    println!("# {}", session.title);
    println!("id\t{}", session.id);
    println!("status\t{}", session.status);
    if let Some(summary) = &session.summary {
        println!("summary\t{}", summary);
    }
    if let Some(workspace) = &session.workspace {
        println!("workspace\t{}", workspace.root);
    }

    println!("\n## Decisions");
    if decisions.is_empty() {
        println!("- none");
    } else {
        for decision in decisions {
            println!("- {}", decision.text);
            if let Some(rationale) = &decision.rationale {
                println!("  rationale: {}", rationale);
            }
        }
    }

    println!("\n## Artifacts");
    if artifacts.is_empty() {
        println!("- none");
    } else {
        for artifact in artifacts {
            let title = artifact.title.as_deref().unwrap_or("untitled");
            println!("- {}: {} ({})", artifact.kind, title, artifact.id);
            if let Some(uri) = &artifact.uri {
                println!("  uri: {}", uri);
            }
            if let Some(body) = &artifact.body {
                for line in redaction.redact(body).lines().take(8) {
                    println!("  {}", line);
                }
            }
        }
    }
}

fn detect_workspace() -> Result<WorkspaceInfo> {
    let root = git_output(["rev-parse", "--show-toplevel"])?
        .unwrap_or(std::env::current_dir()?.display().to_string());
    Ok(WorkspaceInfo {
        root,
        git_remote: git_output(["config", "--get", "remote.origin.url"])?,
        git_branch: git_output(["branch", "--show-current"])?,
        head: git_output(["rev-parse", "--short", "HEAD"])?,
    })
}

fn git_output<const N: usize>(args: [&str; N]) -> Result<Option<String>> {
    let output = Command::new("git").args(args).output();
    let Ok(output) = output else {
        return Ok(None);
    };
    if !output.status.success() {
        return Ok(None);
    }
    let value = String::from_utf8(output.stdout)?.trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn active_session_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("sessionbus")
            .join("current-session")
    } else {
        PathBuf::from(".sessionbus").join("current-session")
    }
}

fn write_active_session(session_id: &str) -> Result<()> {
    let path = active_session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, session_id)?;
    Ok(())
}

fn resolve_session(session: Option<String>) -> Result<String> {
    if let Some(session) = session {
        return Ok(session);
    }
    let path = active_session_path();
    fs::read_to_string(&path)
        .map(|value| value.trim().to_string())
        .with_context(|| {
            format!(
                "no active session found at {}; run `aictx start \"...\"` or pass --session",
                path.display()
            )
        })
}

fn absolutize(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}
