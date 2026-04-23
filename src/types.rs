use serde::{Serialize, Deserialize};

/// 红球和蓝球组合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BallSet {
    /// 5个红球
    pub red: Vec<u8>,
    /// 2个蓝球
    pub blue: Vec<u8>,
}

/// 单期开奖记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawRecord {
    /// 期号，如 "2026043"
    pub issue: String,
    /// 开奖日期，格式 "2026-04-22"
    pub date: String,
    /// 星期
    pub weekday: String,
    /// 开奖号码
    pub balls: BallSet,
    /// 奖池金额（原始字符串，含逗号）
    pub prize_pool: String,
}

/// 一页开奖记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawPage {
    /// 当前页码
    pub current_page: u32,
    /// 总页数
    pub total_pages: u32,
    /// 当期记录列表
    pub records: Vec<DrawRecord>,
}
