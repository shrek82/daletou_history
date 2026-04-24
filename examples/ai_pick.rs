use daletou::{Client, DrawRecord};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

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

/// 线性同余随机数生成器（种子可控，可复现）
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        // SplitMix64 初始化
        Self { state: seed.wrapping_add(0x9E3779B97F4A7C15) }
    }

    /// 生成 [0, 1) 之间的 f64
    fn next_f64(&mut self) -> f64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.state ^= self.state >> 30;
        (self.state >> 11) as f64 / (1u64 << 53) as f64
    }

    /// 加权随机抽样：按权重比例从 items 中抽取 count 个不重复的元素
    fn weighted_sample(&mut self, items: &[(u8, f64)], count: usize) -> Vec<u8> {
        let mut result = Vec::new();
        let mut remaining = items.to_vec();

        while result.len() < count && !remaining.is_empty() {
            let total: f64 = remaining.iter().map(|(_, w)| w.max(0.001)).sum();
            let mut r = self.next_f64() * total;
            let mut chosen_idx = 0;

            for (i, (_, w)) in remaining.iter().enumerate() {
                r -= w.max(0.001);
                if r <= 0.0 {
                    chosen_idx = i;
                    break;
                }
                chosen_idx = i;
            }

            result.push(remaining[chosen_idx].0);
            remaining.remove(chosen_idx);
        }
        result.sort();
        result
    }
}

/// 计算方案评分（0-100）
fn score_pick(pick: &Pick, stats: &Stats) -> f64 {
    let red_score = score_red_pick(pick, stats);
    let blue_score = score_blue_pick(pick, stats);
    red_score * 0.8 + blue_score * 0.2
}

/// 红球评分
fn score_red_pick(pick: &Pick, stats: &Stats) -> f64 {
    let mut score = 0.0;

    // 奇偶比匹配（20%）
    let odd_count = pick.red.iter().filter(|&&n| n % 2 == 1).count() as f64 / RED_COUNT as f64;
    let odd_deviation = (odd_count - stats.odd_ratio).abs();
    score += (1.0 - odd_deviation) * 20.0;

    // 大小比匹配（20%）
    let big_count = pick.red.iter().filter(|&&n| n >= 18).count() as f64 / RED_COUNT as f64;
    let big_deviation = (big_count - stats.big_ratio).abs();
    score += (1.0 - big_deviation) * 20.0;

    // 和值匹配（20%）
    let sum = pick.red.iter().map(|&n| n as f64).sum::<f64>();
    let sum_deviation = (sum - stats.sum_avg).abs() / stats.sum_stddev.max(0.001);
    let sum_score = (1.0 - (sum_deviation / 2.0).min(1.0)).max(0.0);
    score += sum_score * 20.0;

    // 区间分布匹配（20%）
    let mut zones = [0f64; 3];
    for &n in &pick.red {
        if n <= ZONE1_END { zones[0] += 1.0; }
        else if n <= ZONE2_END { zones[1] += 1.0; }
        else { zones[2] += 1.0; }
    }
    let total = zones.iter().sum::<f64>().max(0.001);
    for v in zones.iter_mut() { *v /= total; }

    let total_zone = stats.zone_avg.iter().sum::<f64>().max(0.001);
    let expected = [
        stats.zone_avg[0] / total_zone,
        stats.zone_avg[1] / total_zone,
        stats.zone_avg[2] / total_zone,
    ];
    let zone_deviation: f64 = zones.iter().zip(expected.iter()).map(|(a, b)| (a - b).abs()).sum();
    let zone_score = (1.0 - zone_deviation).max(0.0);
    score += zone_score * 20.0;

    score
}

/// 蓝球评分
fn score_blue_pick(pick: &Pick, stats: &Stats) -> f64 {
    let mut score = 0.0;

    // 奇偶匹配（50%）
    let odd_count = pick.blue.iter().filter(|&&n| n % 2 == 1).count();
    let odd_even_score = if odd_count == 1 { 1.0 } else { 0.5 };
    score += odd_even_score * 50.0;

    // 加权得分归一化（50%）
    let blue_total: f64 = pick.blue.iter().map(|&n| stats.blue_weighted.get(&n).copied().unwrap_or(0.0)).sum();
    let max_possible = stats.blue_weighted.values().copied().fold(0.0f64, f64::max) * BLUE_COUNT as f64;
    if max_possible > 0.0 {
        score += (blue_total / max_possible) * 50.0;
    }

    score
}

