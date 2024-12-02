use ethers::{
    prelude::Lazy,
    types::{Address, H160, U256, U64},
};
use std::str::FromStr;
// 普通静态变量必须在编译时就能确定值
// 复杂计算或运行时的值无法直接用作静态变量
// Lazy 允许第一次访问时才进行初始化
pub static WEI: Lazy<U256> = Lazy::new(|| U256::from(10).pow(U256::from(18)));
pub static GWEI: Lazy<U256> = Lazy::new(|| U256::from(10).pow(U256::from(9)));

pub static ZERO_ADDRESS: Lazy<Address> =
    Lazy::new(|| Address::from_str("0x0000000000000000000000000000000000000000").unwrap());
#[derive(Debug, Clone)]
pub struct Env {
    pub https_url: String,
    pub wss_url: String,
    pub chain_id: U64,
    pub private_key: String,
    pub signing_key: String,
    pub bot_address: String,
}
pub fn get_env(key: &str) -> String {
    std::env::var(key).unwrap()
}
impl Env {
    pub fn new() -> Self {
        Env {
            //"HTTPS_URL" 存储在程序的只读数据段 位于程序的只读数据段（.rodata 段） 和程序代码一起加载到内存中
            // 这段内存：
            // - 是只读的
            // - 在程序启动时分配
            // - 在程序结束时由操作系统回收
            // - 不是在堆或栈上动态分配的
            // 程序启动时由操作系统一次性分配
            // 程序结束时由操作系统一次性回收
            // rust 注意性能：
            // 静态数据是程序的固定部分，由操作系统统一管理
            // 堆内存是动态申请的资源，需要及时释放以避免浪费
            https_url: get_env("HTTPS_URL"),
            wss_url: get_env("WSS_URL"),
            chain_id: U64::from_str(&get_env("CHAIN_ID")).unwrap(),
            private_key: get_env("PRIVATE_KEY"),
            signing_key: get_env("SIGNING_KEY"),
            bot_address: get_env("BOT_ADDRESS"),
        }
    }
}
pub fn get_blacklist_tokens() -> Vec<H160> {
    return vec!["0x9469603F3Efbcf17e4A5868d81C701BDbD222555"]
        .into_iter()
        .map(|addr| H160::from_str(addr).unwrap())
        .collect();
}
