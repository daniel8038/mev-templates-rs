use std::sync::Arc;

use anyhow::Result;
use dotenv::dotenv;
use ethers_providers::{Provider, Ws};
use rust::{
    constants::Env,
    streams::{stream_new_block, Event},
    utils::setup_logger,
};
use tokio::{
    sync::broadcast::{self, Sender},
    task::JoinSet,
};

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    setup_logger()?;
    let env = Env::new();
    let ws = Provider::<Ws>::connect(env.wss_url).await?;
    // ws所有权被Arc获取 或者move
    let ws_provider = Arc::new(ws);
    let (event_sender, _): (Sender<Event>, _) = broadcast::channel(512);
    let mut set = JoinSet::new();
    // 获取区块信息
    set.spawn(stream_new_block(ws_provider, event_sender));
    // 获取pending交易
    //
    Ok(())
}