fn main() {
    let cache_path = PathBuf::from("/tmp/daletou_cache300.json");

    let client = Client::new()
        .with_cache_path(cache_path)
        .with_cache_ttl(Duration::from_secs(86400))
        .with_request_interval(Duration::from_secs(2));

    println!("正在获取最近 300 期开奖数据...");
    let records = match client.get_cached_records(300) {
        Ok(r) => r,
        Err(e) => { eprintln!("获取数据失败: {}", e); return; }
    };
    println!("成功获取 {} 条记录\n", records.len());

    let stats = analyze(&records);
    print_analysis(&stats, &records);

    // 用当前时间戳做随机种子
    let seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let picks = generate_picks(&stats, seed);

    println!("\n===== AI 推荐方案（13组：6组策略 + 2组加权随机 + 5组完全随机，种子={}）=====", seed);
    println!("  方案  红球            蓝球    评分  标签");

    for (i, pick) in picks.iter().enumerate() {
        let score = score_pick(pick, &stats);
        print!("  {:>4}  ", i + 1);
        for n in &pick.red { print!("{:02} ", n); }
        print!(" + ");
        for n in &pick.blue { print!("{:02} ", n); }
        print!("  {:>5.1}  {}", score, pick.label);
        println!();
    }
}

/// 统计分析结果
struct Stats {
    /// 红球全量频率
    red_freq: HashMap<u8, u32>,
    /// 蓝球全量频率（预留）
    #[allow(dead_code)]
    blue_freq: HashMap<u8, u32>,
    /// 红球各窗口频率 [近10, 近20, 近50]
    red_window_freq: [HashMap<u8, u32>; 3],
    /// 蓝球各窗口频率 [近10, 近20, 近50]（预留）
    #[allow(dead_code)]
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
    label: &'static str,
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

    // 红球综合排名 TOP 10
    println!("\n红球综合排名（按加权得分）");
    println!("  排名 号码  总分  全量  近50  近20  近10  遗漏");
    println!("  ---- ---- ---- ---- ---- ---- ---- ----");

    let mut red_sorted: Vec<(u8, f64)> = stats.red_weighted.iter()
        .map(|(&k, &v)| (k, v)).collect();
    red_sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then_with(|| a.0.cmp(&b.0)));

    for (i, (ball, score)) in red_sorted.iter().take(10).enumerate() {
        let total = stats.red_freq.get(ball).copied().unwrap_or(0);
        let freq50 = stats.red_window_freq[2].get(ball).copied().unwrap_or(0);
        let freq20 = stats.red_window_freq[1].get(ball).copied().unwrap_or(0);
        let freq10 = stats.red_window_freq[0].get(ball).copied().unwrap_or(0);
        let omission = stats.red_omission[*ball as usize - 1];
        println!("  {:>4} {:02}  {:>4.1} {:>4} {:>4} {:>4} {:>4} {:>4}",
            i + 1, ball, score, total, freq50, freq20, freq10, omission);
    }

    // 区间分布
    println!("\n红球区间分布");
    println!("  区域  范围    均出现  占比");
    let total_zone: f64 = stats.zone_avg.iter().sum();
    println!("  一区  01-12  {:>5.1} {:>5.1}%",
        stats.zone_avg[0], stats.zone_avg[0] / total_zone * 100.0);
    println!("  二区  13-24  {:>5.1} {:>5.1}%",
        stats.zone_avg[1], stats.zone_avg[1] / total_zone * 100.0);
    println!("  三区  25-35  {:>5.1} {:>5.1}%",
        stats.zone_avg[2], stats.zone_avg[2] / total_zone * 100.0);

    // 和值、连号、同尾
    println!("\n和值分布: 均值 {:.1}  标准差 {:.1}  常见范围: {:.0}-{:.0}",
        stats.sum_avg, stats.sum_stddev,
        stats.sum_avg - stats.sum_stddev * 0.5,
        stats.sum_avg + stats.sum_stddev * 0.5);
    println!("连号出现率: {:.1}%", stats.consecutive_rate * 100.0);
    println!("同尾分析: 平均 {:.1} 个同尾重复/期", stats.avg_tail_duplicates);
}

