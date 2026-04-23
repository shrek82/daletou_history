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
    let total = records.len();
    // 数据不足10期，仅用全量频率
    if total < 10 {
        return default_stats();
    }

    // === 1. 全量频率 ===
    let mut red_freq: HashMap<u8, u32> = HashMap::new();
    let mut blue_freq: HashMap<u8, u32> = HashMap::new();
    for r in records {
        for &n in &r.balls.red { *red_freq.entry(n).or_insert(0) += 1; }
        for &n in &r.balls.blue { *blue_freq.entry(n).or_insert(0) += 1; }
    }

    // === 2. 多时间窗口频率 ===
    let mut red_window_freq: Vec<HashMap<u8, u32>> = [10, 20, 50].iter()
        .map(|&w| count_window_freq(records, w, |r| &r.balls.red))
        .collect();
    let mut blue_window_freq: Vec<HashMap<u8, u32>> = [10, 20, 50].iter()
        .map(|&w| count_window_freq(records, w, |r| &r.balls.blue))
        .collect();

    // === 3. 加权得分 ===
    // 注：窗口重叠是有意设计 -- 近10期出现的号码会在三个窗口都计数，
    // 加上权重系数 5+3+2+1，使得近期号码得分远高于远期号码
    let mut red_weighted: HashMap<u8, f64> = HashMap::new();
    for n in 1..=RED_MAX {
        let score = (red_window_freq[0].get(&n).copied().unwrap_or(0) as f64 * WEIGHT_RECENT10)
            + (red_window_freq[1].get(&n).copied().unwrap_or(0) as f64 * WEIGHT_RECENT20)
            + (red_window_freq[2].get(&n).copied().unwrap_or(0) as f64 * WEIGHT_RECENT50)
            + (red_freq.get(&n).copied().unwrap_or(0) as f64 * WEIGHT_ALL);
        red_weighted.insert(n, score);
    }
    let mut blue_weighted: HashMap<u8, f64> = HashMap::new();
    for n in 1..=BLUE_MAX {
        let score = (blue_window_freq[0].get(&n).copied().unwrap_or(0) as f64 * WEIGHT_RECENT10)
            + (blue_window_freq[1].get(&n).copied().unwrap_or(0) as f64 * WEIGHT_RECENT20)
            + (blue_window_freq[2].get(&n).copied().unwrap_or(0) as f64 * WEIGHT_RECENT50)
            + (blue_freq.get(&n).copied().unwrap_or(0) as f64 * WEIGHT_ALL);
        blue_weighted.insert(n, score);
    }

    // 转换为 array
    let red_window_freq = [
        red_window_freq.swap_remove(0),
        red_window_freq.swap_remove(0),
        red_window_freq.swap_remove(0),
    ];
    let blue_window_freq = [
        blue_window_freq.swap_remove(0),
        blue_window_freq.swap_remove(0),
        blue_window_freq.swap_remove(0),
    ];

    // === 4. 遗漏值 ===
    let red_omission = compute_omission(records, RED_MAX, |r: &DrawRecord| &r.balls.red);
    let blue_omission = compute_omission(records, BLUE_MAX, |r: &DrawRecord| &r.balls.blue);

    // === 5. 区间分布 ===
    let zone_avg = compute_zone_avg(records);

    // === 6. 同尾分析 ===
    let avg_tail_duplicates = compute_avg_tail_duplicates(records);

    // === 7. 和值分布 ===
    let (sum_avg, sum_stddev) = compute_sum_stats(records);

    // === 8. 连号分析 ===
    let consecutive_rate = compute_consecutive_rate(records);

    // === 9. 奇偶比、大小比 ===
    let (odd_ratio, big_ratio) = compute_odd_big_ratios(records);

    // === 10. 蓝球近期热号 ===
    let blue_recent_hot = sorted_by_freq(&blue_window_freq[2], 5);

    Stats {
        red_freq, blue_freq,
        red_window_freq, blue_window_freq,
        red_weighted, blue_weighted,
        red_omission, blue_omission,
        zone_avg,
        sum_avg, sum_stddev,
        consecutive_rate, avg_tail_duplicates,
        odd_ratio, big_ratio,
        blue_recent_hot,
    }
}

