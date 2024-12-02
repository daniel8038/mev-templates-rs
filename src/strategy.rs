use ethers::types::{Address, H160, U256};
use ethers_providers::{Provider, Ws};
use log::info;
use std::sync::Arc;
use std::{collections::HashMap, str::FromStr};
use tokio::sync::broadcast::Sender;

use crate::constants::WEI;
use crate::pools::Pool;
use crate::simulator::UniswapV2Simulator;
use crate::utils::{batch_get_uniswap_v2_reserves, get_touched_pool_reserves};
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
    // 生成所有的usdc_address交换路径 多跳为3
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
    let mut reserves =
        batch_get_uniswap_v2_reserves(env.https_url.clone(), pools_vec.clone()).await;

    // 订阅事件
    let mut event_receiver = event_sender.subscribe();
    //
    loop {
        match event_receiver.recv().await {
            Ok(event) => match event {
                Event::Block(block) => {
                    info!("{:?}", block);
                    let touched_reserves =
                        match get_touched_pool_reserves(provider.clone(), block.block_number).await
                        {
                            Ok(res) => res,
                            Err(e) => {
                                info!("Error from get_touched_pool_reserves: {:?}", e);
                                HashMap::new()
                            }
                        };
                    // 涉及储备量变化的池子
                    let mut touched_pools = Vec::new();
                    for (address, reserve) in touched_reserves {
                        if reserves.contains_key(&address) {
                            reserves.insert(address, reserve);
                            touched_pools.push(address);
                        }
                    }
                    info!("{:?}", touched_pools);
                    // 1. 创建存储价差的HashMap
                    let mut spreads = HashMap::new();

                    // 2. 遍历所有可能的套利路径 有价差就加入spreads
                    // enumerate() 会给迭代器的每个元素加上一个索引
                    for (idx, path) in (&paths).iter().enumerate() {
                        // 3. 检查路径是否包含发生变化的池子
                        // 只有当路径中的某个池子储备量发生变化时，才有可能出现套利机会
                        let touched_path = touched_pools
                            .iter()
                            .map(|pool| path.has_pool(&pool) as i32)
                            .sum::<i32>()
                            >= 1;
                        if touched_path {
                            // 4. 模拟交易
                            let one_token_in = U256::from(1); // 用1个代币测试
                            let simulated = path.simulate_v2_path(one_token_in, &reserves);
                            match simulated {
                                Some(price_quote) => {
                                    let one_usdc_in = one_token_in * U256::from(usdc_decimals);
                                    let _out = price_quote.as_u128() as i128;
                                    let _in = one_usdc_in.as_u128() as i128;
                                    let spread = _out - _in;
                                    if spread > 0 {
                                        spreads.insert(idx, spread);
                                    }
                                }
                                None => {}
                            }
                        }
                    }
                    // 获取 USDC-WETH 池子信息
                    let usdc_weth_address =
                        Address::from_str("0x397FF1542f962076d0BFE58eA045FfA2d347ACa0").unwrap();
                    let pool = pools.get(&usdc_weth_address).unwrap();
                    let reserve = reserves.get(&usdc_weth_address).unwrap();
                    // 根据公式 计算 WETH 价格
                    let weth_price = UniswapV2Simulator::reserves_to_price(
                        reserve.reserve0,
                        reserve.reserve1,
                        pool.decimals0,
                        pool.decimals1,
                        false,
                    );
                    // 获取下一个区块的基础 gas 费
                    let base_fee = block.next_base_fee;
                    // 预估 gas 使用量
                    let estimated_gas_usage = U256::from(550000);
                    // 计算总 gas 成本（以 wei 为单位）
                    let gas_cost_in_wei = base_fee * estimated_gas_usage;
                    // 转换为 WMATIC
                    let gas_cost_in_wmatic =
                        (gas_cost_in_wei.as_u64() as f64) / ((*WEI).as_u64() as f64);
                    // 转换为 USDC
                    let gas_cost_in_usdc = weth_price * gas_cost_in_wmatic;
                    // 调整精度
                    let gas_cost_in_usdc =
                        U256::from((gas_cost_in_usdc * ((10 as f64).powi(usdc_decimals))) as u64);
                    //
                    let mut sorted_spreads: Vec<_> = spreads.iter().collect();
                    sorted_spreads.sort_by_key(|x| x.1);
                    sorted_spreads.reverse();
                    // 遍历排序后的套利机会
                    for spread in sorted_spreads {
                        let path_idx = spread.0;
                        let path = &paths[*path_idx];
                        // 优化输入金额
                        let opt = path.optimize_amount_in(U256::from(1000), 10, &reserves);
                        // 计算扣除 gas 后的净利润
                        let excess_profit =
                            (opt.1.as_u128() as i128) - (gas_cost_in_usdc.as_u128() as i128);

                        // TODO
                        if excess_profit > 0 {
                            // 构建套利交易
                            // 签名交易
                            // 发送交易
                            // 监控交易状态
                            // 处理交易结果
                        }
                    }
                }
                Event::PendingTx(_) => {
                    // not using pending tx
                }
                Event::Log(_) => {
                    // not using logs
                }
            },
            Err(_) => {}
        }
    }
}
