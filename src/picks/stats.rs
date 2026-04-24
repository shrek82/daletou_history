use std::collections::HashMap;
use daletou::DrawRecord;

/// 红球最大值
const RED_MAX: u8 = 35;
/// 蓝球最大值
const BLUE_MAX: u8 = 12;
/// 红球一区(01-12)终点
const ZONE1_END: u8 = 12;
/// 红球二区(13-24)终点
const ZONE2_END: u8 = 24;

/// 加权权重：近10期×5 + 近20期×3 + 近50期×2 + 全量×1
const WEIGHT_RECENT10: f64 = 5.0;
const WEIGHT_RECENT20: f64 = 3.0;
const WEIGHT_RECENT50: f64 = 2.0;
const WEIGHT_ALL: f64 = 1.0;

/// 统计分析结果
pub struct Stats {
    // === 红球 ===
    /// 红球全量频率
    pub red_freq: HashMap<u8, u32>,
    /// 红球各窗口频率 [近10, 近20, 近50]
    pub red_window_freq: [HashMap<u8, u32>; 3],
    /// 红球加权得分
    pub red_weighted: HashMap<u8, f64>,
    /// 红球遗漏期数
    pub red_omission: Vec<u32>,
    /// 区间分布 [一区均值, 二区均值, 三区均值]
    pub zone_avg: [f64; 3],
    /// 和值统计 (均值, 标准差)
    pub sum_avg: f64,
    pub sum_stddev: f64,
    /// 连号出现率
    pub consecutive_rate: f64,
    /// 平均每期同尾重复数
    pub avg_tail_duplicates: f64,
    /// 奇偶比（红球奇数占比）
    pub odd_ratio: f64,
    /// 大小比（红球大数18-35占比）
    pub big_ratio: f64,

    // === 蓝球 ===
    /// 蓝球全量频率
    pub blue_freq: HashMap<u8, u32>,
    /// 蓝球各窗口频率 [近10, 近20, 近50]
    pub blue_window_freq: [HashMap<u8, u32>; 3],
    /// 蓝球加权得分
    pub blue_weighted: HashMap<u8, f64>,
    /// 蓝球遗漏期数
    pub blue_omission: Vec<u32>,
    /// 蓝球小区(1-6)占比
    pub blue_zone_ratio: f64,
    /// 蓝球奇数占比
    pub blue_odd_ratio: f64,
    /// 蓝球和值均值
    pub blue_sum_avg: f64,
    /// 蓝球和值标准差
    pub blue_sum_stddev: f64,
    /// 蓝球近期热号（近50期频率排序）
    pub blue_recent_hot: Vec<u8>,
}

/// 数据不足时返回空统计
pub fn default_stats() -> Stats {
    Stats {
        red_freq: HashMap::new(),
        red_window_freq: [HashMap::new(), HashMap::new(), HashMap::new()],
        red_weighted: HashMap::new(),
        red_omission: vec![0; RED_MAX as usize],
        zone_avg: [0.0, 0.0, 0.0],
        sum_avg: 0.0,
        sum_stddev: 0.0,
        consecutive_rate: 0.0,
        avg_tail_duplicates: 0.0,
        odd_ratio: 0.5,
        big_ratio: 0.5,
        blue_freq: HashMap::new(),
        blue_window_freq: [HashMap::new(), HashMap::new(), HashMap::new()],
        blue_weighted: HashMap::new(),
        blue_omission: vec![0; BLUE_MAX as usize],
        blue_zone_ratio: 0.5,
        blue_odd_ratio: 0.5,
        blue_sum_avg: 0.0,
        blue_sum_stddev: 0.0,
        blue_recent_hot: vec![],
    }
}

