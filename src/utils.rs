use std::{collections::HashMap, sync::Arc, time::Instant};

use anyhow::Result;
use ethers::{
    abi::{self, decode, ParamType, Token},
    types::{Filter, H160, H256, U256, U64},
};
use ethers_contract::{self, Contract, Multicall};
use ethers_providers::{Http, Middleware, Provider, Ws};
use fern::colors::{Color, ColoredLevelConfig};
use log::{info, LevelFilter};
use rand::Rng;

use crate::{abi::ABI, pools::Pool};
#[derive(Default, Debug, Clone)]
pub struct Reserve {
    pub reserve0: U256,
    pub reserve1: U256,
}
pub fn setup_logger() -> Result<()> {
    let colors = ColoredLevelConfig {
        trace: Color::Cyan,
        debug: Color::Magenta,
        info: Color::Green,
        warn: Color::Red,
        error: Color::BrightRed,
        ..ColoredLevelConfig::new()
    };
    fern::Dispatch::new()
        .format(move |out, message, record| {
            out.finish(format_args!(
                "[{} {}] {}",
                chrono::Local::now().format("[%H:%M:%S]"),
                colors.color(record.level()),
                message
            ))
        })
        .chain(std::io::stdout())
        .level(log::LevelFilter::Error)
        .level_for("rust", LevelFilter::Info)
        .apply()?;
    Ok(())
}
pub fn calculate_next_block_base_fee(
    gas_used: U256,
    gas_limit: U256,
    base_fee_per_gas: U256,
) -> U256 {
    let gas_used = gas_used;

    let mut target_gas_used = gas_limit / 2;
    target_gas_used = if target_gas_used == U256::zero() {
        U256::one()
    } else {
        target_gas_used
    };

    let new_base_fee = {
        if gas_used > target_gas_used {
            base_fee_per_gas
                + ((base_fee_per_gas * (gas_used - target_gas_used)) / target_gas_used)
                    / U256::from(8u64)
        } else {
            base_fee_per_gas
                - ((base_fee_per_gas * (target_gas_used - gas_used)) / target_gas_used)
                    / U256::from(8u64)
        }
    };

    let seed = rand::thread_rng().gen_range(0..9);
    new_base_fee + seed
}
pub async fn get_uniswap_v2_reserves(
    https_url: String,
    pools: Vec<Pool>,
) -> Result<HashMap<H160, Reserve>> {
    let client = Provider::<Http>::try_from(https_url).unwrap();
    let client = Arc::new(client);
    let abi = ABI::new();
    // 创建多重调用实例
    let mut multicall = Multicall::new(client.clone(), None).await?;
    //
    for pool in &pools {
        let contract = Contract::<Provider<Http>>::new(
            pool.address,
            abi.uniswap_v2_pair.clone(),
            client.clone(),
        );
        let call = contract.method::<_, H256>("getReserves", ())?;
        multicall.add_call(call, false);
    }
    //  执行批量调用 拿到结果
    let result = multicall.call_raw().await?;
    let mut reserves = HashMap::new();
    //  处理返回结果
    for i in 0..result.len() {
        let pool = &pools[i];
        let reserve = result[i].clone();
        // 解析返回数据
        match reserve.unwrap() {
            abi::Token::Tuple(response) => {
                let reserve_data = Reserve {
                    reserve0: response[0].clone().into_uint().unwrap(),
                    reserve1: response[1].clone().into_uint().unwrap(),
                };
                reserves.insert(pool.address.clone(), reserve_data);
            }
            _ => {}
        }
    }
    Ok(reserves)
}
// 批量获取 Uniswap V2 池子储备量
pub async fn batch_get_uniswap_v2_reserves(
    https_url: String,
    pools: Vec<Pool>,
) -> HashMap<H160, Reserve> {
    let start_time = Instant::now();
    let pools_cnt = pools.len();
    // 使用 ceil() 向上取整确保不遗漏
    let batch = ((pools_cnt / 250) as f32).ceil(); // 计算需要多少批次（每250个一批）

    // usize无符号整数类型，大小取决于系统架构
    let pools_per_batch = ((pools_cnt as f32) / batch).ceil() as usize; // 每批的池子数量
    let mut handles = vec![];
    for i in 0..(batch as usize) {
        let start_idx = i * pools_per_batch;
        let end_idx = std::cmp::min(start_idx + pools_per_batch, pools_cnt);
        let handle = tokio::spawn(get_uniswap_v2_reserves(
            https_url.clone(),
            pools[start_idx..end_idx].to_vec(),
        ));
        handles.push(handle);
    }
    let mut reserves: HashMap<H160, Reserve> = HashMap::new();
    for handle in handles {
        let result = handle.await.unwrap();
        reserves.extend(result.unwrap());
    }
    info!(
        "Batch reserves call took: {} seconds",
        start_time.elapsed().as_secs()
    );
    reserves
}
pub async fn get_touched_pool_reserves(
    provider: Arc<Provider<Ws>>,
    block_number: U64,
) -> Result<HashMap<H160, Reserve>> {
    //对应 reserve0 和 reserve1
    let sync_event = "Sync(uint112,uint112)";
    // 创建事件过滤器
    let event_filter = Filter::new()
        .from_block(block_number)
        .to_block(block_number)
        .event(sync_event);
    // 获取日志
    let logs = provider.get_logs(&event_filter).await?;
    let mut tx_idx = HashMap::new(); // 存储每个池子最新的交易索引
    let mut reserves = HashMap::new(); // 存储每个池子的最新储备量

    //
    for log in &logs {
        // 解码日志数据
        let decoded = decode(&[ParamType::Uint(256), ParamType::Uint(256)], &log.data);
        match decoded {
            Ok(data) => {
                // 区块中的第一笔交易索引是0，第二笔是1，依此类推
                //  获取交易索引 获取交易索引:主要作用是确保我们获取到的是池子在该区块中的最新状态
                let idx = log.transaction_index.unwrap_or_default();
                //  获取该池子之前的交易索引（如果有）
                let prev_tx_idx = tx_idx.get(&log.address);
                // 判断是否需要更新
                // tx_idx 记录的索引 已经小于 最新的索引值 证明有新的储备量变化
                let update = (*prev_tx_idx.unwrap_or(&U64::zero())) <= idx;
                if update {
                    let reserve0 = match data[0] {
                        Token::Uint(rs) => rs,
                        _ => U256::zero(),
                    };
                    let reserve1 = match data[1] {
                        Token::Uint(rs) => rs,
                        _ => U256::zero(),
                    };
                    let reserve = Reserve { reserve0, reserve1 };
                    reserves.insert(log.address, reserve);
                    tx_idx.insert(log.address, idx);
                }
            }
            Err(_) => {}
        }
    }
    Ok(reserves)
}
