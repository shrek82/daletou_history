use encoding_rs::Encoding;
use reqwest::blocking::Client as HttpClient;
use scraper::{Html, Selector};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::db::DbClient;
use crate::error::DaletouError;
use crate::types::{BallSet, DrawPage, DrawRecord};

/// 每页固定约 30 条记录
const RECORDS_PER_PAGE: usize = 30;

/// 默认请求间隔（多页爬取时每页间隔3秒）
const DEFAULT_REQUEST_INTERVAL: Duration = Duration::from_secs(3);

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

/// 后台自动更新句柄，用于停止后台线程
pub struct AutoUpdateHandle {
    stop_signal: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl AutoUpdateHandle {
    /// 停止后台自动更新
    pub fn stop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for AutoUpdateHandle {
    fn drop(&mut self) {
        self.stop_signal.store(true, Ordering::Relaxed);
    }
}

/// 大乐透开奖信息查询客户端
pub struct Client {
    http: HttpClient,
    base_url: String,
    selectors: Selectors,
    /// 两次请求之间的最小间隔，防止被封
    request_interval: Duration,
    /// SQLite 数据库（可选）
    db: Option<DbClient>,
}

impl Client {
    /// 创建新客户端
    pub fn new() -> Self {
        Self {
            http: HttpClient::builder()
                .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                .build()
                .expect("构建 HTTP 客户端失败"),
            base_url: "https://www.cjcp.cn".to_string(),
            selectors: Selectors::new(),
            request_interval: DEFAULT_REQUEST_INTERVAL,
            db: None,
        }
    }

    /// 设置请求间隔（防封策略）
    pub fn with_request_interval(mut self, interval: Duration) -> Self {
        self.request_interval = interval;
        self
    }

    /// 启用 SQLite 存储
    ///
    /// 设置后，`get_cached_records` 会将数据持久化到数据库，
    /// 支持增量更新、按期号去重、自动修剪。
    pub fn with_db(mut self, db: DbClient) -> Self {
        self.db = Some(db);
        self
    }

    /// 启动后台自动更新第一页号码
    ///
    /// 数据库中有数据后，后台线程每隔 `crawl_interval`（默认1小时）自动
    /// 爬取第一页并更新数据库。返回的 `AutoUpdateHandle` 可用于停止线程。
    ///
    /// 如果数据库为空，线程会在启动时立即触发一次初始化爬取。
    pub fn start_auto_update(&self) -> Result<AutoUpdateHandle, DaletouError> {
        let db = self.db.as_ref().ok_or_else(|| {
            DaletouError::ParseError("未启用数据库存储，请先调用 with_db".into())
        })?;

        let stop_signal = Arc::new(AtomicBool::new(false));
        let stop_clone = stop_signal.clone();

        // 克隆必要的数据，用于后台线程
        let db_clone = DbClient::clone_for_auto_update(db);
        let base_url = self.base_url.clone();
        let interval = self.request_interval;

        let handle = std::thread::spawn(move || {
            // 数据库为空时立即触发初始化
            if db_clone.count().unwrap_or(0) == 0 {
                if let Err(e) = Self::auto_update_page_once(&db_clone, &base_url, interval) {
                    eprintln!("后台初始化爬取失败: {}", e);
                }
            }

            loop {
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(10)); // 每10秒检查一次
                if stop_clone.load(Ordering::Relaxed) {
                    break;
                }
                if db_clone.should_crawl() {
                    if let Err(e) = Self::auto_update_page_once(&db_clone, &base_url, interval) {
                        eprintln!("后台自动更新失败: {}", e);
                    }
                }
            }
        });

        Ok(AutoUpdateHandle {
            stop_signal,
            thread: Some(handle),
        })
    }

    /// 后台自动更新一页（不依赖 self）
    fn auto_update_page_once(
        db: &DbClient,
        base_url: &str,
        request_interval: Duration,
    ) -> Result<(), DaletouError> {
        let http = HttpClient::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
            .build()
            .map_err(|e| DaletouError::ParseError(format!("构建 HTTP 客户端失败: {}", e)))?;

        let url = format!("{}/dltkaijiang/", base_url);
        println!("[后台更新] 正在获取第 1 页...");
        let body = Self::fetch_url_gb2312(&http, &url)?;
        let selectors = Selectors::new();
        let page = Self::parse_page_with_selectors(&body, 1, &selectors)?;
        println!("[后台更新] 第 1 页完成，解析到 {} 条记录", page.records.len());
        db.update_latest(&page.records)?;
        println!("[后台更新] 数据库已更新");

        // 避免瞬时请求
        std::thread::sleep(request_interval);

        Ok(())
    }

    /// 发送请求并将 GB2312 响应转为 UTF-8（静态版本）
    fn fetch_url_gb2312(http: &HttpClient, url: &str) -> Result<String, DaletouError> {
        let resp = http.get(url).send()?;
        let bytes = resp.bytes()?;
        let cow = GB2312
            .decode_without_bom_handling_and_without_replacement(&bytes)
            .ok_or_else(|| DaletouError::EncodingError("无效的 GB2312 编码".into()))?;
        Ok(cow.into_owned())
    }

    /// 使用预编译选择器解析页面（静态版本）
    fn parse_page_with_selectors(
        html: &str,
        current_page: u32,
        selectors: &Selectors,
    ) -> Result<DrawPage, DaletouError> {
        let doc = Html::parse_document(html);
        let s = selectors;

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

    /// 从数据库或网络获取历史开奖记录
    ///
    /// 优先读取数据库，如果到了爬取间隔则从网络更新第一页。
    pub fn get_cached_records(&self, count: usize) -> Result<Vec<DrawRecord>, DaletouError> {
        let db = self.db.as_ref().ok_or_else(|| {
            DaletouError::ParseError("未启用数据库存储，请先调用 with_db".into())
        })?;

        let current_count = db.count().unwrap_or(0);

        // 数据库为空，需要爬满 count 条
        if current_count == 0 {
            println!("[爬取] 数据库为空，正在从网络获取数据（目标 {} 条）...", count);
            let pages_needed = ((count + 29) / 30) as u32;
            println!("[爬取] 需要获取 {} 页", pages_needed);

            let first_page = self.get_page(1)?;
            let total_pages = first_page.total_pages.min(pages_needed);
            println!("[爬取] 第 1 页完成，解析到 {} 条记录（网站共 {} 页）", first_page.records.len(), first_page.total_pages);
            db.update_latest(&first_page.records)?;

            if total_pages >= 2 {
                println!("[爬取] 还需获取 {} 页...", total_pages - 1);
                let more = self.collect_records(first_page, 2..=total_pages)?;
                db.update_latest(&more)?;
            }

            let total = db.count()?;
            println!("[爬取] 初始化完成，数据库当前共 {} 条记录", total);
            return db.get_latest_n(count);
        }

        // 数据库有数据，检查是否需要更新
        if db.should_crawl() {
            println!("[爬取] 数据已过期（当前 {} 条），正在从网络获取最新一期...", current_count);
            let first_page = self.get_page(1)?;
            println!("[爬取] 第 1 页完成，解析到 {} 条记录（共 {} 页）", first_page.records.len(), first_page.total_pages);
            db.update_latest(&first_page.records)?;
            println!("[爬取] 数据库已更新");
        }

        let records = db.get_latest_n(count)?;
        println!("[爬取] 从数据库加载 {} 条记录", records.len());
        Ok(records)
    }

    /// 获取前 N 页的所有开奖记录
    pub fn get_pages(&self, count: u32) -> Result<Vec<DrawRecord>, DaletouError> {
        println!("[爬取] 正在获取第 1 页...");
        let first_page = self.get_page(1)?;
        println!("[爬取] 第 1 页完成，解析到 {} 条记录（共 {} 页）", first_page.records.len(), first_page.total_pages);
        let total = first_page.total_pages.min(count);
        if total >= 2 {
            println!("[爬取] 还需获取 {} 页...", total - 1);
        }
        self.collect_records(first_page, 2..=total)
    }

    /// 获取指定数量的最新开奖记录（按时间倒序）
    pub fn get_latest_n(&self, n: usize) -> Result<Vec<DrawRecord>, DaletouError> {
        println!("[爬取] 正在获取第 1 页...");
        let first_page = self.get_page(1)?;
        println!("[爬取] 第 1 页完成，解析到 {} 条记录（共 {} 页）", first_page.records.len(), first_page.total_pages);
        let pages_needed = ((n + 29) / 30).min(first_page.total_pages as usize) as u32;

        if pages_needed >= 2 {
            println!("[爬取] 还需获取 {} 页，预计 {} 条记录...", pages_needed - 1, n);
        }
        let mut records = self.collect_records(first_page, 2..=pages_needed)?;
        records.truncate(n);
        println!("[爬取] 共获取 {} 条记录", records.len());
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

        let total_pages = *range.start() as usize - 1 + remaining_pages;
        let mut current = 0usize;
        for page in range {
            current += 1;
            println!("[爬取] 正在获取第 {}/{} 页...", current, total_pages);
            std::thread::sleep(self.request_interval);
            let p = self.get_page(page)?;
            println!("[爬取] 第 {} 页完成，解析到 {} 条记录", page, p.records.len());
            records.extend(p.records);
        }

        println!("[爬取] 全部完成，累计 {} 条记录", records.len());
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
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
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
