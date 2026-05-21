mod cli;
mod config;
mod db;
mod models;
mod providers;
mod services;
mod utils;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::{fmt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("marketfeed=info".parse()?))
        .init();

    cli::run(cli::Cli::parse()).await
}
