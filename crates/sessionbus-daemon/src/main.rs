use clap::Parser;
use sessionbus_daemon::{default_db_path, serve};
use sessionbus_store::SessionbusStore;
use std::{net::SocketAddr, path::PathBuf};

#[derive(Debug, Parser)]
#[command(name = "sessionbus-daemon")]
#[command(about = "Local Sessionbus API daemon")]
struct Args {
    #[arg(long, default_value = "127.0.0.1:8765")]
    bind: SocketAddr,
    #[arg(long, env = "SESSIONBUS_DB")]
    db: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "sessionbus_daemon=info,tower_http=info".into()),
        )
        .init();
    let db = args.db.unwrap_or_else(default_db_path);
    let store = SessionbusStore::open_path(db).await?;
    serve(args.bind, store).await
}
