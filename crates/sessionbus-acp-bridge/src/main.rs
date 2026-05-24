use anyhow::Result;
use clap::{Parser, Subcommand};
use reqwest::StatusCode;
use sessionbus_acp_bridge::{acp_bridge_descriptor, acp_observation_artifact, AcpSessionRef};

#[derive(Debug, Parser)]
#[command(name = "sessionbus-acp-bridge")]
#[command(about = "ACP bridge sidecar for Sessionbus")]
struct Cli {
    #[arg(long, env = "SESSIONBUS_URL", default_value = "http://127.0.0.1:8765")]
    api: String,
    #[command(subcommand)]
    command: CommandKind,
}

#[derive(Debug, Subcommand)]
enum CommandKind {
    Register,
    Observe {
        #[arg(long)]
        session: String,
        #[arg(long)]
        client_name: String,
        #[arg(long)]
        acp_session: String,
        #[arg(long)]
        thread: Option<String>,
        #[arg(long)]
        workspace_root: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();
    let base = cli.api.trim_end_matches('/').to_string();
    match cli.command {
        CommandKind::Register => {
            let response = client
                .post(format!("{base}/adapters/register"))
                .json(&acp_bridge_descriptor())
                .send()
                .await?;
            ensure_success(response).await?;
            println!("{}", sessionbus_acp_bridge::ACP_BRIDGE_ADAPTER_ID);
        }
        CommandKind::Observe {
            session,
            client_name,
            acp_session,
            thread,
            workspace_root,
        } => {
            let artifact = acp_observation_artifact(AcpSessionRef {
                client_name,
                session_id: acp_session,
                thread_id: thread,
                workspace_root,
            });
            let response = client
                .post(format!("{base}/sessions/{session}/artifacts"))
                .json(&artifact)
                .send()
                .await?;
            ensure_success(response).await?;
            println!("observed");
        }
    }
    Ok(())
}

async fn ensure_success(response: reqwest::Response) -> Result<()> {
    let status = response.status();
    let body = response.text().await?;
    if status.is_success() || status == StatusCode::CREATED {
        Ok(())
    } else {
        anyhow::bail!("daemon returned {status}: {body}");
    }
}
