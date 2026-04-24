//! 守号方案 HTTP 处理函数

use super::picks;
use super::picks::scoring::Pick;
use super::picks::prize::PrizeStats;
use picks::{compute_prize_stats, score_pick};
use picks::stats::Stats;
use picks::prize::PrizeIndex;
use daletou::{DbClient, PersistentPick};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use tiny_http::{Header, Method, Response, StatusCode};

// ============================================================================
// 请求体
// ============================================================================

#[derive(Deserialize)]
pub struct CreatePickRequest {
    pub name: String,
    pub red: Vec<u8>,
    pub blue: Vec<u8>,
}

#[derive(Deserialize)]
pub struct UpdatePickRequest {
    pub name: Option<String>,
    pub red: Option<Vec<u8>>,
    pub blue: Option<Vec<u8>>,
}

// ============================================================================
// 响应体
// ============================================================================

#[derive(Serialize)]
pub struct PrizeStatsJson {
    #[serde(rename = "1")]
    first: u32,
    #[serde(rename = "2")]
    second: u32,
    #[serde(rename = "3")]
    third: u32,
    #[serde(rename = "4")]
    fourth: u32,
    #[serde(rename = "5")]
    fifth: u32,
    #[serde(rename = "6")]
    sixth: u32,
    #[serde(rename = "7")]
    seventh: u32,
    #[serde(rename = "8")]
    eighth: u32,
}

impl From<&PrizeStats> for PrizeStatsJson {
    fn from(ps: &PrizeStats) -> Self {
        Self {
            first: ps.counts[1],
            second: ps.counts[2],
            third: ps.counts[3],
            fourth: ps.counts[4],
            fifth: ps.counts[5],
            sixth: ps.counts[6],
            seventh: ps.counts[7],
            eighth: ps.counts[8],
        }
    }
}

#[derive(Serialize)]
pub struct PersistentListResponse {
    pub total: usize,
    pub picks: Vec<PersistentPickResponse>,
}

#[derive(Serialize)]
pub struct PersistentPickResponse {
    pub id: i64,
    pub name: String,
    pub red: Vec<u8>,
    pub blue: Vec<u8>,
    pub created_at: u64,
}

impl From<&PersistentPick> for PersistentPickResponse {
    fn from(p: &PersistentPick) -> Self {
        Self {
            id: p.id,
            name: p.name.clone(),
            red: p.red.clone(),
            blue: p.blue.clone(),
            created_at: p.created_at,
        }
    }
}

#[derive(Serialize)]
pub struct PersistentAnalysisResponse {
    pub id: i64,
    pub name: String,
    pub red: Vec<u8>,
    pub blue: Vec<u8>,
    pub score: f64,
    pub prize_stats: PrizeStatsJson,
}

#[derive(Serialize)]
pub struct HistoryEntry {
    issue: String,
    date: String,
    red: Vec<u8>,
    blue: Vec<u8>,
    red_match: usize,
    blue_match: usize,
    prize_level: Option<u8>,
}

#[derive(Serialize)]
pub struct HistoryResponse {
    pub id: i64,
    pub name: String,
    pub n_checked: usize,
    pub results: Vec<HistoryEntry>,
    pub summary: PrizeStatsJson,
}

// ============================================================================
// 工具函数
// ============================================================================

fn json_response<T: Serialize>(status: u16, data: &T) -> Response<Cursor<Vec<u8>>> {
    let body = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
    let data = body.into_bytes();
    let len = data.len();
    Response::new(
        StatusCode(status),
        vec![Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap()],
        Cursor::new(data),
        Some(len),
        None,
    )
}

fn json_error(status: u16, message: String) -> Response<Cursor<Vec<u8>>> {
    let body = serde_json::to_string(&serde_json::json!({ "error": message })).unwrap_or_else(|_| "{}".to_string());
    let data = body.into_bytes();
    let len = data.len();
    Response::new(
        StatusCode(status),
        vec![Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap()],
        Cursor::new(data),
        Some(len),
        None,
    )
}

fn validate_balls(red: &[u8], blue: &[u8]) -> Result<(), String> {
    if red.len() != 5 {
        return Err(format!("红球必须为 5 个，当前 {} 个", red.len()));
    }
    if blue.len() != 2 {
        return Err(format!("蓝球必须为 2 个，当前 {} 个", blue.len()));
    }
    for &n in red {
        if n == 0 || n > 35 {
            return Err(format!("红球 {} 超出范围（1-35）", n));
        }
    }
    for &n in blue {
        if n == 0 || n > 12 {
            return Err(format!("蓝球 {} 超出范围（1-12）", n));
        }
    }
    // 检查重复
    let mut sorted_red: [u8; 5] = [0; 5];
    sorted_red.copy_from_slice(red);
    sorted_red.sort();
    for i in 1..sorted_red.len() {
        if sorted_red[i] == sorted_red[i - 1] {
            return Err(format!("红球存在重复号码: {}", sorted_red[i]));
        }
    }
    if blue[0] == blue[1] {
        return Err(format!("蓝球存在重复号码: {}", blue[0]));
    }
    Ok(())
}

