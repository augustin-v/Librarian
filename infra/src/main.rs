// src/main.rs
use anyhow::Result;
use dotenv::dotenv;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub mod backend;
pub mod utils;

use rig::providers::openai::responses_api::ResponsesCompletionModel;

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();

    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .init();

    let agent = utils::init_agent().await?;
    let backend = backend::Backend::new(agent);
    if let Err(e) = backend.launch().await {
        eprintln!("Failed to launch backend: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