/// 执行全面统计分析
pub fn analyze(records: &[DrawRecord]) -> Stats {
    let total = records.len();
    if total < 10 {
        return default_stats();
    }

    // === 1. 全量频率 ===
    let mut red_freq: HashMap<u8, u32> = HashMap::new();
    let mut blue_freq: HashMap<u8, u32> = HashMap::new();
    for r in records {
        for &n in &r.balls.red {
            *red_freq.entry(n).or_insert(0) += 1;
        }
        for &n in &r.balls.blue {
            *blue_freq.entry(n).or_insert(0) += 1;
        }
    }

    // === 2. 多时间窗口频率 ===
    let mut red_window_freq: Vec<HashMap<u8, u32>> = [10, 20, 50]
        .iter()
        .map(|&w| count_window_freq(records, w, |r| &r.balls.red))
        .collect();
    let mut blue_window_freq: Vec<HashMap<u8, u32>> = [10, 20, 50]
        .iter()
        .map(|&w| count_window_freq(records, w, |r| &r.balls.blue))
        .collect();

    // === 3. 加权得分 ===
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
    let (sum_avg, sum_stddev) = compute_sum_stats(records, |r| &r.balls.red);

    // === 8. 连号分析 ===
    let consecutive_rate = compute_consecutive_rate(records);

    // === 9. 奇偶比、大小比 ===
    let (odd_ratio, big_ratio) = compute_odd_big_ratios(records);

    // === 10. 蓝球专项分析 ===
    let blue_zone_ratio = compute_blue_zone_ratio(records);
    let blue_odd_ratio = compute_blue_odd_ratio(records);
    let (blue_sum_avg, blue_sum_stddev) =
        compute_sum_stats(records, |r| &r.balls.blue);

    // === 11. 蓝球近期热号 ===
    let blue_recent_hot = sorted_by_freq(&blue_window_freq[2], 5);

    Stats {
        red_freq,
        red_window_freq,
        red_weighted,
        red_omission,
        zone_avg,
        sum_avg,
        sum_stddev,
        consecutive_rate,
        avg_tail_duplicates,
        odd_ratio,
        big_ratio,
        blue_freq,
        blue_window_freq,
        blue_weighted,
        blue_omission,
        blue_zone_ratio,
        blue_odd_ratio,
        blue_sum_avg,
        blue_sum_stddev,
        blue_recent_hot,
    }
}

/// 计算指定窗口的频率
fn count_window_freq<F>(
    records: &[DrawRecord],
    window: usize,
    balls: F,
) -> HashMap<u8, u32>
where
    F: Fn(&DrawRecord) -> &[u8],
{
    let mut freq = HashMap::new();
    for r in records.iter().take(window) {
        for &n in balls(r) {
            *freq.entry(n).or_insert(0) += 1;
        }
    }
    freq
}

/// 计算遗漏期数
fn compute_omission<F>(records: &[DrawRecord], max: u8, balls: F) -> Vec<u32>
where
    F: Fn(&DrawRecord) -> &[u8],
{
    let mut omission = vec![0u32; max as usize];
    for (i, r) in records.iter().enumerate() {
        for &n in balls(r) {
            if n <= max {
                omission[n as usize - 1] = i as u32;
            }
        }
    }
    let total = records.len() as u32;
    for v in omission.iter_mut() {
        *v = total.saturating_sub(*v);
    }
    omission
}

/// 计算区间分布均值
fn compute_zone_avg(records: &[DrawRecord]) -> [f64; 3] {
    let mut zones = [0f64; 3];
    for r in records {
        for &n in &r.balls.red {
            if n <= ZONE1_END {
                zones[0] += 1.0;
            } else if n <= ZONE2_END {
                zones[1] += 1.0;
            } else {
                zones[2] += 1.0;
            }
        }
    }
    let total = records.len() as f64;
    for v in zones.iter_mut() {
        *v /= total;
    }
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

/// 计算和值统计（均值 + 标准差）
fn compute_sum_stats<F>(records: &[DrawRecord], balls: F) -> (f64, f64)
where
    F: Fn(&DrawRecord) -> &[u8],
{
    let sums: Vec<f64> = records
        .iter()
        .map(|r| balls(r).iter().map(|&n| n as f64).sum())
        .collect();
    let avg = sums.iter().sum::<f64>() / sums.len() as f64;
    let variance =
        sums.iter().map(|s| (s - avg).powi(2)).sum::<f64>() / sums.len() as f64;
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
            if n % 2 == 1 {
                odd += 1;
            }
            if n >= 18 {
                big += 1;
            }
            total += 1;
        }
    }
    (odd as f64 / total as f64, big as f64 / total as f64)
}

/// 计算蓝球小区(1-6)占比
fn compute_blue_zone_ratio(records: &[DrawRecord]) -> f64 {
    let mut small = 0u32;
    let mut total = 0u32;
    for r in records {
        for &n in &r.balls.blue {
            if n <= 6 {
                small += 1;
            }
            total += 1;
        }
    }
    small as f64 / total as f64
}

/// 计算蓝球奇数占比
fn compute_blue_odd_ratio(records: &[DrawRecord]) -> f64 {
    let mut odd = 0u32;
    let mut total = 0u32;
    for r in records {
        for &n in &r.balls.blue {
            if n % 2 == 1 {
                odd += 1;
            }
            total += 1;
        }
    }
    odd as f64 / total as f64
}

