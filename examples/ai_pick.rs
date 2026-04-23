use daletou::{Client, DrawRecord};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// 红球范围 1-35，蓝球范围 1-12
const RED_MAX: u8 = 35;
const BLUE_MAX: u8 = 12;
const RED_COUNT: usize = 5;
const BLUE_COUNT: usize = 2;

/// 红球区间划分
const ZONE1_END: u8 = 12;
const ZONE2_END: u8 = 24;

/// 加权权重
const WEIGHT_RECENT10: f64 = 5.0;
const WEIGHT_RECENT20: f64 = 3.0;
const WEIGHT_RECENT50: f64 = 2.0;
const WEIGHT_ALL: f64 = 1.0;

fn main() {
    let cache_path = PathBuf::from("/tmp/daletou_cache.json");

    let client = Client::new()
        .with_cache_path(cache_path)
        .with_cache_ttl(Duration::from_secs(86400)) // 缓存24小时
        .with_request_interval(Duration::from_secs(2));

    // 获取最近 300 条记录（约需 10 页）
    println!("正在获取最近 300 期开奖数据...");
    let records = match client.get_cached_records(300) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("获取数据失败: {}", e);
            return;
        }
    };
    println!("成功获取 {} 条记录\n", records.len());

    // 分析数据
    let stats = analyze(&records);

    // 展示分析结果
    print_analysis(&stats, &records);

    // 生成推荐号码
    println!("\n===== AI 推荐号码 =====");
    let picks = generate_picks(&stats);
    for (i, pick) in picks.iter().enumerate() {
        print!("方案 {}: ", i + 1);
        for n in &pick.red {
            print!("{:02} ", n);
        }
        print!("+ ");
        for n in &pick.blue {
            print!("{:02} ", n);
        }
        println!();
    }
}

/// 统计分析结果
struct Stats {
    /// 红球全量频率
    red_freq: HashMap<u8, u32>,
    /// 蓝球全量频率
    blue_freq: HashMap<u8, u32>,
    /// 红球各窗口频率 [近10, 近20, 近50]
    red_window_freq: [HashMap<u8, u32>; 3],
    /// 蓝球各窗口频率 [近10, 近20, 近50]
    blue_window_freq: [HashMap<u8, u32>; 3],
    /// 红球加权得分（近10期x5 + 近20期x3 + 近50期x2 + 全量x1）
    red_weighted: HashMap<u8, f64>,
    /// 蓝球加权得分
    blue_weighted: HashMap<u8, f64>,
    /// 红球遗漏期数（索引=号码-1）
    red_omission: Vec<u32>,
    /// 蓝球遗漏期数
    blue_omission: Vec<u32>,
    /// 区间分布 [一区均值, 二区均值, 三区均值]
    zone_avg: [f64; 3],
    /// 和值统计 (均值, 标准差)
    sum_avg: f64,
    sum_stddev: f64,
    /// 连号出现率
    consecutive_rate: f64,
    /// 平均每期同尾重复数
    avg_tail_duplicates: f64,
    /// 奇偶比（红球奇数占比）
    odd_ratio: f64,
    /// 大小比（红球大数18-35占比）
    big_ratio: f64,
    /// 蓝球近期热号（近50期频率排序）
    blue_recent_hot: Vec<u8>,
}

/// 一组推荐号码
struct Pick {
    red: Vec<u8>,
    blue: Vec<u8>,
}

