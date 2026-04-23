use encoding_rs::Encoding;
use reqwest::blocking::Client as HttpClient;
use scraper::{Html, Selector};
use serde::{Serialize, Deserialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::DaletouError;
use crate::types::{BallSet, DrawPage, DrawRecord};

/// 每页固定约 30 条记录
const RECORDS_PER_PAGE: usize = 30;

/// 默认请求间隔
const DEFAULT_REQUEST_INTERVAL: Duration = Duration::from_secs(2);

/// 默认缓存过期时间（1小时）
const DEFAULT_CACHE_TTL: Duration = Duration::from_secs(3600);

/// GB18030 编码（兼容 GB2312）
const GB2312: &'static Encoding = encoding_rs::GB18030;

/// 预编译的 CSS 选择器，避免重复解析
struct Selectors {
    line: Selector,
    qs: Selector,
    date: Selector,
    red_ball: Selector,
    blue_ball: Selector,
    money: Selector,
    page_text: Selector,
}

impl Selectors {
    fn new() -> Self {
        Self {
            line: Selector::parse("div.table-line").unwrap(),
            qs: Selector::parse("div.qs").unwrap(),
            date: Selector::parse("div.date").unwrap(),
            red_ball: Selector::parse("div.red-ball").unwrap(),
            blue_ball: Selector::parse("div.blue-ball").unwrap(),
            money: Selector::parse("div.money").unwrap(),
            page_text: Selector::parse("div.page-text").unwrap(),
        }
    }
}

/// 缓存数据结构
#[derive(Serialize, Deserialize)]
struct CacheData {
    /// 缓存时间戳（秒）
    timestamp: u64,
    /// 缓存的最新期号
    latest_issue: String,
    /// 缓存的记录列表（按期号降序）
    records: Vec<DrawRecord>,
}

/// 大乐透开奖信息查询客户端
pub struct Client {
    http: HttpClient,
    base_url: String,
    selectors: Selectors,
    /// 两次请求之间的最小间隔，防止被封
    request_interval: Duration,
    /// 缓存过期时间
    cache_ttl: Duration,
    /// 缓存文件路径
    cache_path: Option<PathBuf>,
}

impl Client {
    /// 创建新客户端（默认请求间隔 2 秒，缓存 1 小时）
    pub fn new() -> Self {
        Self {
            http: HttpClient::builder()
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                .build()
                .expect("构建 HTTP 客户端失败"),
            base_url: "https://www.cjcp.cn".to_string(),
            selectors: Selectors::new(),
            request_interval: DEFAULT_REQUEST_INTERVAL,
            cache_ttl: DEFAULT_CACHE_TTL,
            cache_path: None,
        }
    }

    /// 设置请求间隔（防封策略）
    pub fn with_request_interval(mut self, interval: Duration) -> Self {
        self.request_interval = interval;
        self
    }

    /// 设置缓存文件路径
    ///
    /// 设置后，`get_cached_records` 会优先从缓存读取，
    /// 仅在缓存过期或需要更新时才发起网络请求。
    pub fn with_cache_path(mut self, path: PathBuf) -> Self {
        self.cache_path = Some(path);
        self
    }

    /// 设置缓存过期时间
    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    /// 获取最新一期开奖信息（第1页第一条）
    pub fn get_latest(&self) -> Result<DrawRecord, DaletouError> {
        let page = self.get_page(1)?;
        page.records
            .into_iter()
            .next()
            .ok_or(DaletouError::ParseError("页面未找到开奖记录".into()))
    }

    /// 获取指定页的开奖记录
    pub fn get_page(&self, page: u32) -> Result<DrawPage, DaletouError> {
        let url = Self::page_url(&self.base_url, page);
        let body = self.fetch_gb2312(&url)?;
        self.parse_page(&body, page)
    }

    /// 从缓存或网络获取历史开奖记录
    ///
    /// 优先读取缓存，如果缓存过期或缓存中缺少最新数据，
    /// 则自动从网络更新。
    pub fn get_cached_records(&self, count: usize) -> Result<Vec<DrawRecord>, DaletouError> {
        let cache_path = self.cache_path.as_ref().ok_or_else(|| {
            DaletouError::ParseError("未设置缓存路径，请先调用 with_cache_path".into())
        })?;

        // 尝试读取缓存
        if let Some(cache) = self.load_cache(cache_path) {
            if self.is_cache_valid(&cache) {
                // 缓存有效，直接从缓存取
                if cache.records.len() >= count {
                    let mut records = cache.records;
                    records.truncate(count);
                    return Ok(records);
                }
            }
        }

        // 缓存过期或记录不足，从网络获取
        println!("缓存过期或记录不足，正在从网络获取...");
        let records = self.get_latest_n(count)?;

        // 写入缓存
        self.save_cache(cache_path, &records)?;

        Ok(records)
    }

    /// 获取前 N 页的所有开奖记录
    pub fn get_pages(&self, count: u32) -> Result<Vec<DrawRecord>, DaletouError> {
        let first_page = self.get_page(1)?;
        let total = first_page.total_pages.min(count);
        self.collect_records(first_page, 2..=total)
    }

    /// 获取指定数量的最新开奖记录（按时间倒序）
    pub fn get_latest_n(&self, n: usize) -> Result<Vec<DrawRecord>, DaletouError> {
        let first_page = self.get_page(1)?;
        let pages_needed = ((n + 29) / 30).min(first_page.total_pages as usize) as u32;

        let mut records = self.collect_records(first_page, 2..=pages_needed)?;
        records.truncate(n);
        Ok(records)
    }

    /// 收集多页记录的内部方法，页间自动等待
    fn collect_records(
        &self,
        first_page: DrawPage,
        range: std::ops::RangeInclusive<u32>,
    ) -> Result<Vec<DrawRecord>, DaletouError> {
        let remaining_pages = range.end().saturating_sub(*range.start()) as usize;
        let mut records = first_page.records;
        records.reserve(remaining_pages * RECORDS_PER_PAGE);

        for page in range {
            std::thread::sleep(self.request_interval);
            let p = self.get_page(page)?;
            records.extend(p.records);
        }

        Ok(records)
    }

    /// 构建分页 URL
    fn page_url(base: &str, page: u32) -> String {
        if page <= 1 {
            format!("{}/dltkaijiang/", base)
        } else {
            format!("{}/dltkaijiang/{}.html", base, page)
        }
    }

    /// 发送请求并将 GB2312 响应转为 UTF-8
    fn fetch_gb2312(&self, url: &str) -> Result<String, DaletouError> {
        let resp = self.http.get(url).send()?;
        let bytes = resp.bytes()?;

        let cow = GB2312.decode_without_bom_handling_and_without_replacement(&bytes)
            .ok_or_else(|| DaletouError::EncodingError("无效的 GB2312 编码".into()))?;
        Ok(cow.into_owned())
    }

    /// 解析开奖页面
    fn parse_page(&self, html: &str, current_page: u32) -> Result<DrawPage, DaletouError> {
        let doc = Html::parse_document(html);
        let s = &self.selectors;

        let mut records = Vec::with_capacity(RECORDS_PER_PAGE);

        for line in doc.select(&s.line) {
            let issue = element_text(&line, &s.qs)
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();

            let date_text = element_text(&line, &s.date);
            let (date, weekday) = parse_date_weekday(&date_text);

            let mut red = Vec::with_capacity(5);
            for el in line.select(&s.red_ball) {
                if let Some(n) = parse_number(&el) {
                    red.push(n);
                }
            }

            let mut blue = Vec::with_capacity(2);
            for el in line.select(&s.blue_ball) {
                if let Some(n) = parse_number(&el) {
                    blue.push(n);
                }
            }

            let prize_pool = element_text(&line, &s.money);

            records.push(DrawRecord {
                issue,
                date,
                weekday,
                balls: BallSet { red, blue },
                prize_pool,
            });
        }

        let total_pages = parse_total_pages(&doc, &s.page_text).unwrap_or(current_page);

        Ok(DrawPage {
            current_page,
            total_pages,
            records,
        })
    }

    /// 加载本地缓存
    fn load_cache(&self, path: &PathBuf) -> Option<CacheData> {
        if !path.exists() {
            return None;
        }
        let content = fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// 保存记录到缓存
    fn save_cache(&self, path: &PathBuf, records: &[DrawRecord]) -> Result<(), DaletouError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| DaletouError::ParseError(format!("创建缓存目录失败: {}", e)))?;
        }

        let cache = CacheData {
            timestamp: now_secs(),
            latest_issue: records.first().map(|r| r.issue.clone()).unwrap_or_default(),
            records: records.to_vec(),
        };

        let content = serde_json::to_string_pretty(&cache)
            .map_err(|e| DaletouError::ParseError(format!("序列化缓存失败: {}", e)))?;

        fs::write(path, content)
            .map_err(|e| DaletouError::ParseError(format!("写入缓存失败: {}", e)))?;

        Ok(())
    }

    /// 检查缓存是否有效
    fn is_cache_valid(&self, cache: &CacheData) -> bool {
        let elapsed = now_secs().saturating_sub(cache.timestamp);
        elapsed < self.cache_ttl.as_secs()
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// 当前时间戳（秒）
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// 提取元素内所有文本并拼接
fn element_text(line: &scraper::ElementRef, sel: &Selector) -> String {
    line.select(sel)
        .next()
        .map(|el| el.text().collect())
        .unwrap_or_default()
}

/// 从元素文本中提取数字
fn parse_number(el: &scraper::ElementRef) -> Option<u8> {
    el.text().next()?.trim().parse().ok()
}

/// 解析 "2026-04-22(三)" 为 ("2026-04-22", "三")
fn parse_date_weekday(text: &str) -> (String, String) {
    let text = text.trim();
    if let Some(start) = text.find('(') {
        if let Some(end) = text.find(')') {
            return (text[..start].trim().to_string(), text[start + 1..end].to_string());
        }
    }
    (text.to_string(), String::new())
}

/// 从分页控件解析总页数
fn parse_total_pages(doc: &Html, sel: &Selector) -> Option<u32> {
    let text = doc.select(sel).next()?
        .first_child()
        .and_then(|n| n.value().as_text())?;

    text.find('/')
        .map(|pos| text[pos + 1..].trim())
        .and_then(|after| after.chars().take_while(|&c| c.is_ascii_digit()).collect::<String>().parse().ok())
}