/// 生成多组推荐号码（6组确定性 + 2组随机扰动）
fn generate_picks(stats: &Stats, seed: u64) -> Vec<Pick> {
    let mut picks = Vec::with_capacity(8);

    // 方案1: 纯加权热号
    let red1 = pick_by_weighted_top(&stats.red_weighted, RED_COUNT);
    let blue1 = pick_by_weighted_top(&stats.blue_weighted, BLUE_COUNT);
    picks.push(Pick { red: red1, blue: blue1, label: "纯热号" });

    // 方案2: 冷热混合（3热 + 2冷）
    let red2 = pick_hot_cold_mix(&stats.red_weighted, &stats.red_omission, 3, 2);
    let blue2 = pick_blue_hot_cold(&stats.blue_weighted, &stats.blue_omission);
    picks.push(Pick { red: red2, blue: blue2, label: "冷热混合" });

    // 方案3: 区间均衡
    let red3 = pick_by_zone(&stats.red_weighted, stats.zone_avg);
    let blue3 = pick_blue_odd_even(&stats.blue_weighted);
    picks.push(Pick { red: red3, blue: blue3, label: "区间均衡" });

    // 方案4: 和值约束
    let red4 = pick_by_sum_constraint(&stats.red_weighted, stats.sum_avg, stats.sum_stddev);
    let blue4 = pick_by_weighted_top(&stats.blue_weighted, BLUE_COUNT);
    picks.push(Pick { red: red4, blue: blue4, label: "和值约束" });

    // 方案5: 同尾约束
    let red5 = pick_by_tail_constraint(&stats.red_weighted);
    let blue5 = pick_blue_different_tail(&stats.blue_weighted);
    picks.push(Pick { red: red5, blue: blue5, label: "同尾约束" });

    // 方案6: 连号策略
    let red6 = pick_by_consecutive(&stats.red_weighted);
    let blue6 = pick_blue_hot(&stats.blue_weighted, &stats.blue_recent_hot);
    picks.push(Pick { red: red6, blue: blue6, label: "连号策略" });

    // 方案7: 随机扰动 A -- 从TOP20候选中按权重随机抽样
    let mut rng_a = Rng::new(seed);
    let red7 = pick_by_weighted_random(&stats.red_weighted, 20, RED_COUNT, &mut rng_a);
    let blue7 = pick_by_weighted_random(&stats.blue_weighted, 8, BLUE_COUNT, &mut rng_a);
    picks.push(Pick { red: red7, blue: blue7, label: "加权随机A" });

    // 方案8: 随机扰动 B -- 不同种子
    let mut rng_b = Rng::new(seed.wrapping_add(1));
    let red8 = pick_by_weighted_random(&stats.red_weighted, 20, RED_COUNT, &mut rng_b);
    let blue8 = pick_by_weighted_random(&stats.blue_weighted, 8, BLUE_COUNT, &mut rng_b);
    picks.push(Pick { red: red8, blue: blue8, label: "加权随机B" });

    // 方案9-13: 完全随机（基于操作系统级加密安全随机源）
    for i in 0..5 {
        let (red, blue) = pick_completely_random();
        picks.push(Pick { red, blue, label: BOXES[i] });
    }

    picks
}

/// 完全随机标签
const BOXES: [&str; 5] = ["完全随机A", "完全随机B", "完全随机C", "完全随机D", "完全随机E"];

/// 加权随机抽样：从TOP N候选池中按权重随机抽取 count 个号码
fn pick_by_weighted_random(weighted: &HashMap<u8, f64>, pool_size: usize, count: usize, rng: &mut Rng) -> Vec<u8> {
    let candidates = pick_by_weighted_top(weighted, pool_size);
    let items: Vec<(u8, f64)> = candidates.iter()
        .map(|&n| (n, *weighted.get(&n).unwrap_or(&0.0)))
        .collect();
    rng.weighted_sample(&items, count)
}

/// 按加权得分TOP N选号
fn pick_by_weighted_top(weighted: &HashMap<u8, f64>, count: usize) -> Vec<u8> {
    let mut items: Vec<(u8, f64)> = weighted.iter().map(|(&k, &v)| (k, v)).collect();
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then_with(|| a.0.cmp(&b.0)));
    items.into_iter().take(count).map(|(k, _)| k).collect()
}

/// 冷热混合：hot_count个高加权 + cold_count个高遗漏
fn pick_hot_cold_mix(weighted: &HashMap<u8, f64>, omission: &[u32],
                      hot_count: usize, cold_count: usize) -> Vec<u8> {
    let hot = pick_by_weighted_top(weighted, hot_count);
    let mut omission_items: Vec<(u8, u32)> = omission.iter().enumerate()
        .map(|(i, &v)| (i as u8 + 1, v))
        .filter(|(n, _)| !hot.contains(n))
        .collect();
    omission_items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut cold: Vec<u8> = omission_items.into_iter().take(cold_count).map(|(k, _)| k).collect();

    let mut result = hot;
    result.append(&mut cold);

    // 不足时用加权补足
    while result.len() < hot_count + cold_count {
        let top = pick_by_weighted_top(weighted, result.len() + 1);
        for n in top { if !result.contains(&n) { result.push(n); break; } }
    }
    result.truncate(hot_count + cold_count);
    result.sort();
    result
}

