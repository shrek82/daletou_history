mod picks;

use picks::{analyze, build_prize_index, compute_prize_stats, generate_picks, is_completely_random, print_analysis, score_pick};
use picks::scoring::Pick;
use picks::prize::PrizeStats;
use serde::Serialize;
use std::io::Cursor;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tiny_http::{Server, Response, Header, StatusCode};
use daletou::Client;

/// 固定号码：02 22 30 33 34 + 08 12
const FIXED_RED: [u8; 5] = [2, 22, 30, 33, 34];
const FIXED_BLUE: [u8; 2] = [8, 12];

/// 策略名映射
const STRATEGY_MAP: &[(&str, &str)] = &[
    ("hot", "纯热号"),
    ("hot_cold", "冷热混合"),
    ("zone", "区间均衡"),
    ("sum", "和值约束"),
    ("tail", "同尾约束"),
    ("consecutive", "连号策略"),
    ("weighted_random_a", "加权随机A"),
    ("weighted_random_b", "加权随机B"),
    ("random_a", "完全随机A"),
    ("random_b", "完全随机B"),
    ("random_c", "完全随机C"),
    ("random_d", "完全随机D"),
    ("random_e", "完全随机E"),
    ("fixed", "固定号码"),
];

fn resolve_strategy(key: &str) -> Option<&'static str> {
    STRATEGY_MAP.iter().find(|&&(k, _)| k == key).map(|&(_, v)| v)
}

// ============================================================================
// CLI参数解析
// ============================================================================

