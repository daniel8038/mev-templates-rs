use ethers::types::{Log, Transaction, U256, U64};

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
