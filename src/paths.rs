use std::{collections::HashMap, time::Instant};

use ethers::{
    abi,
    types::{Address, H160, U256},
};
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;

use crate::{
    pools::{self, Pool},
    simulator::UniswapV2Simulator,
    utils::Reserve,
};
#[derive(Debug, Clone)]
pub struct PathParam {
    pub router: Address,
    pub token_in: Address,
    pub token_out: Address,
}

impl PathParam {
    pub fn make_params(&self) -> Vec<abi::Token> {
        vec![
            abi::Token::Address(self.router.into()),
            abi::Token::Address(self.token_in.into()),
            abi::Token::Address(self.token_out.into()),
        ]
    }
}
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
    pub fn has_pool(&self, pool: &H160) -> bool {
        let is_pool_1 = self.pool_1.address == *pool;
        let is_pool_2 = self.pool_2.address == *pool;
        let is_pool_3 = self.pool_3.address == *pool;
        return is_pool_1 || is_pool_2 || is_pool_3;
    }
    pub fn _get_zero_for_one(&self, i: u8) -> bool {
        match i {
            0 => Some(self.zero_for_one_1),
            1 => Some(self.zero_for_one_2),
            2 => Some(self.zero_for_one_3),
            _ => None,
        }
        .unwrap()
    }
    pub fn _get_pool(&self, i: u8) -> &Pool {
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
    pub fn simulate_v2_path(
        &self,
        amount_in: U256,
        reserves: &HashMap<H160, Reserve>,
    ) -> Option<U256> {
        let token_in_decimals = if self.zero_for_one_1 {
            self.pool_1.decimals0
        } else {
            self.pool_2.decimals1
        };
        let uint = U256::from(10).pow(U256::from(token_in_decimals));
        let mut amount_out = amount_in * uint;
        for i in 0..self.nhop {
            let pool = self._get_pool(i);
            let zero_for_one = self._get_zero_for_one(i);
            let reserve = reserves.get(&pool.address)?;
            let reserve0 = reserve.reserve0;
            let reserve1 = reserve.reserve1;
            let fee = U256::from(pool.fee);
            let reserve_in;
            let reserve_out;
            if zero_for_one {
                reserve_in = reserve0;
                reserve_out = reserve1;
            } else {
                reserve_in = reserve1;
                reserve_out = reserve0;
            }
            amount_out =
                UniswapV2Simulator::get_amount_out(amount_out, reserve_in, reserve_out, fee)?;
        }
        Some(amount_out)
    }
    // 优化输入金额，找到最佳套利数量
    // 交易量越大，滑点越大
    // 滑点会降低实际获得的代币数量
    // 当滑点损失超过套利利润时，继续增加输入反而会减少利润
    pub fn optimize_amount_in(
        &self,
        max_amount_in: U256,               // 最大输入金额
        step_size: usize,                  // 每次增加的步长
        reserves: &HashMap<H160, Reserve>, // 所有池子的储备量
    ) -> (U256, U256) {
        // 获取输入代币的小数位数
        let token_in_decimals = if self.zero_for_one_1 {
            self.pool_1.decimals0
        } else {
            self.pool_1.decimals1
        };
        // 初始化最优值
        let mut optimized_in = U256::zero(); // 最优输入金额
        let mut profit = 0; // 最大利润
                            // 逐步增加输入金额寻找最优值
        for amount_in in (0..max_amount_in.as_u64()).step_by(step_size) {
            let amount_in = U256::from(amount_in);
            // 计算单位（考虑代币小数位）
            let unit = U256::from(10).pow(U256::from(token_in_decimals));
            // 模拟这个输入金额的交易路径
            if let Some(amount_out) = self.simulate_v2_path(amount_in, &reserves) {
                // 计算这次尝试的利润
                let this_profit =
                    (amount_out.as_u128() as i128) - ((amount_in * unit).as_u128() as i128);
                // 如果利润更高，更新最优值
                if this_profit >= profit {
                    optimized_in = amount_in;
                    profit = this_profit;
                } else {
                    // 如果利润开始下降，说明找到了最优点
                    break;
                }
            }
        }

        (optimized_in, U256::from(profit))
    }
    // 将交易路径转换为路由参数
    pub fn to_path_params(&self, routers: &Vec<H160>) -> Vec<PathParam> {
        let mut path_params = Vec::new();
        // 遍历路径中的每一跳
        for i in 0..self.nhop {
            // 获取当前池子和交易方向
            let pool = self._get_pool(i);
            let zero_for_one = self._get_zero_for_one(i);
            let token_in;
            let token_out;
            // 根据交易方向确定输入输出代币
            if zero_for_one {
                token_in = pool.token0;
                token_out = pool.token1;
            } else {
                token_in = pool.token1;
                token_out = pool.token0;
            }
            // 创建路径参数
            let param = PathParam {
                router: routers[i as usize], // 使用对应的路由合约
                token_in: token_in,          // 输入代币
                token_out: token_out,        // 输出代币
            };
            path_params.push(param);
        }
        path_params
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