fn analyze(records: &[DrawRecord]) -> Stats {
    let mut red_freq: HashMap<u8, u32> = HashMap::new();
    let mut blue_freq: HashMap<u8, u32> = HashMap::new();

    // 全量统计
    for r in records {
        for &n in &r.balls.red {
            *red_freq.entry(n).or_insert(0) += 1;
        }
        for &n in &r.balls.blue {
            *blue_freq.entry(n).or_insert(0) += 1;
        }
    }

    // 最近50期热度
    let recent_n = records.len().min(50);
    let mut red_recent: HashMap<u8, u32> = HashMap::new();
    let mut blue_recent: HashMap<u8, u32> = HashMap::new();
    for r in &records[..recent_n] {
        for &n in &r.balls.red {
            *red_recent.entry(n).or_insert(0) += 1;
        }
        for &n in &r.balls.blue {
            *blue_recent.entry(n).or_insert(0) += 1;
        }
    }

    // 按近期频率排序，取最热
    let mut red_hot: Vec<(u8, u32)> = red_recent.into_iter().collect();
    red_hot.sort_by_key(|&(_, v)| -(v as i32));
    let red_hot: Vec<u8> = red_hot.into_iter().map(|(k, _)| k).take(10).collect();

    let mut blue_hot: Vec<(u8, u32)> = blue_recent.into_iter().collect();
    blue_hot.sort_by_key(|&(_, v)| -(v as i32));
    let blue_hot: Vec<u8> = blue_hot.into_iter().map(|(k, _)| k).take(5).collect();

    // 近100期未出现的冷号
    let lookback = records.len().min(100);
    let mut red_seen = vec![false; (RED_MAX + 1) as usize];
    let mut blue_seen = vec![false; (BLUE_MAX + 1) as usize];
    for r in &records[..lookback] {
        for &n in &r.balls.red {
            if n <= RED_MAX { red_seen[n as usize] = true; }
        }
        for &n in &r.balls.blue {
            if n <= BLUE_MAX { blue_seen[n as usize] = true; }
        }
    }

    let red_cold: Vec<u8> = (1..=RED_MAX).filter(|&n| !red_seen[n as usize]).collect();
    let blue_cold: Vec<u8> = (1..=BLUE_MAX).filter(|&n| !blue_seen[n as usize]).collect();

    // 奇偶比统计
    let mut odd_count = 0u32;
    let mut total_red = 0u32;
    for r in records {
        for &n in &r.balls.red {
            if n % 2 == 1 { odd_count += 1; }
            total_red += 1;
        }
    }
    let odd_even_ratio = odd_count as f64 / total_red as f64;

    // 大小比统计
    let mut big_count = 0u32;
    for r in records {
        for &n in &r.balls.red {
            if n >= 18 { big_count += 1; }
        }
    }
    let big_small_ratio = big_count as f64 / total_red as f64;

    Stats {
        red_freq,
        blue_freq,
        red_hot,
        blue_hot,
        red_cold,
        blue_cold,
        odd_even_ratio,
        big_small_ratio,
    }
}

fn print_analysis(stats: &Stats, records: &[DrawRecord]) {
    println!("===== 数据分析（{} 期）=====", records.len());

    // 红球频率 TOP 10
    println!("\n红球出现频率 TOP 10:");
    let mut red_sorted: Vec<(u8, u32)> = stats.red_freq.iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    red_sorted.sort_by_key(|&(_, v)| -(v as i32));
    for (i, (ball, count)) in red_sorted.iter().take(10).enumerate() {
        let pct = *count as f64 / records.len() as f64 * 100.0;
        println!("  {:2}. {:02} 出现 {:3} 次 ({:.1}%)", i + 1, ball, count, pct);
    }

    // 蓝球频率
    println!("\n蓝球出现频率:");
    let mut blue_sorted: Vec<(u8, u32)> = stats.blue_freq.iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    blue_sorted.sort_by_key(|&(_, v)| -(v as i32));
    for (i, (ball, count)) in blue_sorted.iter().enumerate() {
        let pct = *count as f64 / records.len() as f64 * 100.0;
        println!("  {:2}. {:02} 出现 {:3} 次 ({:.1}%)", i + 1, ball, count, pct);
    }

    // 热号
    println!("\n近期热号（近50期）:");
    print!("  红球: ");
    for n in &stats.red_hot { print!("{:02} ", n); }
    print!("\n  蓝球: ");
    for n in &stats.blue_hot { print!("{:02} ", n); }
    println!();

    // 冷号
    println!("\n冷门号码（近100期未出现）:");
    print!("  红球: ");
    if stats.red_cold.is_empty() {
        print!("无");
    } else {
        for n in &stats.red_cold { print!("{:02} ", n); }
    }
    print!("\n  蓝球: ");
    if stats.blue_cold.is_empty() {
        print!("无");
    } else {
        for n in &stats.blue_cold { print!("{:02} ", n); }
    }
    println!();

    // 奇偶比
    println!("\n奇偶比: 奇数 {:.1}% / 偶数 {:.1}%",
        stats.odd_even_ratio * 100.0,
        (1.0 - stats.odd_even_ratio) * 100.0);

    // 大小比
    println!("大小比: 大(18-35) {:.1}% / 小(1-17) {:.1}%",
        stats.big_small_ratio * 100.0,
        (1.0 - stats.big_small_ratio) * 100.0);
}