/// 空 Stats（数据不足时返回）
fn default_stats() -> Stats {
    Stats {
        red_freq: HashMap::new(), blue_freq: HashMap::new(),
        red_window_freq: [HashMap::new(), HashMap::new(), HashMap::new()],
        blue_window_freq: [HashMap::new(), HashMap::new(), HashMap::new()],
        red_weighted: HashMap::new(), blue_weighted: HashMap::new(),
        red_omission: vec![0; RED_MAX as usize],
        blue_omission: vec![0; BLUE_MAX as usize],
        zone_avg: [0.0, 0.0, 0.0],
        sum_avg: 0.0, sum_stddev: 0.0,
        consecutive_rate: 0.0, avg_tail_duplicates: 0.0,
        odd_ratio: 0.5, big_ratio: 0.5,
        blue_recent_hot: vec![],
    }
}

/// 计算指定窗口的频率
fn count_window_freq<F>(records: &[DrawRecord], window: usize, balls: F) -> HashMap<u8, u32>
where F: Fn(&DrawRecord) -> &[u8] {
    let mut freq = HashMap::new();
    for r in records.iter().take(window) {
        for &n in balls(r) { *freq.entry(n).or_insert(0) += 1; }
    }
    freq
}

/// 计算遗漏期数
fn compute_omission<F>(records: &[DrawRecord], max: u8, balls: F) -> Vec<u32>
where F: Fn(&DrawRecord) -> &[u8] {
    let mut omission = vec![0u32; max as usize];
    for (i, r) in records.iter().enumerate() {
        for &n in balls(r) {
            if n <= max { omission[n as usize - 1] = i as u32; }
        }
    }
    let total = records.len() as u32;
    for v in omission.iter_mut() {
        *v = total.saturating_sub(*v);
    }
    omission
}

/// 计算区间分布
fn compute_zone_avg(records: &[DrawRecord]) -> [f64; 3] {
    let mut zones = [0f64; 3];
    for r in records {
        for &n in &r.balls.red {
            if n <= ZONE1_END { zones[0] += 1.0; }
            else if n <= ZONE2_END { zones[1] += 1.0; }
            else { zones[2] += 1.0; }
        }
    }
    let total = records.len() as f64;
    for v in zones.iter_mut() { *v /= total; }
    zones
}

/// 计算平均同尾重复数
fn compute_avg_tail_duplicates(records: &[DrawRecord]) -> f64 {
    let mut total = 0u32;
    for r in records {
        let mut tails: Vec<u8> = r.balls.red.iter().map(|&n| n % 10).collect();
        tails.sort();
        tails.dedup();
        total += (5 - tails.len()) as u32;
    }
    total as f64 / records.len() as f64
}

/// 计算和值统计
fn compute_sum_stats(records: &[DrawRecord]) -> (f64, f64) {
    let sums: Vec<f64> = records.iter()
        .map(|r| r.balls.red.iter().map(|&n| n as f64).sum())
        .collect();
    let avg = sums.iter().sum::<f64>() / sums.len() as f64;
    let variance = sums.iter().map(|s| (s - avg).powi(2)).sum::<f64>() / sums.len() as f64;
    (avg, variance.sqrt())
}

/// 计算连号出现率
fn compute_consecutive_rate(records: &[DrawRecord]) -> f64 {
    let mut count = 0u32;
    for r in records {
        let mut sorted = r.balls.red.clone();
        sorted.sort();
        for i in 1..sorted.len() {
            if sorted[i] == sorted[i - 1] + 1 {
                count += 1;
                break;
            }
        }
    }
    count as f64 / records.len() as f64
}

/// 计算奇偶比和大小比
fn compute_odd_big_ratios(records: &[DrawRecord]) -> (f64, f64) {
    let mut odd = 0u32;
    let mut big = 0u32;
    let mut total = 0u32;
    for r in records {
        for &n in &r.balls.red {
            if n % 2 == 1 { odd += 1; }
            if n >= 18 { big += 1; }
            total += 1;
        }
    }
    (odd as f64 / total as f64, big as f64 / total as f64)
}

/// 按频率排序，返回前N个号码
fn sorted_by_freq(freq: &HashMap<u8, u32>, n: usize) -> Vec<u8> {
    let mut items: Vec<(u8, u32)> = freq.iter().map(|(&k, &v)| (k, v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.into_iter().take(n).map(|(k, _)| k).collect()
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
