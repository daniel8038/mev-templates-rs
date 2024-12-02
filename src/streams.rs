use std::sync::Arc;

use crate::utils::calculate_next_block_base_fee;
use ethers::types::{Log, Transaction, U256, U64};
use ethers_providers::{Middleware, Provider, Ws};
use tokio::sync::broadcast::Sender;
use tokio_stream::StreamExt;
#[derive(Default, Debug, Clone)]
pub struct NewBlock {
    pub block_number: U64,
    pub base_fee: U256,
    pub next_base_fee: U256,
}
#[derive(Debug, Clone)]
pub enum Event {
    Block(NewBlock),
    PendingTx(Transaction),
    Log(Log),
}
// 处理区块信息流
pub async fn stream_new_block(provider: Arc<Provider<Ws>>, event_sender: Sender<Event>) {
    let stream = provider.subscribe_blocks().await.unwrap();
    let mut stream = stream.filter_map(|block| match block.number {
        Some(number) => Some(NewBlock {
            block_number: number,
            base_fee: block.base_fee_per_gas.unwrap_or_default(),
            next_base_fee: U256::from(calculate_next_block_base_fee(
                block.gas_used,
                block.gas_limit,
                block.base_fee_per_gas.unwrap_or_default(),
            )),
        }),
        None => None,
    });
    while let Some(block) = stream.next().await {
        match event_sender.send(Event::Block(block)) {
            Ok(_) => {}
            Err(_) => {}
        }
    }
}
