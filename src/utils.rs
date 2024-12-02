use std::{sync::Arc, time::Instant};

use anyhow::Result;
use ethers::{
    abi,
    types::{H160, H256, U256},
};
use ethers_contract::{self, Contract, Multicall};
use ethers_providers::{Http, Provider};
use fern::colors::{Color, ColoredLevelConfig};
use hashbrown::HashMap;
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