/// 生成多组推荐号码
fn generate_picks(stats: &Stats) -> Vec<Pick> {
    let mut picks = Vec::new();

    // 方案1: 热号组合
    let mut red1 = stats.red_hot.iter().take(RED_COUNT).cloned().collect::<Vec<_>>();
    red1.sort();
    let mut blue1 = stats.blue_hot.iter().take(BLUE_COUNT).cloned().collect::<Vec<_>>();
    blue1.sort();
    if red1.len() == RED_COUNT {
        picks.push(Pick { red: red1, blue: blue1 });
    }

    // 方案2: 热号 + 冷号混合（3热 + 2冷）
    let hot_take = RED_COUNT - 2;
    let mut red2: Vec<u8> = stats.red_hot.iter().take(hot_take).cloned().collect();
    for &n in stats.red_cold.iter().take(2) {
        if !red2.contains(&n) { red2.push(n); }
    }
    // 冷号不足时补热号
    while red2.len() < RED_COUNT {
        for &n in &stats.red_hot {
            if !red2.contains(&n) {
                red2.push(n);
                if red2.len() == RED_COUNT { break; }
            }
        }
    }
    red2.sort();

    let mut blue2 = Vec::new();
    if !stats.blue_hot.is_empty() { blue2.push(stats.blue_hot[0]); }
    if !stats.blue_cold.is_empty() && blue2.len() < BLUE_COUNT {
        blue2.push(stats.blue_cold[0]);
    }
    while blue2.len() < BLUE_COUNT {
        for i in 1..=BLUE_MAX {
            if !blue2.contains(&i) { blue2.push(i); break; }
        }
    }
    blue2.sort();

    picks.push(Pick { red: red2, blue: blue2 });

    // 方案3: 按奇偶比和大小比选号
    let odd_target = (stats.odd_even_ratio * RED_COUNT as f64).round() as usize;
    let even_target = RED_COUNT - odd_target;

    let odd_nums: Vec<u8> = (1..=RED_MAX).filter(|&n| n % 2 == 1).collect();
    let even_nums: Vec<u8> = (1..=RED_MAX).filter(|&n| n % 2 == 0).collect();

    let mut odd_candidates: Vec<u8> = odd_nums.iter()
        .filter(|&&n| stats.red_freq.contains_key(&n))
        .cloned()
        .collect();
    odd_candidates.sort_by_key(|n| -(stats.red_freq.get(n).cloned().unwrap_or(0) as i32));

    let mut even_candidates: Vec<u8> = even_nums.iter()
        .filter(|&&n| stats.red_freq.contains_key(&n))
        .cloned()
        .collect();
    even_candidates.sort_by_key(|n| -(stats.red_freq.get(n).cloned().unwrap_or(0) as i32));

    let mut red3 = Vec::new();
    for &n in odd_candidates.iter().take(odd_target) { red3.push(n); }
    for &n in even_candidates.iter().take(even_target) {
        if !red3.contains(&n) { red3.push(n); }
    }
    while red3.len() < RED_COUNT {
        for i in 1..=RED_MAX {
            if !red3.contains(&i) { red3.push(i); break; }
        }
    }
    red3.truncate(RED_COUNT);
    red3.sort();

    let mut blue3: Vec<(u8, u32)> = stats.blue_freq.iter()
        .map(|(&k, &v)| (k, v))
        .collect();
    blue3.sort_by_key(|&(_, v)| -(v as i32));
    let blue3: Vec<u8> = blue3.iter().take(BLUE_COUNT).map(|&(k, _)| k).collect();
    let mut blue3 = blue3;
    blue3.sort();

    picks.push(Pick { red: red3, blue: blue3 });

    picks
}
