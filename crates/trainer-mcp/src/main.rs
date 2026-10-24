//! `trainer-mcp`: stdio MCP server for the singing trainer.
//!
//! Usage: `trainer-mcp [--data-dir <dir>]`. Without `--data-dir` it uses the app's data
//! directory (`%APPDATA%\SingingTrainer` on Windows, or `SINGING_TRAINER_DATA_DIR`).

use std::path::PathBuf;

use anyhow::{Context, Result};
use rmcp::ServiceExt;
use trainer_core::storage::{Db, db_path, default_data_dir};
use trainer_mcp::TrainerServer;

fn parse_args() -> Result<PathBuf> {
    let mut args = std::env::args().skip(1);
    let mut data_dir = None;
    while let Some(a) = args.next() {
        match a.as_str() {
            "--data-dir" => {
                data_dir = Some(PathBuf::from(
                    args.next().context("--data-dir needs a path")?,
                ))
            }
            "--help" | "-h" => {
                eprintln!(
                    "trainer-mcp [--data-dir <dir>]  (default: {})",
                    default_data_dir().display()
                );
                std::process::exit(0);
            }
            "--version" => {
                println!("trainer-mcp {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => anyhow::bail!("unknown argument `{other}`"),
        }
    }
    Ok(data_dir.unwrap_or_else(default_data_dir))
}

#[tokio::main]
async fn main() -> Result<()> {
    // Logs go to stderr; stdout carries the protocol.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let data_dir = parse_args()?;
    let db = Db::open(&db_path(&data_dir))
        .with_context(|| format!("opening the database in {}", data_dir.display()))?;
    let server = TrainerServer::new(db, data_dir);
    let service = server.serve(rmcp::transport::stdio()).await?;
    service.waiting().await?;
    Ok(())
}