/// 根据红球、蓝球命中数判断奖项等级
fn determine_prize_level(red_match: usize, blue_match: usize) -> Option<u8> {
    match (red_match, blue_match) {
        (5, 2) => Some(1),
        (5, 1) => Some(2),
        (5, 0) => Some(3),
        (4, 2) => Some(4),
        (4, 1) => Some(5),
        (4, 0) => Some(6),
        (3, 2) => Some(7),
        (2, 2) | (3, 1) | (0, 2) | (1, 2) => Some(8),
        _ => None,
    }
}

fn count_matches(pick_balls: &[u8], draw_balls: &[u8]) -> usize {
    pick_balls.iter().filter(|&b| draw_balls.contains(b)).count()
}

// ============================================================================
// 处理函数
// ============================================================================

pub fn handle_create(db: &DbClient, body: &[u8]) -> Response<Cursor<Vec<u8>>> {
    let req: CreatePickRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return json_error(400, format!("JSON 解析失败: {}", e)),
    };

    if let Err(e) = validate_balls(&req.red, &req.blue) {
        return json_error(400, e);
    }

    let red: [u8; 5] = req.red.try_into().unwrap();
    let blue: [u8; 2] = req.blue.try_into().unwrap();

    match db.persistent_insert(&req.name, &red, &blue) {
        Ok(pick) => json_response(201, &PersistentPickResponse::from(&pick)),
        Err(e) => json_error(500, format!("数据库写入失败: {}", e)),
    }
}

pub fn handle_list(db: &DbClient) -> Response<Cursor<Vec<u8>>> {
    match db.persistent_list() {
        Ok(picks) => {
            let resp = PersistentListResponse {
                total: picks.len(),
                picks: picks.iter().map(PersistentPickResponse::from).collect(),
            };
            json_response(200, &resp)
        }
        Err(e) => json_error(500, format!("数据库查询失败: {}", e)),
    }
}

pub fn handle_get(db: &DbClient, id: i64) -> Response<Cursor<Vec<u8>>> {
    match db.persistent_get(id) {
        Ok(Some(pick)) => json_response(200, &PersistentPickResponse::from(&pick)),
        Ok(None) => json_error(404, format!("守号方案 ID={} 不存在", id)),
        Err(e) => json_error(500, format!("数据库查询失败: {}", e)),
    }
}

pub fn handle_update(db: &DbClient, id: i64, body: &[u8]) -> Response<Cursor<Vec<u8>>> {
    // 先获取现有方案
    let existing = match db.persistent_get(id) {
        Ok(Some(p)) => p,
        Ok(None) => return json_error(404, format!("守号方案 ID={} 不存在", id)),
        Err(e) => return json_error(500, format!("数据库查询失败: {}", e)),
    };

    let req: UpdatePickRequest = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return json_error(400, format!("JSON 解析失败: {}", e)),
    };

    let name = req.name.as_deref().unwrap_or(&existing.name);
    let red_vec = req.red.as_deref().unwrap_or(&existing.red);
    let blue_vec = req.blue.as_deref().unwrap_or(&existing.blue);

    if let Err(e) = validate_balls(red_vec, blue_vec) {
        return json_error(400, e);
    }

    let red: [u8; 5] = red_vec.try_into().unwrap();
    let blue: [u8; 2] = blue_vec.try_into().unwrap();

    match db.persistent_update(id, name, &red, &blue) {
        Ok(()) => {
            let updated = PersistentPick {
                id,
                name: name.to_string(),
                red: red_vec.to_vec(),
                blue: blue_vec.to_vec(),
                created_at: existing.created_at,
            };
            json_response(200, &PersistentPickResponse::from(&updated))
        }
        Err(e) => json_error(500, format!("数据库更新失败: {}", e)),
    }
}

pub fn handle_delete(db: &DbClient, id: i64) -> Response<Cursor<Vec<u8>>> {
    match db.persistent_delete(id) {
        Ok(true) => json_response(200, &serde_json::json!({ "ok": true, "id": id })),
        Ok(false) => json_error(404, format!("守号方案 ID={} 不存在", id)),
        Err(e) => json_error(500, format!("数据库删除失败: {}", e)),
    }
}

