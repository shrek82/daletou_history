/// 大乐透开奖信息查询
///
/// 支持实时获取最新开奖号码和历史开奖记录
///
/// # 示例
///
/// ```no_run
/// use daletou::Client;
///
/// let client = Client::new();
/// let latest = client.get_latest().unwrap();
/// println!("最新开奖: {} 期", latest.issue);
/// ```

mod client;
mod error;
mod types;

pub use client::Client;
pub use error::DaletouError;
pub use types::{BallSet, DrawPage, DrawRecord};
