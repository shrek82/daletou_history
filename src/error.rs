/// 错误类型定义
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DaletouError {
    #[error("网络请求失败: {0}")]
    Network(#[from] reqwest::Error),

    #[error("页面解析失败: {0}")]
    ParseError(String),

    #[error("编码转换失败: {0}")]
    EncodingError(String),

    #[error("页码无效: {0}，有效范围 1-{1}")]
    InvalidPage(u32, u32),
}
