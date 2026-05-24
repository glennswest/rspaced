use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

mod artifacts;
mod cli;
mod mirror;
mod output;
mod registry;
mod stage;
mod verify;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    cli::run(cli::Cli::parse())
}