pub fn handle_analysis(db: &DbClient, id: i64, stats: &Stats, prize_index: &PrizeIndex) -> Response<Cursor<Vec<u8>>> {
    let pick = match db.persistent_get(id) {
        Ok(Some(p)) => p,
        Ok(None) => return json_error(404, format!("守号方案 ID={} 不存在", id)),
        Err(e) => return json_error(500, format!("数据库查询失败: {}", e)),
    };

    let red: [u8; 5] = pick.red.clone().try_into().unwrap();
    let blue: [u8; 2] = pick.blue.clone().try_into().unwrap();
    let label: &'static str = Box::leak(pick.name.clone().into_boxed_str());
    let pick_obj = Pick::new(pick.red.clone(), pick.blue.clone(), label);
    let score = score_pick(&pick_obj, stats);
    let prize_stats = compute_prize_stats(prize_index, &red, &blue);

    json_response(200, &PersistentAnalysisResponse {
        id: pick.id,
        name: pick.name,
        red: pick.red,
        blue: pick.blue,
        score,
        prize_stats: (&prize_stats).into(),
    })
}

pub fn handle_history(db: &DbClient, id: i64, n: usize) -> Response<Cursor<Vec<u8>>> {
    let pick = match db.persistent_get(id) {
        Ok(Some(p)) => p,
        Ok(None) => return json_error(404, format!("守号方案 ID={} 不存在", id)),
        Err(e) => return json_error(500, format!("数据库查询失败: {}", e)),
    };

    let records = match db.get_latest_n(n) {
        Ok(r) => r,
        Err(e) => return json_error(500, format!("数据库查询失败: {}", e)),
    };

    let mut results = Vec::with_capacity(records.len());
    let mut summary_counts = [0u32; 9];

    for record in &records {
        let red_match = count_matches(&pick.red, &record.balls.red);
        let blue_match = count_matches(&pick.blue, &record.balls.blue);
        let prize_level = determine_prize_level(red_match, blue_match);

        if let Some(level) = prize_level {
            summary_counts[level as usize] += 1;
        }

        results.push(HistoryEntry {
            issue: record.issue.clone(),
            date: record.date.clone(),
            red: record.balls.red.clone(),
            blue: record.balls.blue.clone(),
            red_match,
            blue_match,
            prize_level,
        });
    }

    let summary = PrizeStatsJson {
        first: summary_counts[1],
        second: summary_counts[2],
        third: summary_counts[3],
        fourth: summary_counts[4],
        fifth: summary_counts[5],
        sixth: summary_counts[6],
        seventh: summary_counts[7],
        eighth: summary_counts[8],
    };

    json_response(200, &HistoryResponse {
        id: pick.id,
        name: pick.name,
        n_checked: results.len(),
        results,
        summary,
    })
}

// ============================================================================
// 路由分发
// ============================================================================

pub fn handle_persistent(
    url: &str,
    method: &Method,
    body: &[u8],
    db: &DbClient,
    stats: Option<&Stats>,
    prize_index: Option<&PrizeIndex>,
) -> Response<Cursor<Vec<u8>>> {
    let path = url.split('?').next().unwrap_or("/");
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // /api/persistent (无 ID)
    if parts.len() == 2 {
        match *method {
            Method::Post => handle_create(db, body),
            Method::Get => handle_list(db),
            _ => json_error(405, "仅支持 GET、POST".to_string()),
        }
    }
    // /api/persistent/<id>
    else if parts.len() == 3 {
        let id = match parts[2].parse::<i64>() {
            Ok(id) => id,
            Err(_) => return json_error(400, "ID 必须为整数".to_string()),
        };
        match *method {
            Method::Get => handle_get(db, id),
            Method::Put => handle_update(db, id, body),
            Method::Delete => handle_delete(db, id),
            _ => json_error(405, "仅支持 GET、PUT、DELETE".to_string()),
        }
    }
    // /api/persistent/<id>/analysis
    else if parts.len() == 4 && parts[3] == "analysis" {
        let id = match parts[2].parse::<i64>() {
            Ok(id) => id,
            Err(_) => return json_error(400, "ID 必须为整数".to_string()),
        };
        match *method {
            Method::Get => {
                match (stats, prize_index) {
                    (Some(s), Some(p)) => handle_analysis(db, id, s, p),
                    _ => json_error(500, "分析功能需要 Stats 和 PrizeIndex".to_string()),
                }
            }
            _ => json_error(405, "仅支持 GET".to_string()),
        }
    }
    // /api/persistent/<id>/history?n=30
    else if parts.len() == 4 && parts[3] == "history" {
        let id = match parts[2].parse::<i64>() {
            Ok(id) => id,
            Err(_) => return json_error(400, "ID 必须为整数".to_string()),
        };
        let mut n: usize = 30;
        if let Some(query) = url.split('?').nth(1) {
            for param in query.split('&') {
                if let Some((key, val)) = param.split_once('=') {
                    if key == "n" {
                        if let Ok(v) = val.parse::<usize>() {
                            n = v;
                        }
                    }
                }
            }
        }
        match *method {
            Method::Get => handle_history(db, id, n),
            _ => json_error(405, "仅支持 GET".to_string()),
        }
    } else {
        json_error(404, "未找到该路径。可用: /api/persistent, /api/persistent/<id>, /api/persistent/<id>/analysis, /api/persistent/<id>/history?n=30".to_string())
    }
}