/// 蓝球：1高加权 + 1高遗漏
fn pick_blue_hot_cold(weighted: &HashMap<u8, f64>, omission: &[u32]) -> Vec<u8> {
    let hot = pick_by_weighted_top(weighted, 1);
    let mut omission_items: Vec<(u8, u32)> = omission.iter().enumerate()
        .map(|(i, &v)| (i as u8 + 1, v))
        .filter(|(n, _)| !hot.contains(n))
        .collect();
    omission_items.sort_by(|a, b| b.1.cmp(&a.1));
    let mut cold: Vec<u8> = omission_items.into_iter().take(1).map(|(k, _)| k).collect();

    let mut result = hot;
    result.append(&mut cold);
    while result.len() < 2 {
        for i in 1..=BLUE_MAX { if !result.contains(&i) { result.push(i); break; } }
    }
    result.sort();
    result
}

/// 区间均衡：按历史比例分配
fn pick_by_zone(weighted: &HashMap<u8, f64>, zone_avg: [f64; 3]) -> Vec<u8> {
    let total_zone: f64 = zone_avg.iter().sum();
    if total_zone == 0.0 { return pick_by_weighted_top(weighted, RED_COUNT); }

    let mut zone_counts = [0usize; 3];
    let mut assigned = 0usize;
    for i in 0..3 {
        zone_counts[i] = (zone_avg[i] / total_zone * RED_COUNT as f64).round() as usize;
        assigned += zone_counts[i];
    }
    while assigned > RED_COUNT {
        let max_zone = zone_counts.iter().enumerate().max_by_key(|&(_, v)| v).unwrap().0;
        zone_counts[max_zone] -= 1; assigned -= 1;
    }
    while assigned < RED_COUNT {
        let max_zone = zone_counts.iter().enumerate().max_by_key(|&(_, v)| v).unwrap().0;
        zone_counts[max_zone] += 1; assigned += 1;
    }

    let mut result = Vec::new();
    let zones = [(1, ZONE1_END), (ZONE1_END + 1, ZONE2_END), (ZONE2_END + 1, RED_MAX)];
    for (i, &(start, end)) in zones.iter().enumerate() {
        let zone_weighted: Vec<(u8, f64)> = weighted.iter()
            .filter(|(k, _)| **k >= start && **k <= end)
            .map(|(k, v)| (*k, *v))
            .collect();
        let mut sorted = zone_weighted;
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then_with(|| a.0.cmp(&b.0)));
        for (k, _) in sorted.into_iter().take(zone_counts[i]) {
            result.push(k);
        }
    }
    result.sort();
    result
}

/// 和值约束：在和值范围内找最优组合
fn pick_by_sum_constraint(weighted: &HashMap<u8, f64>, sum_avg: f64, sum_stddev: f64) -> Vec<u8> {
    if let Some(pick) = find_sum_in_range(weighted, sum_avg - sum_stddev * 0.5, sum_avg + sum_stddev * 0.5) {
        return pick;
    }
    if let Some(pick) = find_sum_in_range(weighted, sum_avg - sum_stddev, sum_avg + sum_stddev) {
        return pick;
    }
    pick_by_weighted_top(weighted, RED_COUNT)
}

/// 在和值范围内找加权最高的组合
fn find_sum_in_range(weighted: &HashMap<u8, f64>, min_sum: f64, max_sum: f64) -> Option<Vec<u8>> {
    let candidates = pick_by_weighted_top(weighted, 15);
    let mut best_pick: Option<Vec<u8>> = None;
    let mut best_score = f64::MIN;

    for i in 0..candidates.len() {
        for j in (i+1)..candidates.len() {
            for k in (j+1)..candidates.len() {
                for l in (k+1)..candidates.len() {
                    for m in (l+1)..candidates.len() {
                        let combo = vec![candidates[i], candidates[j], candidates[k], candidates[l], candidates[m]];
                        let sum = combo.iter().map(|&n| n as f64).sum::<f64>();
                        if sum >= min_sum && sum <= max_sum {
                            let score: f64 = combo.iter().map(|&n| weighted.get(&n).copied().unwrap_or(0.0)).sum();
                            if score > best_score {
                                best_score = score;
                                best_pick = Some(combo);
                            }
                        }
                    }
                }
            }
        }
    }
    best_pick.map(|mut p| { p.sort(); p })
}

/// 同尾约束：最多2个同尾
fn pick_by_tail_constraint(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let candidates = pick_by_weighted_top(weighted, 15);
    greedy_pick_with_tail_constraint(&candidates, RED_COUNT)
}