/// 按频率排序，返回前N个号码
fn sorted_by_freq(freq: &HashMap<u8, u32>, n: usize) -> Vec<u8> {
    let mut items: Vec<(u8, u32)> =
        freq.iter().map(|(&k, &v)| (k, v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.into_iter().take(n).map(|(k, _)| k).collect()
}

/// 打印分析报告
pub fn print_analysis(stats: &Stats, records: &[DrawRecord]) {
    println!("===== 数据分析（{} 期）=====", records.len());

    // 红球综合排名 TOP 10
    println!("\n红球综合排名（按加权得分）");
    println!("  排名 号码  总分  全量  近50  近20  近10  遗漏");
    println!("  ---- ---- ---- ---- ---- ---- ---- ----");

    let mut red_sorted: Vec<(u8, f64)> =
        stats.red_weighted.iter().map(|(&k, &v)| (k, v)).collect();
    red_sorted.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap()
            .then_with(|| a.0.cmp(&b.0))
    });

    for (i, (ball, score)) in red_sorted.iter().take(10).enumerate() {
        let total = stats.red_freq.get(ball).copied().unwrap_or(0);
        let freq50 = stats.red_window_freq[2].get(ball).copied().unwrap_or(0);
        let freq20 = stats.red_window_freq[1].get(ball).copied().unwrap_or(0);
        let freq10 = stats.red_window_freq[0].get(ball).copied().unwrap_or(0);
        let omission = stats.red_omission[*ball as usize - 1];
        println!(
            "  {:>4} {:02}  {:>4.1} {:>4} {:>4} {:>4} {:>4} {:>4}",
            i + 1,
            ball,
            score,
            total,
            freq50,
            freq20,
            freq10,
            omission
        );
    }

    // 区间分布
    println!("\n红球区间分布");
    println!("  区域  范围    均出现  占比");
    let total_zone: f64 = stats.zone_avg.iter().sum();
    println!(
        "  一区  01-12  {:>5.1} {:>5.1}%",
        stats.zone_avg[0],
        stats.zone_avg[0] / total_zone * 100.0
    );
    println!(
        "  二区  13-24  {:>5.1} {:>5.1}%",
        stats.zone_avg[1],
        stats.zone_avg[1] / total_zone * 100.0
    );
    println!(
        "  三区  25-35  {:>5.1} {:>5.1}%",
        stats.zone_avg[2],
        stats.zone_avg[2] / total_zone * 100.0
    );

    // 和值、连号、同尾
    println!(
        "\n和值分布: 均值 {:.1}  标准差 {:.1}  常见范围: {:.0}-{:.0}",
        stats.sum_avg,
        stats.sum_stddev,
        stats.sum_avg - stats.sum_stddev * 0.5,
        stats.sum_avg + stats.sum_stddev * 0.5
    );
    println!("连号出现率: {:.1}%", stats.consecutive_rate * 100.0);
    println!("同尾分析: 平均 {:.1} 个同尾重复/期", stats.avg_tail_duplicates);

    // 蓝球分析
    println!("\n蓝球综合排名（按加权得分）");
    println!("  排名 号码  总分  全量  近50  近20  近10  遗漏");
    println!("  ---- ---- ---- ---- ---- ---- ---- ----");

    let mut blue_sorted: Vec<(u8, f64)> =
        stats.blue_weighted.iter().map(|(&k, &v)| (k, v)).collect();
    blue_sorted.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap()
            .then_with(|| a.0.cmp(&b.0))
    });

    for (i, (ball, score)) in blue_sorted.iter().take(5).enumerate() {
        let total = stats.blue_freq.get(ball).copied().unwrap_or(0);
        let freq50 =
            stats.blue_window_freq[2].get(ball).copied().unwrap_or(0);
        let freq20 =
            stats.blue_window_freq[1].get(ball).copied().unwrap_or(0);
        let freq10 =
            stats.blue_window_freq[0].get(ball).copied().unwrap_or(0);
        let omission = stats.blue_omission[*ball as usize - 1];
        println!(
            "  {:>4} {:02}  {:>4.1} {:>4} {:>4} {:>4} {:>4} {:>4}",
            i + 1,
            ball,
            score,
            total,
            freq50,
            freq20,
            freq10,
            omission
        );
    }

    println!("\n蓝球统计");
    println!(
        "  小区(1-6)占比: {:.1}%  奇数占比: {:.1}%",
        stats.blue_zone_ratio * 100.0,
        stats.blue_odd_ratio * 100.0
    );
    println!(
        "  和值均值: {:.1}  标准差: {:.1}",
        stats.blue_sum_avg, stats.blue_sum_stddev
    );
}
