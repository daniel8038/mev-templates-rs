use ethers::types::H160;
use ethers_providers::{Provider, Ws};
use log::info;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::broadcast::Sender;

use crate::{constants::Env, pools::load_all_pools_from_v2, streams::Event};

pub async fn event_handler(provider: Arc<Provider<Ws>>, event_sender: Sender<Event>) {
    let env = Env::new();
    // address
    let factory_addresses = vec!["0xC0AEe478e3658e2610c5F7A4A2E1777cE9e4f2Ac"];
    let router_addresses = vec!["0xd9e1cE17f2641f24aE83637ab66a2cca9C378B9F"];
    let factory_blocks = vec![10794229u64];
    let pools_vec = load_all_pools_from_v2(env.wss_url, factory_addresses, factory_blocks)
        .await
        .unwrap();
    info!("Initial pool count: {}", pools_vec.len());
    // Performing USDC triangular arbitrage
    let usdc_address = H160::from_str("0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48").unwrap();
    let usdc_decimals = 6;

    // 订阅事件
    let mut event_receiver = event_sender.subscribe();
}
