use ethers::types::H160;
use ethers_providers::{Provider, Ws};
use log::info;
use std::sync::Arc;
use std::{collections::HashMap, str::FromStr};
use tokio::sync::broadcast::Sender;

use crate::pools::Pool;
use crate::utils::batch_get_uniswap_v2_reserves;
use crate::{
    constants::{get_blacklist_tokens, Env},
    paths::generate_triangular_paths,
    pools::load_all_pools_from_v2,
    streams::Event,
};

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
    let paths = generate_triangular_paths(&pools_vec, usdc_address);
    let blacklist_tokens = get_blacklist_tokens();
    // 三角路径池map
    let mut pools = HashMap::new();
    for path in &paths {
        if !path.should_blacklist(&blacklist_tokens) {
            pools.insert(path.pool_1.address.clone(), path.pool_1.clone());
            pools.insert(path.pool_2.address.clone(), path.pool_2.clone());
            pools.insert(path.pool_3.address.clone(), path.pool_3.clone());
        }
    }
    info!("New pool count: {:?}", pools.len());
    // pools_vec从所有池子转换成三角路径池 且是有关usdc_address的
    // pools.values() - 获取 HashMap 中所有的值（Pool）的引用
    // cloned() - 克隆每个 Pool
    // collect() - 收集到一个新的 Vec 中
    let pools_vec: Vec<Pool> = pools.values().cloned().collect();
    let reserves = batch_get_uniswap_v2_reserves(env.https_url.clone(), pools_vec.clone());
    // 订阅事件
    let mut event_receiver = event_sender.subscribe();
}
