use anyhow::{anyhow, Context, Result};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{generate, Shell as CompletionShell};
use serde_json::{json, Value};
use sessionbus_core::{
    AdapterCapability, AdapterProtocol, Artifact, ArtifactKind, CapabilityDescriptor, ContextPack,
    CreateArtifactRequest, CreateDecisionRequest, CreateSessionRequest, Decision, PackProfile,
    RedactionPolicy, Session, SessionStatus, WorkspaceInfo,
};
use sessionbus_daemon::{default_db_path, serve};
use sessionbus_store::SessionbusStore;
use std::{
    fs,
    io::{self, BufRead, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

mod client;
mod workspace;

use client::ApiClient;
use workspace::{
    absolutize, clear_active_session_if_matches, clear_workspace_session_if_matches,
    detect_workspace, git_output, print_workspace, resolve_session, workspace_session_path,
    workspace_text, workspace_watch_artifact, write_active_session, write_workspace_session,
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
    Doctor,
    Completions {
        #[arg(value_enum)]
        shell: CompletionShell,
    },
    Setup {
        #[arg(long)]
        write: bool,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        rc: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = ShellKind::Zsh)]
        shell: ShellKind,
        #[arg(long)]
        auto_capture: bool,
        #[arg(long)]
        open_dashboard: bool,
        #[arg(long)]
        skip_codex: bool,
        #[arg(long)]
        skip_shell: bool,
        #[arg(long)]
        skip_adapters: bool,
    },
    Start {
        #[arg(long)]
        repo: bool,
        title: String,
        #[arg(long)]
        summary: Option<String>,
    },
    ShellInit {
        #[arg(value_enum)]
        shell: ShellKind,
        #[arg(long)]
        auto_capture: bool,
    },
    Install {
        #[arg(value_enum)]
        target: InstallTarget,
        #[arg(long)]
        write: bool,
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        rc: Option<PathBuf>,
        #[arg(long, value_enum, default_value_t = ShellKind::Zsh)]
        shell: ShellKind,
        #[arg(long)]
        auto_capture: bool,
    },
    Policy {
        #[command(subcommand)]
        command: PolicyCommand,
    },
    Redact {
        #[command(subcommand)]
        command: RedactCommand,
    },
    Session {
        #[command(subcommand)]
        command: SessionCommand,
    },
    List {
        #[arg(long)]
        active: bool,
    },
    Sessions {
        #[arg(long)]
        active: bool,
    },
    Current,
    Use {
        session: String,
    },
    Switch {
        session: String,
    },
    Close {
        #[arg(long)]
        session: Option<String>,
    },
    Show {
        #[arg(long)]
        session: Option<String>,
    },
    Run {
        #[arg(long)]
        session: Option<String>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    Capture {
        #[arg(long)]
        session: Option<String>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    ObserveCommand {
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        exit_code: Option<i32>,
        #[arg(long)]
        duration_ms: Option<u128>,
        #[arg(long)]
        shell: Option<String>,
        #[arg(last = true, required = true)]
        command: Vec<String>,
    },
    Workspace,
    Watch {
        #[arg(long, default_value = ".")]
        workspace: PathBuf,
        #[arg(long)]
        once: bool,
        #[arg(long, default_value_t = 2_000)]
        interval_ms: u64,
        #[arg(long)]
        session: Option<String>,
    },
    Dogfood {
        #[arg(long = "for", default_value = "generic")]
        profile: String,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        note: Option<String>,
        #[arg(long)]
        preview: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Markdown)]
        format: OutputFormat,
    },
    Mcp {
        #[arg(long)]
        ensure_daemon: bool,
        #[arg(long)]
        no_ensure_daemon: bool,
    },
    AddDiff {
        #[arg(long)]
        session: Option<String>,
    },
    AddCommit {
        #[arg(default_value = "HEAD")]
        rev: String,
        #[arg(long)]
        session: Option<String>,
    },
    Import {
        path: PathBuf,
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
    Message {
        #[command(subcommand)]
        command: MessageCommand,
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
        #[arg(long)]
        preview: bool,
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
    Dashboard {
        #[arg(long)]
        print_url: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Markdown,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ShellKind {
    Bash,
    Zsh,
    Fish,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum InstallTarget {
    Codex,
    Shell,
}

#[derive(Debug, Subcommand)]
enum PolicyCommand {
    Init,
}

#[derive(Debug, Subcommand)]
enum RedactCommand {
    Test { text: String },
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    Doctor,
    Bind {
        #[arg(long)]
        repo: bool,
        #[arg(long)]
        session: Option<String>,
    },
    Unbind,
    Suggest,
}

#[derive(Debug, Subcommand)]
enum MessageCommand {
    Add {
        text: String,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        requires_response: bool,
        #[arg(long)]
        session: Option<String>,
    },
    List {
        #[arg(long, default_value = "open")]
        status: String,
        #[arg(long)]
        session: Option<String>,
    },
    Ack {
        artifact_id: String,
        #[arg(long)]
        session: Option<String>,
    },
    Resolve {
        artifact_id: String,
        #[arg(long)]
        session: Option<String>,
    },
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
        CommandKind::Completions { shell } => {
            let mut command = Cli::command();
            let mut output = Vec::new();
            generate(shell, &mut command, "aictx", &mut output);
            if let Err(error) = io::stdout().write_all(&output) {
                if error.kind() != io::ErrorKind::BrokenPipe {
                    return Err(error.into());
                }
            }
            Ok(())
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
        CommandKind::Completions { .. } => {
            unreachable!("completions handled before client dispatch")
        }
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
        CommandKind::Doctor => run_doctor(&client).await,
        CommandKind::Setup {
            write,
            config,
            rc,
            shell,
            auto_capture,
            open_dashboard: should_open_dashboard,
            skip_codex,
            skip_shell,
            skip_adapters,
        } => {
            run_setup(
                &client,
                SetupOptions {
                    write,
                    config,
                    rc,
                    shell,
                    auto_capture,
                    open_dashboard: should_open_dashboard,
                    skip_codex,
                    skip_shell,
                    skip_adapters,
                },
            )
            .await
        }
        CommandKind::Start {
            repo: _repo,
            title,
            summary,
        } => {
            let workspace = detect_workspace()?;
            let session = client
                .create_session(CreateSessionRequest {
                    title,
                    workspace: Some(workspace.clone()),
                    summary,
                })
                .await?;
            write_active_session(&session.id)?;
            write_workspace_session(&workspace.root, &session.id)?;
            println!("{}", session.id);
            Ok(())
        }
        CommandKind::ShellInit {
            shell,
            auto_capture,
        } => {
            print_shell_init(shell, auto_capture);
            Ok(())
        }
        CommandKind::Install {
            target,
            write,
            config,
            rc,
            shell,
            auto_capture,
        } => {
            print_install(target, write, config, rc, shell, auto_capture)?;
            Ok(())
        }
        CommandKind::Policy { command } => run_policy_command(command),
        CommandKind::Redact { command } => run_redact_command(command),
        CommandKind::Session { command } => run_session_command(&client, command).await,
        CommandKind::List { active } => print_session_list(&client, active).await,
        CommandKind::Sessions { active } => print_session_list(&client, active).await,
        CommandKind::Current => {
            println!("{}", resolve_session(None)?);
            Ok(())
        }
        CommandKind::Use { session } | CommandKind::Switch { session } => {
            let session = client.get_session(&session).await?;
            write_active_session(&session.id)?;
            if let Some(workspace) = &session.workspace {
                write_workspace_session(&workspace.root, &session.id)?;
            }
            println!("{}", session.id);
            Ok(())
        }
        CommandKind::Close { session } => {
            let session_id = resolve_session(session)?;
            let session = client
                .update_session_status(&session_id, SessionStatus::Done)
                .await?;
            clear_active_session_if_matches(&session.id)?;
            if let Some(workspace) = &session.workspace {
                clear_workspace_session_if_matches(&workspace.root, &session.id)?;
            }
            println!("{}\t{}", session.id, session.status);
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
        CommandKind::Run { session, command } => {
            let session_id = resolve_session(session)?;
            run_and_capture(&client, &session_id, &command).await
        }
        CommandKind::Capture { session, command } => {
            let session_id = resolve_session(session)?;
            run_and_capture(&client, &session_id, &command).await
        }
        CommandKind::ObserveCommand {
            session,
            exit_code,
            duration_ms,
            shell,
            command,
        } => {
            let session_id = resolve_session(session)?;
            observe_command(
                &client,
                &session_id,
                &command,
                exit_code,
                duration_ms,
                shell,
            )
            .await
        }
        CommandKind::Workspace => {
            print_workspace()?;
            Ok(())
        }
        CommandKind::Watch {
            workspace,
            once,
            interval_ms,
            session,
        } => {
            let session_id = resolve_session(session)?;
            watch_workspace(&client, &session_id, &workspace, once, interval_ms).await
        }
        CommandKind::Dogfood {
            profile,
            session,
            note,
            preview,
            format,
        } => {
            let session_id = resolve_session(session)?;
            let profile = parse_profile(&profile)?;
            dogfood_handoff(&client, &session_id, profile, note, preview, format).await
        }
        CommandKind::Mcp {
            ensure_daemon: _ensure_daemon,
            no_ensure_daemon,
        } => {
            let _daemon = if no_ensure_daemon {
                None
            } else {
                ensure_daemon(&client).await?
            };
            run_mcp_server(client).await
        }
        CommandKind::AddDiff { session } => {
            let session_id = resolve_session(session)?;
            let artifact = client
                .add_artifact(&session_id, git_diff_artifact()?)
                .await?;
            println!("{}", artifact.id);
            Ok(())
        }
        CommandKind::AddCommit { rev, session } => {
            let session_id = resolve_session(session)?;
            let artifact = client
                .add_artifact(&session_id, git_commit_artifact(&rev)?)
                .await?;
            println!("{}", artifact.id);
            Ok(())
        }
        CommandKind::Import { path } => {
            let session = import_pack(&client, &path).await?;
            write_active_session(&session.id)?;
            if let Some(workspace) = &session.workspace {
                write_workspace_session(&workspace.root, &session.id)?;
            }
            println!("{}", session.id);
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
        CommandKind::Message { command } => run_message_command(&client, command).await,
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
            preview,
            format,
        } => {
            let session_id = resolve_session(session)?;
            let profile = parse_profile(&profile)?;
            let mut pack = client.pack(&session_id, profile).await?;
            apply_local_redaction(&mut pack)?;
            print_pack(pack, format, preview)
        }
        CommandKind::Export {
            profile,
            session,
            format,
        } => {
            let session_id = resolve_session(session)?;
            let profile = parse_profile(&profile)?;
            let mut pack = client.pack(&session_id, profile).await?;
            apply_local_redaction(&mut pack)?;
            print_pack(pack, format, false)
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
        CommandKind::Dashboard { print_url } => {
            let url = format!("{}/dashboard", client.base);
            if print_url {
                println!("{}", url);
            } else {
                open_dashboard(&url)?;
            }
            Ok(())
        }
    }
}

async fn print_session_list(client: &ApiClient, active: bool) -> Result<()> {
    for session in client.list_sessions().await? {
        if active && session.status != SessionStatus::Active {
            continue;
        }
        println!("{}\t{}\t{}", session.id, session.status, session.title);
    }
    Ok(())
}

async fn run_doctor(client: &ApiClient) -> Result<()> {
    match client.health().await {
        Ok(_) => println!("daemon\tok\t{}", client.base),
        Err(error) => println!("daemon\terror\t{}", error),
    }
    match detect_workspace() {
        Ok(workspace) => {
            println!("workspace\t{}", workspace.root);
            if let Some(branch) = workspace.git_branch {
                println!("branch\t{}", branch);
            }
            if let Some(head) = workspace.head {
                println!("head\t{}", head);
            }
        }
        Err(error) => println!("workspace\terror\t{}", error),
    }
    match resolve_session(None) {
        Ok(session_id) => println!("session\t{}", session_id),
        Err(_) => println!("session\tnone"),
    }
    match client.list_adapters().await {
        Ok(adapters) if adapters.is_empty() => println!("adapters\tnone"),
        Ok(adapters) => {
            println!("adapters\t{}", adapters.len());
            for adapter in adapters {
                let descriptor = adapter.get("descriptor").unwrap_or(&Value::Null);
                let id = descriptor
                    .get("adapter_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let status = adapter
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let capabilities = descriptor
                    .get("capabilities")
                    .and_then(Value::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>()
                            .join(",")
                    })
                    .unwrap_or_default();
                println!("adapter\t{}\t{}\t{}", id, status, capabilities);
            }
        }
        Err(error) => println!("adapters\terror\t{}", error),
    }
    Ok(())
}

struct SetupOptions {
    write: bool,
    config: Option<PathBuf>,
    rc: Option<PathBuf>,
    shell: ShellKind,
    auto_capture: bool,
    open_dashboard: bool,
    skip_codex: bool,
    skip_shell: bool,
    skip_adapters: bool,
}

async fn run_setup(client: &ApiClient, options: SetupOptions) -> Result<()> {
    let started = ensure_daemon_running(client).await?;
    println!(
        "daemon\t{}\t{}",
        if started { "started" } else { "ok" },
        client.base
    );

    if options.write && !options.skip_codex {
        let exe = std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "aictx".to_string());
        let path = options.config.unwrap_or_else(default_codex_config_path);
        write_codex_config(&path, &codex_mcp_block(&exe))?;
        println!("codex\tinstalled\t{}", path.display());
    } else if options.skip_codex {
        println!("codex\tskipped");
    } else {
        println!("codex\tpreview\tuse --write to install MCP config");
    }

    if options.write && !options.skip_shell {
        let path = options
            .rc
            .unwrap_or_else(|| default_shell_rc_path(options.shell));
        write_shell_config(&path, options.shell, options.auto_capture)?;
        println!("shell\tinstalled\t{}", path.display());
    } else if options.skip_shell {
        println!("shell\tskipped");
    } else {
        println!("shell\tpreview\tuse --write to install shell helpers");
    }

    if !options.skip_adapters {
        for descriptor in bundled_adapter_descriptors() {
            let adapter_id = descriptor.adapter_id.clone();
            client.register_adapter(descriptor).await?;
            println!("adapter\tregistered\t{}", adapter_id);
        }
    } else {
        println!("adapters\tskipped");
    }

    let url = format!("{}/dashboard", client.base);
    println!("dashboard\t{}", url);
    if options.open_dashboard {
        open_dashboard(&url)?;
    }
    Ok(())
}

fn bundled_adapter_descriptors() -> Vec<CapabilityDescriptor> {
    vec![
        CapabilityDescriptor {
            adapter_id: "sessionbus.terminal".to_string(),
            protocol: AdapterProtocol::NativeHttp,
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![
                AdapterCapability::WriteArtifact,
                AdapterCapability::StreamUpdates,
                AdapterCapability::SessionObserve,
            ],
            metadata: json!({
                "runtime": "bun",
                "bundled": true,
                "path": "adapters/terminal"
            }),
        },
        CapabilityDescriptor {
            adapter_id: "sessionbus.filesystem".to_string(),
            protocol: AdapterProtocol::Filesystem,
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![
                AdapterCapability::ReadWorkspace,
                AdapterCapability::WriteArtifact,
            ],
            metadata: json!({
                "runtime": "bun",
                "bundled": true,
                "path": "adapters/filesystem"
            }),
        },
    ]
}

fn parse_profile(value: &str) -> Result<PackProfile> {
    value.parse().map_err(anyhow::Error::msg)
}

fn print_pack(pack: ContextPack, format: OutputFormat, preview: bool) -> Result<()> {
    if preview {
        println!("# Preview\n");
    }
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

fn print_shell_init(shell: ShellKind, auto_capture: bool) {
    match shell {
        ShellKind::Bash | ShellKind::Zsh => {
            println!(
                r#"# Sessionbus shell helpers
aictx-capture() {{
  command aictx capture -- "$@"
}}

aictx-watch-once() {{
  command aictx watch --once --workspace "${{1:-$PWD}}"
}}

aictx-pack-copy() {{
  command aictx pack --for "${{1:-generic}}" | pbcopy
}}
"#
            );
            if auto_capture {
                match shell {
                    ShellKind::Zsh => print_zsh_auto_capture_hook(),
                    ShellKind::Bash => print_bash_auto_capture_hook(),
                    ShellKind::Fish => unreachable!("fish handled separately"),
                }
            }
        }
        ShellKind::Fish => {
            println!(
                r#"# Sessionbus shell helpers
function aictx-capture
  command aictx capture -- $argv
end

function aictx-watch-once
  set workspace (pwd)
  if test (count $argv) -gt 0
    set workspace $argv[1]
  end
  command aictx watch --once --workspace $workspace
end

function aictx-pack-copy
  set profile generic
  if test (count $argv) -gt 0
    set profile $argv[1]
  end
  command aictx pack --for $profile | pbcopy
end
"#
            );
            if auto_capture {
                print_fish_auto_capture_hook();
            }
        }
    }
}

fn print_zsh_auto_capture_hook() {
    println!(
        r#"# Sessionbus passive command observation
autoload -Uz add-zsh-hook
__aictx_preexec() {{
  export __AICTX_LAST_COMMAND="$1"
  export __AICTX_LAST_STARTED_AT="$(date +%s%3N 2>/dev/null || date +%s)"
}}
__aictx_precmd() {{
  local status="$?"
  if [[ -n "${{__AICTX_LAST_COMMAND:-}}" ]]; then
    local now="$(date +%s%3N 2>/dev/null || date +%s)"
    local duration=""
    if [[ -n "${{__AICTX_LAST_STARTED_AT:-}}" && "$now" == <-> && "$__AICTX_LAST_STARTED_AT" == <-> ]]; then
      duration=$(( now - __AICTX_LAST_STARTED_AT ))
    fi
    if [[ -n "$duration" ]]; then
      command aictx observe-command --shell zsh --exit-code "$status" --duration-ms "$duration" -- "$__AICTX_LAST_COMMAND" >/dev/null 2>&1
    else
      command aictx observe-command --shell zsh --exit-code "$status" -- "$__AICTX_LAST_COMMAND" >/dev/null 2>&1
    fi
    unset __AICTX_LAST_COMMAND __AICTX_LAST_STARTED_AT
  fi
}}
add-zsh-hook preexec __aictx_preexec
add-zsh-hook precmd __aictx_precmd
"#
    );
}

fn print_bash_auto_capture_hook() {
    println!(
        r#"# Sessionbus passive command observation
__aictx_prompt_command() {{
  local status="$?"
  local command_line
  command_line="$(history 1 | sed 's/^ *[0-9]* *//')"
  if [[ -n "$command_line" && "$command_line" != "$__AICTX_LAST_COMMAND" ]]; then
    __AICTX_LAST_COMMAND="$command_line"
    command aictx observe-command --shell bash --exit-code "$status" -- "$command_line" >/dev/null 2>&1
  fi
}}
PROMPT_COMMAND="__aictx_prompt_command${{PROMPT_COMMAND:+;$PROMPT_COMMAND}}"
"#
    );
}

fn print_fish_auto_capture_hook() {
    println!(
        r#"# Sessionbus passive command observation
function __aictx_postexec --on-event fish_postexec
  set status $status
  set command_line (string join " " $argv)
  if test -n "$command_line"
    command aictx observe-command --shell fish --exit-code $status -- "$command_line" >/dev/null 2>&1
  end
end
"#
    );
}

fn print_install(
    target: InstallTarget,
    write: bool,
    config: Option<PathBuf>,
    rc: Option<PathBuf>,
    shell: ShellKind,
    auto_capture: bool,
) -> Result<()> {
    let exe = std::env::current_exe()
        .ok()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "aictx".to_string());
    match target {
        InstallTarget::Codex => {
            let block = codex_mcp_block(&exe);
            if write {
                let path = config.unwrap_or_else(default_codex_config_path);
                write_codex_config(&path, &block)?;
                println!("installed codex MCP config\t{}", path.display());
                return Ok(());
            }
            println!("# Runs: aictx mcp");
            println!("{}", block.trim_end());
        }
        InstallTarget::Shell => {
            if write {
                let path = rc.unwrap_or_else(|| default_shell_rc_path(shell));
                write_shell_config(&path, shell, auto_capture)?;
                println!("installed shell helpers\t{}", path.display());
                return Ok(());
            }
            println!("# Add one of these to your shell rc file:");
            println!("eval \"$(aictx shell-init zsh)\"");
            println!("eval \"$(aictx shell-init bash)\"");
            println!("aictx shell-init fish | source");
            println!();
            println!("# Add --auto-capture to observe command lines and exit codes:");
            println!("eval \"$(aictx shell-init zsh --auto-capture)\"");
        }
    }
    Ok(())
}

fn codex_mcp_block(exe: &str) -> String {
    format!(
        "[mcp_servers.sessionbus]\ncommand = \"{}\"\nargs = [\"mcp\"]\nstartup_timeout_sec = 10\n",
        exe.replace('\\', "\\\\").replace('"', "\\\"")
    )
}

fn default_codex_config_path() -> PathBuf {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        return PathBuf::from(home).join("config.toml");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".codex").join("config.toml");
    }
    PathBuf::from(".codex").join("config.toml")
}

fn default_shell_rc_path(shell: ShellKind) -> PathBuf {
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    match shell {
        ShellKind::Bash => home.join(".bashrc"),
        ShellKind::Zsh => home.join(".zshrc"),
        ShellKind::Fish => home.join(".config").join("fish").join("config.fish"),
    }
}

fn write_codex_config(path: &Path, block: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();
    let trimmed = remove_toml_table(&existing, "[mcp_servers.sessionbus]");
    let mut next = trimmed.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(block.trim_end());
    next.push('\n');
    fs::write(path, next).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn remove_toml_table(contents: &str, header: &str) -> String {
    let mut output = Vec::new();
    let mut skipping = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == header {
            skipping = true;
            continue;
        }
        if skipping && trimmed.starts_with('[') && trimmed.ends_with(']') {
            skipping = false;
        }
        if !skipping {
            output.push(line);
        }
    }
    output.join("\n")
}

fn write_shell_config(path: &Path, shell: ShellKind, auto_capture: bool) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let existing = fs::read_to_string(path).unwrap_or_default();
    let mut next = remove_marked_block(&existing, "# sessionbus start", "# sessionbus end")
        .trim_end()
        .to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str("# sessionbus start\n");
    next.push_str(&shell_install_line(shell, auto_capture));
    next.push_str("\n# sessionbus end\n");
    fs::write(path, next).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn shell_install_line(shell: ShellKind, auto_capture: bool) -> String {
    let suffix = if auto_capture { " --auto-capture" } else { "" };
    match shell {
        ShellKind::Bash => format!("eval \"$(aictx shell-init bash{suffix})\""),
        ShellKind::Zsh => format!("eval \"$(aictx shell-init zsh{suffix})\""),
        ShellKind::Fish => format!("aictx shell-init fish{suffix} | source"),
    }
}

fn remove_marked_block(contents: &str, start: &str, end: &str) -> String {
    let mut output = Vec::new();
    let mut skipping = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == start {
            skipping = true;
            continue;
        }
        if skipping && trimmed == end {
            skipping = false;
            continue;
        }
        if !skipping {
            output.push(line);
        }
    }
    output.join("\n")
}

fn open_dashboard(url: &str) -> Result<()> {
    let mut command = if let Ok(override_command) = std::env::var("SESSIONBUS_OPEN_COMMAND") {
        let mut command = Command::new(override_command);
        command.arg(url);
        command
    } else if cfg!(target_os = "macos") {
        let mut command = Command::new("open");
        command.arg(url);
        command
    } else if cfg!(target_os = "windows") {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    } else {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    let output = command.output().context("open dashboard")?;
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    if !output.status.success() {
        return Err(anyhow!("dashboard opener exited with {}", output.status));
    }
    if output.stdout.is_empty() {
        println!("{}", url);
    }
    Ok(())
}

fn run_policy_command(command: PolicyCommand) -> Result<()> {
    match command {
        PolicyCommand::Init => {
            let path = policy_path()?;
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            if !path.exists() {
                fs::write(
                    &path,
                    "# Sessionbus local redaction policy\nredact_keys = [\"CLIENT_ID\"]\n",
                )?;
            }
            println!("{}", path.display());
            Ok(())
        }
    }
}

fn run_redact_command(command: RedactCommand) -> Result<()> {
    match command {
        RedactCommand::Test { text } => {
            println!("{}", redact_with_local_policy(&text)?);
            Ok(())
        }
    }
}

async fn run_session_command(client: &ApiClient, command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::Doctor => {
            let session_id = resolve_session(None)?;
            let session = client.get_session(&session_id).await?;
            println!(
                "session\t{}\t{}\t{}",
                session.id, session.status, session.title
            );
            if let Some(workspace) = session.workspace {
                println!("workspace\t{}", workspace.root);
            }
            Ok(())
        }
        SessionCommand::Bind {
            repo: _repo,
            session,
        } => {
            let session_id = resolve_session(session)?;
            let workspace = detect_workspace()?;
            write_workspace_session(&workspace.root, &session_id)?;
            println!("{}\t{}", workspace.root, session_id);
            Ok(())
        }
        SessionCommand::Unbind => {
            let workspace = detect_workspace()?;
            let path = workspace_session_path(&workspace.root);
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            println!("unbound\t{}", workspace.root);
            Ok(())
        }
        SessionCommand::Suggest => {
            let workspace = detect_workspace()?;
            let sessions = client.list_sessions().await?;
            for session in sessions {
                if session.status == SessionStatus::Active
                    && session
                        .workspace
                        .as_ref()
                        .map(|info| info.root == workspace.root)
                        .unwrap_or(false)
                {
                    println!("{}\t{}\t{}", session.id, session.status, session.title);
                    return Ok(());
                }
            }
            println!("none\t{}", workspace.root);
            Ok(())
        }
    }
}

async fn run_message_command(client: &ApiClient, command: MessageCommand) -> Result<()> {
    match command {
        MessageCommand::Add {
            text,
            to,
            topic,
            requires_response,
            session,
        } => {
            let session_id = resolve_session(session)?;
            let artifact = client
                .add_artifact(
                    &session_id,
                    coordination_message_artifact(text, to, topic, requires_response),
                )
                .await?;
            println!("{}", artifact.id);
            Ok(())
        }
        MessageCommand::List { status, session } => {
            let session_id = resolve_session(session)?;
            let artifacts = client.list_artifacts(&session_id).await?;
            let updates = message_status_updates(&artifacts);
            for artifact in artifacts {
                if artifact.metadata.get("type").and_then(Value::as_str)
                    != Some("coordination_message")
                {
                    continue;
                }
                let current_status = updates
                    .get(&artifact.id)
                    .map(String::as_str)
                    .or_else(|| artifact.metadata.get("status").and_then(Value::as_str))
                    .unwrap_or("open");
                if current_status != status {
                    continue;
                }
                let to_agent = artifact
                    .metadata
                    .get("to_agent")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let topic = artifact
                    .metadata
                    .get("topic")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                println!(
                    "{}\t{}\t{}\t{}",
                    artifact.id, current_status, to_agent, topic
                );
            }
            Ok(())
        }
        MessageCommand::Ack {
            artifact_id,
            session,
        } => add_message_update(client, session, artifact_id, "acknowledged").await,
        MessageCommand::Resolve {
            artifact_id,
            session,
        } => add_message_update(client, session, artifact_id, "resolved").await,
    }
}

fn message_status_updates(artifacts: &[Artifact]) -> std::collections::HashMap<String, String> {
    let mut updates = std::collections::HashMap::new();
    for artifact in artifacts {
        if artifact.metadata.get("type").and_then(Value::as_str)
            == Some("coordination_message_update")
        {
            if let (Some(message_id), Some(status)) = (
                artifact.metadata.get("message_id").and_then(Value::as_str),
                artifact.metadata.get("status").and_then(Value::as_str),
            ) {
                updates.insert(message_id.to_string(), status.to_string());
            }
        }
    }
    updates
}

async fn add_message_update(
    client: &ApiClient,
    session: Option<String>,
    artifact_id: String,
    status: &str,
) -> Result<()> {
    let session_id = resolve_session(session)?;
    let artifact = client
        .add_artifact(
            &session_id,
            CreateArtifactRequest {
                kind: ArtifactKind::Note,
                title: Some(format!("message {status}")),
                uri: None,
                body: Some(format!("{status} {artifact_id}")),
                metadata: json!({
                    "type": "coordination_message_update",
                    "message_id": artifact_id,
                    "status": status
                }),
                snapshot: true,
            },
        )
        .await?;
    println!("{}", artifact.id);
    Ok(())
}

fn coordination_message_artifact(
    text: String,
    to_agent: Option<String>,
    topic: Option<String>,
    requires_response: bool,
) -> CreateArtifactRequest {
    CreateArtifactRequest {
        kind: ArtifactKind::Note,
        title: Some(
            topic
                .clone()
                .unwrap_or_else(|| "coordination message".to_string()),
        ),
        uri: None,
        body: Some(text),
        metadata: json!({
            "type": "coordination_message",
            "from_agent": "human",
            "to_agent": to_agent,
            "topic": topic,
            "requires_response": requires_response,
            "status": if requires_response { "open" } else { "noted" }
        }),
        snapshot: true,
    }
}

fn policy_path() -> Result<PathBuf> {
    Ok(PathBuf::from(detect_workspace()?.root)
        .join(".sessionbus")
        .join("policy.toml"))
}

fn local_redaction_keys() -> Result<Vec<String>> {
    let path = policy_path()?;
    let Ok(body) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    let Some((_, rest)) = body.split_once("redact_keys") else {
        return Ok(Vec::new());
    };
    let Some((_, list)) = rest.split_once('[') else {
        return Ok(Vec::new());
    };
    let Some((items, _)) = list.split_once(']') else {
        return Ok(Vec::new());
    };
    Ok(items
        .split(',')
        .map(|item| item.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|item| !item.is_empty())
        .collect())
}

fn redact_with_local_policy(input: &str) -> Result<String> {
    let mut output = RedactionPolicy::default().redact(input);
    for key in local_redaction_keys()? {
        output = redact_key_fragment(&output, &key);
    }
    Ok(output)
}

fn redact_key_fragment(input: &str, key_fragment: &str) -> String {
    let key_fragment = key_fragment.to_ascii_uppercase();
    input
        .lines()
        .map(|line| {
            let Some((key, _value)) = line.split_once('=') else {
                return line.to_string();
            };
            if key.to_ascii_uppercase().contains(&key_fragment) {
                format!("{}=[REDACTED]", key.trim_end())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn apply_local_redaction(pack: &mut ContextPack) -> Result<()> {
    pack.markdown = redact_with_local_policy(&pack.markdown)?;
    pack.json = redact_json_value(pack.json.clone())?;
    Ok(())
}

fn redact_json_value(value: Value) -> Result<Value> {
    Ok(match value {
        Value::String(text) => Value::String(redact_with_local_policy(&text)?),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(redact_json_value)
                .collect::<Result<Vec<_>>>()?,
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| Ok((key, redact_json_value(value)?)))
                .collect::<Result<serde_json::Map<_, _>>>()?,
        ),
        other => other,
    })
}

async fn run_and_capture(client: &ApiClient, session_id: &str, command: &[String]) -> Result<()> {
    let started = Instant::now();
    let output = Command::new(&command[0])
        .args(&command[1..])
        .output()
        .with_context(|| format!("run command: {}", command.join(" ")))?;
    io::stdout().write_all(&output.stdout)?;
    io::stderr().write_all(&output.stderr)?;
    let duration_ms = started.elapsed().as_millis();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let joined = command.join(" ");
    let kind = if command.iter().any(|part| part.contains("test")) {
        ArtifactKind::TestResult
    } else {
        ArtifactKind::TerminalOutput
    };
    client
        .add_artifact(
            session_id,
            CreateArtifactRequest {
                kind,
                title: Some(joined.clone()),
                uri: None,
                body: Some(format!(
                    "$ {}\n\n[stdout]\n{}\n[stderr]\n{}",
                    joined, stdout, stderr
                )),
                metadata: json!({
                    "command": command,
                    "exit_code": output.status.code(),
                    "success": output.status.success(),
                    "duration_ms": duration_ms
                }),
                snapshot: true,
            },
        )
        .await?;
    if let Some(code) = output.status.code() {
        if code != 0 {
            std::process::exit(code);
        }
    } else if !output.status.success() {
        std::process::exit(1);
    }
    Ok(())
}

async fn observe_command(
    client: &ApiClient,
    session_id: &str,
    command: &[String],
    exit_code: Option<i32>,
    duration_ms: Option<u128>,
    shell: Option<String>,
) -> Result<()> {
    let joined = command.join(" ");
    let mut body = format!("$ {}", joined);
    if let Some(exit_code) = exit_code {
        body.push_str(&format!("\nexit_code\t{}", exit_code));
    }
    if let Some(duration_ms) = duration_ms {
        body.push_str(&format!("\nduration_ms\t{}", duration_ms));
    }
    if let Some(shell) = shell.as_deref() {
        body.push_str(&format!("\nshell\t{}", shell));
    }
    let artifact = client
        .add_artifact(
            session_id,
            CreateArtifactRequest {
                kind: ArtifactKind::ToolInvocation,
                title: Some(joined.clone()),
                uri: None,
                body: Some(body),
                metadata: json!({
                    "source": "shell-hook",
                    "command": command,
                    "command_line": joined,
                    "exit_code": exit_code,
                    "duration_ms": duration_ms,
                    "shell": shell
                }),
                snapshot: true,
            },
        )
        .await?;
    println!("{}", artifact.id);
    Ok(())
}

async fn watch_workspace(
    client: &ApiClient,
    session_id: &str,
    workspace: &Path,
    once: bool,
    interval_ms: u64,
) -> Result<()> {
    let workspace = absolutize(workspace)?;
    let mut last_body = None;
    loop {
        let artifact = workspace_watch_artifact(&workspace)?;
        let body = artifact.body.clone().unwrap_or_default();
        if last_body.as_deref() != Some(body.as_str()) {
            let created = client.add_artifact(session_id, artifact).await?;
            println!("{}", created.id);
            last_body = Some(body);
        }
        if once {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(interval_ms.max(250))).await;
    }
}

async fn dogfood_handoff(
    client: &ApiClient,
    session_id: &str,
    profile: PackProfile,
    note: Option<String>,
    preview: bool,
    format: OutputFormat,
) -> Result<()> {
    let result = prepare_dogfood_handoff(client, session_id, profile, note).await?;
    for artifact in &result.artifacts {
        eprintln!("artifact\t{}\t{}", artifact.label, artifact.id);
    }
    print_pack(result.pack, format, preview)
}

async fn prepare_dogfood_handoff(
    client: &ApiClient,
    session_id: &str,
    profile: PackProfile,
    note: Option<String>,
) -> Result<DogfoodHandoff> {
    let artifacts = capture_dogfood_artifacts(client, session_id, note).await?;
    let mut pack = client.pack(session_id, profile).await?;
    apply_local_redaction(&mut pack)?;
    Ok(DogfoodHandoff { artifacts, pack })
}

async fn capture_dogfood_artifacts(
    client: &ApiClient,
    session_id: &str,
    note: Option<String>,
) -> Result<Vec<DogfoodArtifact>> {
    let workspace = detect_workspace()?;
    let mut artifacts = Vec::new();
    let workspace_artifact = client
        .add_artifact(
            session_id,
            workspace_watch_artifact(Path::new(&workspace.root))?,
        )
        .await?;
    artifacts.push(DogfoodArtifact {
        label: "workspace".to_string(),
        id: workspace_artifact.id,
    });

    if git_output(["status", "--short"])?.is_some() {
        let diff_artifact = client
            .add_artifact(session_id, git_diff_artifact()?)
            .await?;
        artifacts.push(DogfoodArtifact {
            label: "git_diff".to_string(),
            id: diff_artifact.id,
        });
    } else {
        artifacts.push(DogfoodArtifact {
            label: "git_diff".to_string(),
            id: "skipped-clean-worktree".to_string(),
        });
    }

    if let Some(note) = note {
        let note_artifact = client
            .add_artifact(
                session_id,
                CreateArtifactRequest {
                    kind: ArtifactKind::Note,
                    title: Some("dogfood note".to_string()),
                    uri: None,
                    body: Some(note),
                    metadata: json!({ "source": "dogfood" }),
                    snapshot: true,
                },
            )
            .await?;
        artifacts.push(DogfoodArtifact {
            label: "note".to_string(),
            id: note_artifact.id,
        });
    }

    Ok(artifacts)
}

struct DogfoodArtifact {
    label: String,
    id: String,
}

struct DogfoodHandoff {
    artifacts: Vec<DogfoodArtifact>,
    pack: ContextPack,
}

struct DaemonGuard {
    child: Child,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

async fn ensure_daemon(client: &ApiClient) -> Result<Option<DaemonGuard>> {
    if client.health().await.is_ok() {
        return Ok(None);
    }

    let bind = api_bind_addr(&client.base)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("daemon")
        .arg("--bind")
        .arg(bind.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(db) = std::env::var("SESSIONBUS_DB") {
        command.arg("--db").arg(db);
    }
    let child = command.spawn().with_context(|| "start sessionbus daemon")?;
    let guard = DaemonGuard { child };
    let started = Instant::now();
    loop {
        if client.health().await.is_ok() {
            return Ok(Some(guard));
        }
        if started.elapsed() > Duration::from_secs(5) {
            return Err(anyhow!(
                "sessionbus daemon did not become healthy at {}",
                client.base
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

async fn ensure_daemon_running(client: &ApiClient) -> Result<bool> {
    if client.health().await.is_ok() {
        return Ok(false);
    }

    let bind = api_bind_addr(&client.base)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("daemon")
        .arg("--bind")
        .arg(bind.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Ok(db) = std::env::var("SESSIONBUS_DB") {
        command.arg("--db").arg(db);
    }
    command.spawn().with_context(|| "start sessionbus daemon")?;
    let started = Instant::now();
    loop {
        if client.health().await.is_ok() {
            return Ok(true);
        }
        if started.elapsed() > Duration::from_secs(5) {
            return Err(anyhow!(
                "sessionbus daemon did not become healthy at {}",
                client.base
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn api_bind_addr(base: &str) -> Result<SocketAddr> {
    let without_scheme = base
        .strip_prefix("http://")
        .or_else(|| base.strip_prefix("https://"))
        .unwrap_or(base);
    let authority = without_scheme
        .split('/')
        .next()
        .ok_or_else(|| anyhow!("invalid api URL: {base}"))?;
    authority
        .parse()
        .with_context(|| format!("parse daemon bind address from {base}"))
}

async fn run_mcp_server(client: ApiClient) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: Value = serde_json::from_str(&line)?;
        if let Some(response) = handle_mcp_request(&client, request).await {
            writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
            stdout.flush()?;
        }
    }
    Ok(())
}

async fn handle_mcp_request(client: &ApiClient, request: Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let Some(id) = id else {
        return None;
    };

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2025-11-25",
            "capabilities": {
                "resources": {},
                "tools": {}
            },
            "serverInfo": {
                "name": "sessionbus-aictx-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "tools/list" => Ok(json!({ "tools": mcp_tools() })),
        "tools/call" => mcp_tool_call(client, request.get("params").cloned().unwrap_or_default())
            .await
            .map(|text| json!({ "content": [{ "type": "text", "text": text }], "isError": false })),
        "resources/list" => Ok(json!({
            "resources": [
                {
                    "uri": "sessionbus://current/pack?profile=generic",
                    "name": "Current Sessionbus Pack",
                    "description": "Markdown handoff for the current durable engineering task",
                    "mimeType": "text/markdown"
                }
            ]
        })),
        "resources/read" => {
            mcp_resource_read(
                client,
                request
                    .get("params")
                    .and_then(|params| params.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
            .await
        }
        other => Err(anyhow!("unknown MCP method: {other}")),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32000,
                "message": error.to_string()
            }
        }),
    })
}

fn mcp_tools() -> Value {
    json!([
        {
            "name": "sessionbus_current",
            "description": "Read the current durable engineering session.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_pack",
            "description": "Render a deterministic context pack for the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "profile": {
                        "type": "string",
                        "enum": ["generic", "chatgpt", "claude", "cursor", "acp"]
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_handoff",
            "description": "Render a target-specific handoff for the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "target": {
                        "type": "string",
                        "enum": ["generic", "chatgpt", "claude", "cursor", "acp"]
                    }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_dogfood",
            "description": "Capture current workspace handoff state, then render a deterministic pack for the next AI tool.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "profile": {
                        "type": "string",
                        "enum": ["generic", "chatgpt", "claude", "cursor", "acp"]
                    },
                    "note": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_artifacts",
            "description": "List artifacts for the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_events",
            "description": "List durable bus events for the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_workspace",
            "description": "Inspect the current git workspace.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_add_artifact",
            "description": "Add an artifact to the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "kind": { "type": "string" },
                    "title": { "type": "string" },
                    "body": { "type": "string" },
                    "uri": { "type": "string" }
                },
                "required": ["kind"],
                "additionalProperties": true
            }
        },
        {
            "name": "sessionbus_note",
            "description": "Add an inspectable note artifact to the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "text": { "type": "string" }
                },
                "required": ["text"],
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_decision",
            "description": "Record a durable engineering decision for the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "text": { "type": "string" },
                    "rationale": { "type": "string" }
                },
                "required": ["text"],
                "additionalProperties": false
            }
        },
        {
            "name": "sessionbus_message",
            "description": "Leave an inspectable coordination message in the current or selected session.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "text": { "type": "string" },
                    "to_agent": { "type": "string" },
                    "topic": { "type": "string" },
                    "requires_response": { "type": "boolean" }
                },
                "required": ["text"],
                "additionalProperties": false
            }
        }
    ])
}

async fn mcp_tool_call(client: &ApiClient, params: Value) -> Result<String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("MCP tools/call missing name"))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    match name {
        "sessionbus_current" => {
            let session_id = resolve_session(None)?;
            let session = client.get_session(&session_id).await?;
            Ok(serde_json::to_string_pretty(&session)?)
        }
        "sessionbus_pack" => {
            let session_id = mcp_session_id(&arguments)?;
            let profile = arguments
                .get("profile")
                .and_then(Value::as_str)
                .unwrap_or("generic")
                .parse()
                .map_err(anyhow::Error::msg)?;
            Ok(client.pack(&session_id, profile).await?.markdown)
        }
        "sessionbus_handoff" => {
            let session_id = mcp_session_id(&arguments)?;
            let profile = arguments
                .get("target")
                .and_then(Value::as_str)
                .unwrap_or("generic")
                .parse()
                .map_err(anyhow::Error::msg)?;
            Ok(client.pack(&session_id, profile).await?.markdown)
        }
        "sessionbus_dogfood" => {
            let session_id = mcp_session_id(&arguments)?;
            let profile = arguments
                .get("profile")
                .and_then(Value::as_str)
                .unwrap_or("generic")
                .parse()
                .map_err(anyhow::Error::msg)?;
            let note = arguments
                .get("note")
                .and_then(Value::as_str)
                .map(ToString::to_string);
            let handoff = prepare_dogfood_handoff(client, &session_id, profile, note).await?;
            let artifacts = handoff
                .artifacts
                .iter()
                .map(|artifact| format!("{}={}", artifact.label, artifact.id))
                .collect::<Vec<_>>()
                .join(", ");
            Ok(format!(
                "artifacts: {}\n\n{}",
                artifacts,
                handoff.pack.markdown.trim_end()
            ))
        }
        "sessionbus_artifacts" => {
            let session_id = mcp_session_id(&arguments)?;
            Ok(serde_json::to_string_pretty(
                &client.list_artifacts(&session_id).await?,
            )?)
        }
        "sessionbus_events" => {
            let session_id = mcp_session_id(&arguments)?;
            Ok(serde_json::to_string_pretty(
                &client.list_events(Some(&session_id)).await?,
            )?)
        }
        "sessionbus_workspace" => Ok(workspace_text()?),
        "sessionbus_add_artifact" => {
            let session_id = mcp_session_id(&arguments)?;
            let kind_text = arguments
                .get("kind")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("sessionbus_add_artifact requires kind"))?;
            let kind = serde_json::from_value::<ArtifactKind>(json!(kind_text))?;
            let artifact = client
                .add_artifact(
                    &session_id,
                    CreateArtifactRequest {
                        kind,
                        title: arguments
                            .get("title")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        uri: arguments
                            .get("uri")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        body: arguments
                            .get("body")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        metadata: arguments
                            .get("metadata")
                            .cloned()
                            .unwrap_or_else(|| json!({ "source": "mcp" })),
                        snapshot: true,
                    },
                )
                .await?;
            Ok(format!("added artifact {}", artifact.id))
        }
        "sessionbus_note" => {
            let text = arguments
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("sessionbus_note requires text"))?;
            let session_id = mcp_session_id(&arguments)?;
            let artifact = client
                .add_artifact(
                    &session_id,
                    CreateArtifactRequest {
                        kind: ArtifactKind::Note,
                        title: Some("mcp note".to_string()),
                        uri: None,
                        body: Some(text.to_string()),
                        metadata: json!({ "source": "mcp" }),
                        snapshot: true,
                    },
                )
                .await?;
            Ok(format!("added note artifact {}", artifact.id))
        }
        "sessionbus_decision" => {
            let text = arguments
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("sessionbus_decision requires text"))?;
            let session_id = mcp_session_id(&arguments)?;
            let decision = client
                .add_decision(
                    &session_id,
                    CreateDecisionRequest {
                        text: text.to_string(),
                        rationale: arguments
                            .get("rationale")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                    },
                )
                .await?;
            Ok(format!("recorded decision {}", decision.id))
        }
        "sessionbus_message" => {
            let text = arguments
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("sessionbus_message requires text"))?;
            let session_id = mcp_session_id(&arguments)?;
            let artifact = client
                .add_artifact(
                    &session_id,
                    coordination_message_artifact(
                        text.to_string(),
                        arguments
                            .get("to_agent")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        arguments
                            .get("topic")
                            .and_then(Value::as_str)
                            .map(ToString::to_string),
                        arguments
                            .get("requires_response")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    ),
                )
                .await?;
            Ok(format!("added coordination message {}", artifact.id))
        }
        other => Err(anyhow!("unknown Sessionbus MCP tool: {other}")),
    }
}

