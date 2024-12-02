use std::time::Instant;

use ethers::types::H160;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;

use crate::pools::{self, Pool};

#[derive(Debug, Clone)]
// 三角套利路径
pub struct ArbPath {
    pub nhop: u8,             // 跳数（经过多少个池子）
    pub pool_1: Pool,         // 第一个交易池
    pub pool_2: Pool,         // 第二个交易池
    pub pool_3: Pool,         // 第三个交易池
    pub zero_for_one_1: bool, // 第一个池子的交易方向
    pub zero_for_one_2: bool, // 第二个池子的交易方向
    pub zero_for_one_3: bool, // 第三个池子的交易方向
}
impl ArbPath {
    fn _get_pool(&self, i: u8) -> &Pool {
        match i {
            0 => Some(&self.pool_1),
            1 => Some(&self.pool_2),
            2 => Some(&self.pool_3),
            _ => None,
        }
        .unwrap()
    }
    pub fn should_blacklist(&self, blacklist_tokens: &Vec<H160>) -> bool {
        for i in 0..self.nhop {
            let pool = self._get_pool(i);
            return blacklist_tokens.contains(&pool.token0)
                || blacklist_tokens.contains(&pool.token1);
        }
        false
    }
}
// 生成所有的交换路径 多跳为3
// e:
// USDC -> WETH (池子1)
// WETH -> USDT (池子2)
// USDT -> USDC (池子3)
pub fn generate_triangular_paths(pools: &Vec<Pool>, token_in: H160) -> Vec<ArbPath> {
    let start_time = Instant::now();
    let token_out = token_in.clone();
    let mut paths: Vec<ArbPath> = Vec::new();
    let pb = ProgressBar::new(pools.len() as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}",
        )
        .unwrap()
        .progress_chars("##-"),
    );
    for i in 0..pools.len() {
        let pool_1 = &pools[i];
        // 检查这个池子是否包含起始代币
        let can_trade_1 = (pool_1.token0 == token_in) || (pool_1.token1 == token_in);
        if can_trade_1 {
            // 确定交易方向
            let zero_for_one_1 = pool_1.token0 == token_in;
            // 根据交易方向获取输入输出代币
            let (token_in_1, token_out_1) = if zero_for_one_1 {
                (pool_1.token0, pool_1.token1)
            } else {
                (pool_1.token1, pool_1.token0)
            };
            // 确保第一个池子的输入代币是我们想要的起始代币
            if token_in_1 != token_in {
                continue;
            }
            for j in 0..pools.len() {
                let pool_2 = &pools[j];
                let can_trade_2 = (pool_2.token0 == token_out_1) || (pool_2.token1 == token_out_1);
                if can_trade_2 {
                    let zero_for_one_2 = pool_2.token0 == token_out_1;
                    let (token_in_2, token_out_2) = if zero_for_one_2 {
                        (pool_2.token0, pool_2.token1)
                    } else {
                        (pool_2.token1, pool_2.token0)
                    };
                    // 确保第一跳的输出代币是第二跳的输入代币
                    if token_out_1 != token_in_2 {
                        continue;
                    }
                    for k in 0..pools.len() {
                        // 确定token in token out 确定 pool_3
                        let pool_3 = &pools[k];
                        // 判断能不能交易
                        let can_trade_3 =
                            (pool_3.token0 == token_out_2) || (pool_3.token1 == token_out_2);
                        if can_trade_3 {
                            let zero_for_one_3 =
                                (pool_3.token0 == token_out_2) || (pool_3.token1 == token_out_2);
                            let (token_in_3, token_out_3) = if zero_for_one_3 {
                                (pool_3.token0, pool_3.token1)
                            } else {
                                (pool_3.token1, pool_3.token0)
                            };
                            if token_out_2 != token_in_3 {
                                continue;
                            }
                            // 确保最后回到起始代币
                            if token_out_3 == token_out {
                                // 确保三个池子都不相同
                                let unique_pool_cnt =
                                    vec![pool_1.address, pool_2.address, pool_3.address]
                                        .into_iter()
                                        .unique()
                                        .collect::<Vec<H160>>()
                                        .len();

                                if unique_pool_cnt < 3 {
                                    continue;
                                }

                                let arb_path = ArbPath {
                                    nhop: 3,
                                    pool_1: pool_1.clone(),
                                    pool_2: pool_2.clone(),
                                    pool_3: pool_3.clone(),
                                    zero_for_one_1: zero_for_one_1,
                                    zero_for_one_2: zero_for_one_2,
                                    zero_for_one_3: zero_for_one_3,
                                };

                                paths.push(arb_path);
                            }
                        }
                    }
                };
            }
        }
    }
    pb.finish_with_message(format!(
        "Generated {} 3-hop arbitrage paths in {} seconds",
        paths.len(),
        start_time.elapsed().as_secs()
    ));
    paths
}
