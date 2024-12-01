use std::sync::Arc;

use anyhow::Result;
use dotenv::dotenv;
use ethers_providers::{Provider, Ws};
use rust::{constants::Env, streams::Event, utils::setup_logger};
use tokio::sync::broadcast::{self, Sender};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    setup_logger()?;
    let env = Env::new();
    let ws = Provider::<Ws>::connect(env.wss_url).await?;
    // ws所有权被Arc获取 或者move
    let ws_provider = Arc::new(ws);
    let (event_sender, _): (Sender<Event>, _) = broadcast::channel(512);
    Ok(())
}