async fn mcp_resource_read(client: &ApiClient, uri: &str) -> Result<Value> {
    if !uri.starts_with("sessionbus://current/pack") {
        return Err(anyhow!("unknown Sessionbus MCP resource: {uri}"));
    }
    let profile = uri
        .split('?')
        .nth(1)
        .and_then(|query| {
            query.split('&').find_map(|part| {
                let (key, value) = part.split_once('=')?;
                (key == "profile").then_some(value)
            })
        })
        .unwrap_or("generic")
        .parse()
        .map_err(anyhow::Error::msg)?;
    let session_id = resolve_session(None)?;
    let pack = client.pack(&session_id, profile).await?;
    Ok(json!({
        "contents": [
            {
                "uri": uri,
                "mimeType": "text/markdown",
                "text": pack.markdown
            }
        ]
    }))
}

fn mcp_session_id(arguments: &Value) -> Result<String> {
    resolve_session(
        arguments
            .get("session_id")
            .and_then(Value::as_str)
            .map(ToString::to_string),
    )
}

fn git_diff_artifact() -> Result<CreateArtifactRequest> {
    let status = git_output(["status", "--short"])?.unwrap_or_default();
    let diff = git_output(["diff", "--no-ext-diff"])?.unwrap_or_default();
    let workspace = detect_workspace()?;
    Ok(CreateArtifactRequest {
        kind: ArtifactKind::GitDiff,
        title: Some("git diff".to_string()),
        uri: None,
        body: Some(format!(
            "git status --short\n{}\n\ngit diff\n{}",
            status, diff
        )),
        metadata: json!({
            "branch": workspace.git_branch,
            "head": workspace.head,
            "status": status,
        }),
        snapshot: true,
    })
}