/// 贪心选号，满足同尾约束
fn greedy_pick_with_tail_constraint(candidates: &[u8], count: usize) -> Vec<u8> {
    let mut result = Vec::new();
    let mut tail_count = [0u32; 10];

    for &n in candidates {
        if result.len() >= count { break; }
        let tail = (n % 10) as usize;
        if tail_count[tail] < 2 {
            result.push(n);
            tail_count[tail] += 1;
        }
    }
    // 不足时放宽约束
    if result.len() < count {
        for &n in candidates {
            if !result.contains(&n) {
                result.push(n);
                if result.len() == count { break; }
            }
        }
    }
    result.sort();
    result
}

/// 连号策略：选一组加权最高的相邻号码
fn pick_by_consecutive(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let top15 = pick_by_weighted_top(weighted, 15);
    let mut best_pair: Option<(u8, u8)> = None;
    let mut best_pair_score = 0.0;

    for i in 0..top15.len() {
        for j in (i+1)..top15.len() {
            if top15[j] == top15[i] + 1 {
                let score = weighted.get(&top15[i]).copied().unwrap_or(0.0)
                    + weighted.get(&top15[j]).copied().unwrap_or(0.0);
                if score > best_pair_score {
                    best_pair_score = score;
                    best_pair = Some((top15[i], top15[j]));
                }
            }
        }
    }

    let mut result = match best_pair {
        Some((a, b)) => vec![a, b],
        None => return pick_by_weighted_top(weighted, RED_COUNT),
    };

    for &n in pick_by_weighted_top(weighted, RED_COUNT + 2).iter() {
        if !result.contains(&n) {
            result.push(n);
            if result.len() == RED_COUNT { break; }
        }
    }
    result.sort();
    result
}

/// 蓝球：一奇一偶优先
fn pick_blue_odd_even(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let top5 = pick_by_weighted_top(weighted, 5);
    let odd: Vec<u8> = top5.iter().filter(|&&n| n % 2 == 1).copied().collect();
    let even: Vec<u8> = top5.iter().filter(|&&n| n % 2 == 0).copied().collect();

    if !odd.is_empty() && !even.is_empty() {
        let mut r = vec![odd[0], even[0]];
        r.sort();
        return r;
    }
    top5.into_iter().take(2).collect::<Vec<_>>()
}

/// 蓝球：不同尾优先
fn pick_blue_different_tail(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let top = pick_by_weighted_top(weighted, BLUE_COUNT + 2);
    for i in 0..top.len() {
        for j in (i+1)..top.len() {
            if top[i] % 10 != top[j] % 10 {
                let mut r = vec![top[i], top[j]];
                r.sort();
                return r;
            }
        }
    }
    top.into_iter().take(BLUE_COUNT).collect::<Vec<_>>()
}

/// 蓝球：近期热度TOP2
fn pick_blue_hot(_weighted: &HashMap<u8, f64>, recent_hot: &[u8]) -> Vec<u8> {
    let mut result: Vec<u8> = recent_hot.iter().take(BLUE_COUNT).copied().collect();
    while result.len() < BLUE_COUNT {
        for i in 1..=BLUE_MAX {
            if !result.contains(&i) { result.push(i); break; }
        }
    }
    result.sort();
    result
}

/// 完全随机选号：使用操作系统级加密安全随机源
/// macOS: getentropy() | Linux: getrandom() syscall | Windows: BCryptGenRandom
/// 模拟真实摇奖过程：Fisher-Yates 洗牌，从小到大排列
fn pick_completely_random() -> (Vec<u8>, Vec<u8>) {
    let red = fisher_yates_sample(1, RED_MAX, RED_COUNT);
    let blue = fisher_yates_sample(1, BLUE_MAX, BLUE_COUNT);
    (red, blue)
}

/// Fisher-Yates 洗牌：从 [min, max] 中均匀随机抽取 count 个不重复的数，返回已排序结果
fn fisher_yates_sample(min: u8, max: u8, count: usize) -> Vec<u8> {
    let range = (max - min + 1) as usize;
    // 初始化球池
    let mut pool: Vec<u8> = (min..=max).collect();

    // Fisher-Yates 洗牌：只洗前 count 个位置
    for i in 0..count {
        let remaining = range - i;
        let mut buf = [0u8; 4];
        getrandom::fill(&mut buf).expect("获取系统随机失败");
        let j = i + (u32::from_ne_bytes(buf) as usize % remaining);
        pool.swap(i, j);
    }

    // 取前 count 个，从小到大排序
    let mut result: Vec<u8> = pool.into_iter().take(count).collect();
    result.sort();
    result
}
