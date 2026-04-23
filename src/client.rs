use encoding_rs::Encoding;
use reqwest::blocking::Client as HttpClient;
use scraper::{Html, Selector};

use crate::error::DaletouError;
use crate::types::{BallSet, DrawPage, DrawRecord};

/// GB2312 编码实例
const GB2312: &'static Encoding = encoding_rs::GB18030;

/// 大乐透开奖信息查询客户端
pub struct Client {
    http: HttpClient,
    base_url: String,
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
        }
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

    /// 获取前 N 页的所有开奖记录
    pub fn get_pages(&self, count: u32) -> Result<Vec<DrawRecord>, DaletouError> {
        let first_page = self.get_page(1)?;
        let total = first_page.total_pages.min(count);
        let mut records = first_page.records;

        for page in 2..=total {
            let p = self.get_page(page)?;
            records.extend(p.records);
        }

        Ok(records)
    }

    /// 获取指定数量的最新开奖记录（按时间倒序）
    pub fn get_latest_n(&self, n: usize) -> Result<Vec<DrawRecord>, DaletouError> {
        let first_page = self.get_page(1)?;
        let total_pages = first_page.total_pages;
        // 每页约30条，计算需要请求的页数
        let pages_needed = ((n + 29) / 30).min(total_pages as usize) as u32;

        let mut records = first_page.records;
        for page in 2..=pages_needed {
            let p = self.get_page(page)?;
            records.extend(p.records);
        }

        records.truncate(n);
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

        // GB18030 解码（兼容 GB2312）
        let cow = GB2312.decode_without_bom_handling_and_without_replacement(&bytes)
            .ok_or_else(|| DaletouError::EncodingError("无效的 GB2312 编码".into()))?;
        Ok(cow.into_owned())
    }

    /// 解析开奖页面
    fn parse_page(&self, html: &str, current_page: u32) -> Result<DrawPage, DaletouError> {
        let doc = Html::parse_document(html);

        // 解析开奖记录行
        let line_sel = Selector::parse("div.table-line")
            .map_err(|e| DaletouError::ParseError(format!("选择器解析失败: {}", e)))?;
        let qs_sel = Selector::parse("div.qs")
            .map_err(|e| DaletouError::ParseError(format!("选择器解析失败: {}", e)))?;
        let date_sel = Selector::parse("div.date")
            .map_err(|e| DaletouError::ParseError(format!("选择器解析失败: {}", e)))?;
        let red_sel = Selector::parse("div.red-ball")
            .map_err(|e| DaletouError::ParseError(format!("选择器解析失败: {}", e)))?;
        let blue_sel = Selector::parse("div.blue-ball")
            .map_err(|e| DaletouError::ParseError(format!("选择器解析失败: {}", e)))?;
        let money_sel = Selector::parse("div.money")
            .map_err(|e| DaletouError::ParseError(format!("选择器解析失败: {}", e)))?;

        let mut records = Vec::new();

        for line in doc.select(&line_sel) {
            // 期号: 从 "大乐透第2026043期开奖结果" 提取数字
            let qs_text = line
                .select(&qs_sel)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default();
            let issue = extract_issue(&qs_text);

            // 日期和星期: "2026-04-22(三)"
            let date_text = line
                .select(&date_sel)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default();
            let (date, weekday) = parse_date_weekday(&date_text);

            // 红球
            let red: Vec<u8> = line
                .select(&red_sel)
                .filter_map(|el| el.text().collect::<String>().trim().parse::<u8>().ok())
                .collect();

            // 蓝球
            let blue: Vec<u8> = line
                .select(&blue_sel)
                .filter_map(|el| el.text().collect::<String>().trim().parse::<u8>().ok())
                .collect();

            // 奖池
            let prize_pool = line
                .select(&money_sel)
                .next()
                .map(|el| el.text().collect::<String>())
                .unwrap_or_default();

            records.push(DrawRecord {
                issue,
                date,
                weekday,
                balls: BallSet { red, blue },
                prize_pool,
            });
        }

        // 解析总页数（从分页控件中获取）
        let total_pages = parse_total_pages(&doc).unwrap_or(current_page);

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

/// 从 "大乐透第2026043期开奖结果" 提取期号 "2026043"
fn extract_issue(text: &str) -> String {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    digits
}

/// 解析 "2026-04-22(三)" 为 ("2026-04-22", "三")
fn parse_date_weekday(text: &str) -> (String, String) {
    let text = text.trim();
    if let Some(start) = text.find('(') {
        if let Some(end) = text.find(')') {
            let date = text[..start].trim().to_string();
            let weekday = text[start + 1..end].to_string();
            return (date, weekday);
        }
    }
    (text.to_string(), String::new())
}

/// 从分页控件解析总页数
fn parse_total_pages(doc: &Html) -> Option<u32> {
    let page_text_sel = Selector::parse("div.page-text").ok()?;
    let text = doc.select(&page_text_sel).next()?
        .first_child()
        .and_then(|n| n.value().as_text())?;

    // 文本格式如 "2/96"，提取斜杠后的数字
    if let Some(pos) = text.find('/') {
        let after = text[pos + 1..].trim();
        let digits: String = after.chars().take_while(|&c| c.is_ascii_digit()).collect();
        digits.parse::<u32>().ok()
    } else {
        None
    }
}