fn git_commit_artifact(rev: &str) -> Result<CreateArtifactRequest> {
    let body = git_output(["show", "--stat", "--patch", "--no-ext-diff", rev])?
        .ok_or_else(|| anyhow!("git commit not found: {rev}"))?;
    let workspace = detect_workspace()?;
    Ok(CreateArtifactRequest {
        kind: ArtifactKind::GitDiff,
        title: Some(format!("git commit {rev}")),
        uri: None,
        body: Some(body),
        metadata: json!({
            "rev": rev,
            "branch": workspace.git_branch,
            "head": workspace.head,
        }),
        snapshot: true,
    })
}

async fn import_pack(client: &ApiClient, path: &Path) -> Result<Session> {
    let raw = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let pack: serde_json::Value = serde_json::from_str(&raw)?;
    let session_value = pack
        .get("session")
        .ok_or_else(|| anyhow!("pack missing session"))?;
    let title = session_value
        .get("title")
        .and_then(|value| value.as_str())
        .unwrap_or("Imported Session")
        .to_string();
    let summary = session_value
        .get("summary")
        .and_then(|value| value.as_str())
        .map(ToString::to_string);
    let workspace = session_value
        .get("workspace")
        .cloned()
        .map(serde_json::from_value::<WorkspaceInfo>)
        .transpose()?;
    let session = client
        .create_session(CreateSessionRequest {
            title,
            workspace,
            summary,
        })
        .await?;

    if let Some(decisions) = pack.get("decisions").and_then(|value| value.as_array()) {
        for decision in decisions {
            if let Some(text) = decision.get("text").and_then(|value| value.as_str()) {
                client
                    .add_decision(
                        &session.id,
                        CreateDecisionRequest {
                            text: text.to_string(),
                            rationale: decision
                                .get("rationale")
                                .and_then(|value| value.as_str())
                                .map(ToString::to_string),
                        },
                    )
                    .await?;
            }
        }
    }

    if let Some(artifacts) = pack.get("artifacts").and_then(|value| value.as_array()) {
        for artifact in artifacts {
            let Some(kind_value) = artifact.get("kind").cloned() else {
                continue;
            };
            let kind = serde_json::from_value::<ArtifactKind>(kind_value)?;
            client
                .add_artifact(
                    &session.id,
                    CreateArtifactRequest {
                        kind,
                        title: artifact
                            .get("title")
                            .and_then(|value| value.as_str())
                            .map(ToString::to_string),
                        uri: artifact
                            .get("uri")
                            .and_then(|value| value.as_str())
                            .map(ToString::to_string),
                        body: artifact
                            .get("body")
                            .and_then(|value| value.as_str())
                            .map(ToString::to_string),
                        metadata: artifact
                            .get("metadata")
                            .cloned()
                            .unwrap_or_else(|| json!({ "imported": true })),
                        snapshot: true,
                    },
                )
                .await?;
        }
    }

    Ok(session)
}