struct CliArgs {
    server: bool,
    port: u16,
    /// 数据库初始化时最多爬取多少条记录，默认 365
    max_records: usize,
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut server = false;
    let mut port: u16 = 8888;
    let mut max_records: usize = 365;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--server" => server = true,
            "--port" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(p) = val.parse::<u16>() {
                        port = p;
                        i += 1;
                    }
                }
            }
            "--max-records" => {
                if let Some(val) = args.get(i + 1) {
                    if let Ok(n) = val.parse::<usize>() {
                        max_records = n;
                        i += 1;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    CliArgs { server, port, max_records }
}

// ============================================================================
// JSON响应结构体
// ============================================================================

#[derive(Serialize)]
struct PrizeStatsJson {
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
struct FixedAnalysisJson {
    sum: u8,
    odd_even: [usize; 2],
    big_small: [usize; 2],
    zones: [usize; 3],
    tail_dupes: usize,
}

#[derive(Serialize)]
struct PickJson {
    index: usize,
    red: Vec<u8>,
    blue: Vec<u8>,
    score: Option<f64>,
    label: String,
    prize_stats: PrizeStatsJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    analysis: Option<FixedAnalysisJson>,
}

#[derive(Serialize)]
struct PicksResponse {
    seed: u64,
    records_count: usize,
    strategies: Vec<PickJson>,
    random: Vec<PickJson>,
    fixed: PickJson,
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    records: usize,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Serialize)]
struct DrawRecordJson {
    issue: String,
    date: String,
    weekday: String,
    red: Vec<u8>,
    blue: Vec<u8>,
    prize_pool: String,
}

#[derive(Serialize)]
struct DrawsResponse {
    total: usize,
    page: u32,
    page_size: usize,
    total_pages: usize,
    records: Vec<DrawRecordJson>,
}

// ============================================================================
// 终端输出函数
// ============================================================================

fn print_terminal_output(picks: &[Pick], stats: &picks::stats::Stats, prize_index: &picks::prize::PrizeIndex, seed: u64) {
    // AI策略推荐
    println!("\n===== AI 推荐方案（8组：6组策略 + 2组加权随机，种子={}）=====", seed);
    println!("  方案  红球            蓝球    评分  标签           中奖统计");

    for (i, pick) in picks.iter().enumerate() {
        if is_completely_random(pick.label) {
            break;
        }
        let score = score_pick(pick, stats);
        let prize_stats = compute_prize_stats(
            prize_index,
            &pick.red.clone().try_into().expect("红球数量应为5"),
            &pick.blue.clone().try_into().expect("蓝球数量应为2"),
        );
        print_pick_line(i + 1, pick, Some(score), Some(&prize_stats));
    }

    // 完全随机
    println!("\n===== 完全随机（5组，基于操作系统级加密随机源，评分不适用）=====");
    println!("  方案  红球            蓝球    标签           中奖统计");

    let ai_count = picks.iter().position(|p| is_completely_random(p.label)).unwrap_or(picks.len());
    for (i, pick) in picks.iter().skip(ai_count).enumerate() {
        let prize_stats = compute_prize_stats(
            prize_index,
            &pick.red.clone().try_into().expect("红球数量应为5"),
            &pick.blue.clone().try_into().expect("蓝球数量应为2"),
        );
        print_pick_line(i + 1, pick, None, Some(&prize_stats));
    }

    // 固定号码分析
    print_fixed_number_analysis(stats, prize_index);
}

fn print_pick_line(index: usize, pick: &Pick, score: Option<f64>, prize_stats: Option<&PrizeStats>) {
    print!("  {:>4}  ", index);
    for n in &pick.red {
        print!("{:02} ", n);
    }
    print!(" + ");
    for n in &pick.blue {
        print!("{:02} ", n);
    }
    if let Some(s) = score {
        print!("  {:>5.1}  {:<10}", s, pick.label);
    } else {
        print!("   N/A  {:<10}", pick.label);
    }
    if let Some(ps) = prize_stats {
        print!("  {}", ps.display());
    }
    println!();
}

fn print_fixed_number_analysis(stats: &picks::stats::Stats, prize_index: &picks::prize::PrizeIndex) {
    use picks::score_pick;

    let pick = Pick::new(
        FIXED_RED.to_vec(),
        FIXED_BLUE.to_vec(),
        "固定号码",
    );

    let score = score_pick(&pick, stats);
    let prize_stats = compute_prize_stats(prize_index, &FIXED_RED, &FIXED_BLUE);

    let sum: u8 = FIXED_RED.iter().sum();
    let odd_count = FIXED_RED.iter().filter(|&&n| n % 2 == 1).count();
    let big_count = FIXED_RED.iter().filter(|&&n| n >= 18).count();

    let mut zones = [0usize; 3];
    for &n in &FIXED_RED {
        if n <= 12 { zones[0] += 1; }
        else if n <= 24 { zones[1] += 1; }
        else { zones[2] += 1; }
    }

    println!("\n===== 固定号码分析 =====");
    println!("  号码: {:02} {:02} {:02} {:02} {:02} + {:02} {:02}  评分: {:.1}",
        FIXED_RED[0], FIXED_RED[1], FIXED_RED[2], FIXED_RED[3], FIXED_RED[4],
        FIXED_BLUE[0], FIXED_BLUE[1], score);
    println!("  中奖统计: {}", prize_stats.display());
    println!("  和值: {}  奇偶比: {}/{}  大小比: {}/{}",
        sum, odd_count, 5 - odd_count, big_count, 5 - big_count);
    println!("  区间分布: {}-{}-{}  同尾组数: {}",
        zones[0], zones[1], zones[2], count_tail_dupes(&FIXED_RED));
}

fn count_tail_dupes(red: &[u8; 5]) -> usize {
    let mut tails = *red;
    tails.sort();
    let mut dupes = 0;
    for i in 1..tails.len() {
        if tails[i] % 10 == tails[i - 1] % 10 {
            dupes += 1;
        }
    }
    dupes
}

// ============================================================================
// 构建JSON响应数据
// ============================================================================

fn build_pick_json(index: usize, pick: &Pick, stats: &picks::stats::Stats, prize_index: &picks::prize::PrizeIndex, include_analysis: bool) -> PickJson {
    let score = if is_completely_random(pick.label) {
        None
    } else {
        Some(score_pick(pick, stats))
    };
    let prize_stats = compute_prize_stats(
        prize_index,
        &pick.red.clone().try_into().expect("红球数量应为5"),
        &pick.blue.clone().try_into().expect("蓝球数量应为2"),
    );

    let analysis = if include_analysis {
        let sum: u8 = pick.red.iter().sum();
        let odd_count = pick.red.iter().filter(|&&n| n % 2 == 1).count();
        let big_count = pick.red.iter().filter(|&&n| n >= 18).count();
        let mut zones = [0usize; 3];
        for &n in &pick.red {
            if n <= 12 { zones[0] += 1; }
            else if n <= 24 { zones[1] += 1; }
            else { zones[2] += 1; }
        }
        Some(FixedAnalysisJson {
            sum,
            odd_even: [odd_count, 5 - odd_count],
            big_small: [big_count, 5 - big_count],
            zones,
            tail_dupes: count_tail_dupes_vec(&pick.red),
        })
    } else {
        None
    };

    PickJson {
        index,
        red: pick.red.clone(),
        blue: pick.blue.clone(),
        score,
        label: pick.label.to_string(),
        prize_stats: (&prize_stats).into(),
        analysis,
    }
}

fn count_tail_dupes_vec(red: &[u8]) -> usize {
    let mut tails: Vec<u8> = red.iter().copied().collect();
    tails.sort();
    let mut dupes = 0;
    for i in 1..tails.len() {
        if tails[i] % 10 == tails[i - 1] % 10 {
            dupes += 1;
        }
    }
    dupes
}

fn build_picks_response(picks: &[Pick], stats: &picks::stats::Stats, prize_index: &picks::prize::PrizeIndex, seed: u64, records_count: usize) -> PicksResponse {
    let mut strategies = Vec::new();
    let mut random = Vec::new();

    let ai_count = picks.iter().position(|p| is_completely_random(p.label)).unwrap_or(picks.len());

    for (i, pick) in picks.iter().enumerate().take(ai_count) {
        strategies.push(build_pick_json(i + 1, pick, stats, prize_index, false));
    }
    for (i, pick) in picks.iter().skip(ai_count).enumerate() {
        random.push(build_pick_json(i + 1, pick, stats, prize_index, false));
    }

    // 固定号码
    let fixed_pick = Pick::new(FIXED_RED.to_vec(), FIXED_BLUE.to_vec(), "固定号码");
    let fixed_json = build_pick_json(0, &fixed_pick, stats, prize_index, true);

    PicksResponse {
        seed,
        records_count,
        strategies,
        random,
        fixed: fixed_json,
    }
}

// ============================================================================
// HTTP服务器
// ============================================================================

fn run_server(port: u16, stats: picks::stats::Stats, prize_index: picks::prize::PrizeIndex, records_count: usize, db: daletou::DbClient) {
    use std::sync::{Arc, Mutex};

    // 共享数据
    let server_state = Arc::new(Mutex::new((stats, prize_index, records_count)));
    let db_state = Arc::new(Mutex::new(db));

    let server = Server::http(format!("0.0.0.0:{}", port))
        .unwrap_or_else(|e| {
            eprintln!("启动HTTP服务失败: {}", e);
            std::process::exit(1);
        });

    println!("HTTP服务已启动: http://0.0.0.0:{}", port);
    println!("  GET /api/picks              - 返回全部推荐");
    println!("  GET /api/pick?strategy=hot  - 返回指定策略");
    println!("  GET /api/draws?page=1&page_size=20 - 分页查询开奖记录");
    println!("  GET /api/draws?issue=26001           - 按期号查询");
    println!("  GET /health                 - 健康检查");
    println!("  Ctrl+C 停止服务");

    for request in server.incoming_requests() {
        let state = server_state.lock().unwrap();
        let (stats, prize_index, records_count) = &*state;
        let db_guard = db_state.lock().unwrap();

        let url = request.url().to_string();
        let response = handle_request(&url, stats, prize_index, *records_count, &db_guard);

        let _ = request.respond(response);
    }
}

fn handle_request(url: &str, stats: &picks::stats::Stats, prize_index: &picks::prize::PrizeIndex, records_count: usize, db: &daletou::DbClient) -> Response<Cursor<Vec<u8>>> {
    let path = url.split('?').next().unwrap_or("/");

    match path {
        "/health" => json_response(200, &HealthResponse {
            status: "ok",
            records: records_count,
        }),
        "/api/draws" => handle_draws_query(db, url),
        "/api/picks" => {
            let seed = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let picks = generate_picks(stats, seed);
            json_response(200, &build_picks_response(&picks, stats, prize_index, seed, records_count))
        }
        "/api/pick" => {
            // 解析query参数
            let query = url.split('?').nth(1).unwrap_or("");
            let strategy_key = query
                .split('&')
                .find(|p| p.starts_with("strategy="))
                .and_then(|p| p.strip_prefix("strategy="));

            match strategy_key {
                Some(key) => {
                    // fixed 策略特殊处理（不在 generate_picks 列表中）
                    if key == "fixed" {
                        let pick = Pick::new(
                            FIXED_RED.to_vec(),
                            FIXED_BLUE.to_vec(),
                            "固定号码",
                        );
                        let pj = build_pick_json(1, &pick, stats, prize_index, true);
                        return json_response(200, &pj);
                    }

                    match resolve_strategy(key) {
                        Some(label) => {
                            let seed = SystemTime::now()
                                .duration_since(SystemTime::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs();
                            let picks = generate_picks(stats, seed);
                            let pick = picks.iter().find(|p| p.label == label);

                            match pick {
                                Some(p) => {
                                    let pj = build_pick_json(1, p, stats, prize_index, key == "fixed");
                                    json_response(200, &pj)
                                }
                                None => json_error(404, format!("策略 '{}' 不存在", key)),
                            }
                        }
                        None => json_error(404, format!("未知策略: {}\n可用策略: hot, hot_cold, zone, sum, tail, consecutive, weighted_random_a, weighted_random_b, random_a~e, fixed", key)),
                    }
                }
                None => json_error(400, "缺少 strategy 参数，例如: /api/pick?strategy=hot".to_string()),
            }
        }
        _ => json_error(404, "未找到该路径。可用: /api/picks, /api/pick?strategy=<name>, /health".to_string()),
    }
}

fn handle_draws_query(db: &daletou::DbClient, url: &str) -> Response<Cursor<Vec<u8>>> {
    let query = url.split('?').nth(1).unwrap_or("");
    let mut page: u32 = 1;
    let mut page_size: usize = 20;
    let mut issue: Option<String> = None;

    for param in query.split('&') {
        if let Some((key, val)) = param.split_once('=') {
            match key {
                "page" => { if let Ok(p) = val.parse() { page = p; } }
                "page_size" => { if let Ok(s) = val.parse() { page_size = s; } }
                "issue" => { issue = Some(val.to_string()); }
                _ => {}
            }
        }
    }

    // 按期号查询优先
    if let Some(issue_str) = issue {
        match db.get_by_issue(&issue_str) {
            Ok(Some(record)) => {
                let rec = DrawRecordJson {
                    issue: record.issue,
                    date: record.date,
                    weekday: record.weekday,
                    red: record.balls.red,
                    blue: record.balls.blue,
                    prize_pool: record.prize_pool,
                };
                return json_response(200, &rec);
            }
            Ok(None) => return json_error(404, format!("未找到期号为 '{}' 的记录", issue_str)),
            Err(e) => return json_error(500, format!("数据库查询失败: {}", e)),
        }
    }

    // 分页校验
    if page == 0 {
        return json_error(400, "page 必须 >= 1".to_string());
    }
    if page_size == 0 || page_size > 100 {
        return json_error(400, "page_size 必须在 1~100 之间".to_string());
    }

    match db.get_page_records(page, page_size) {
        Ok((total, records)) => {
            let total_pages = (total + page_size - 1) / page_size;
            let recs: Vec<DrawRecordJson> = records
                .into_iter()
                .map(|r| DrawRecordJson {
                    issue: r.issue,
                    date: r.date,
                    weekday: r.weekday,
                    red: r.balls.red,
                    blue: r.balls.blue,
                    prize_pool: r.prize_pool,
                })
                .collect();
            json_response(200, &DrawsResponse {
                total,
                page,
                page_size,
                total_pages,
                records: recs,
            })
        }
        Err(e) => json_error(500, format!("数据库查询失败: {}", e)),
    }
}

fn json_response<T: Serialize>(status: u16, data: &T) -> Response<Cursor<Vec<u8>>> {
    let body = serde_json::to_string_pretty(data).unwrap_or_else(|_| "{}".to_string());
    let data = body.into_bytes();
    let len = data.len();

    Response::new(
        StatusCode(status),
        vec![
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
        ],
        Cursor::new(data),
        Some(len),
        None,
    )
}

fn json_error(status: u16, message: String) -> Response<Cursor<Vec<u8>>> {
    let body = serde_json::to_string(&ErrorResponse { error: message }).unwrap_or_else(|_| "{}".to_string());
    let data = body.into_bytes();
    let len = data.len();

    Response::new(
        StatusCode(status),
        vec![
            Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap(),
        ],
        Cursor::new(data),
        Some(len),
        None,
    )
}

// ============================================================================
// 主入口
// ============================================================================

fn main() {
    let args = parse_args();

    let db_path = PathBuf::from("data/daletou.db");

    let db = match daletou::DbClient::new(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("初始化数据库失败: {}", e);
            return;
        }
    };
    // 应用启动参数指定的 max_records
    let config = daletou::DbConfig {
        crawl_interval: std::time::Duration::from_secs(3600),
        max_records: args.max_records,
    };
    let db = db.with_config(config);

    let client = Client::new()
        .with_db(db.clone())
        .with_request_interval(Duration::from_secs(3));

    println!("正在获取最近 {} 期开奖数据...", args.max_records);
    let records = match client.get_cached_records(args.max_records) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("获取数据失败: {}", e);
            return;
        }
    };
    println!("成功获取 {} 条记录", records.len());

    // 启动后台自动更新（每隔1小时自动更新第一页）
    let _auto_update = match client.start_auto_update() {
        Ok(handle) => {
            println!("后台自动更新已启动（间隔1小时）");
            Some(handle)
        }
        Err(e) => {
            eprintln!("启动后台更新失败: {}", e);
            None
        }
    };

    let stats = analyze(&records);
    let prize_index = build_prize_index(&records);

    if args.server {
        // HTTP服务器模式
        run_server(args.port, stats, prize_index, records.len(), db);
    } else {
        // 终端打印模式
        print_analysis(&stats, &records);

        let seed = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let picks = generate_picks(&stats, seed);
        print_terminal_output(&picks, &stats, &prize_index, seed);
    }
}
