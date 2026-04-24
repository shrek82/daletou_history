use super::stats::Stats;

/// 红球一区(01-12)终点
const ZONE1_END: u8 = 12;
/// 红球二区(13-24)终点
const ZONE2_END: u8 = 24;
/// 红球选取数量
const RED_COUNT: usize = 5;
/// 蓝球选取数量
const BLUE_COUNT: usize = 2;

/// 一组推荐号码
pub struct Pick {
    pub red: Vec<u8>,
    pub blue: Vec<u8>,
    pub label: &'static str,
}

impl Pick {
    pub fn new(red: Vec<u8>, blue: Vec<u8>, label: &'static str) -> Self {
        Self { red, blue, label }
    }
}

/// 计算方案综合评分（0-100）
/// 红球80% + 蓝球20%
pub fn score_pick(pick: &Pick, stats: &Stats) -> f64 {
    let red_score = score_red_pick(pick, stats);
    let blue_score = score_blue_pick(pick, stats);
    red_score * 0.8 + blue_score * 0.2
}

/// 红球评分（满分100，含同尾惩罚和连号匹配）
///
/// 维度权重：
/// - 奇偶比匹配度 15%
/// - 大小比匹配度 15%
/// - 和值匹配度 15%
/// - 区间分布匹配度 15%
/// - 同尾惩罚 -10%（每多一组同尾扣10分，最高-20分）
/// - 连号匹配 ±10%
fn score_red_pick(pick: &Pick, stats: &Stats) -> f64 {
    let mut score = 0.0;

    // 奇偶比匹配（15%）
    let odd_count = pick
        .red
        .iter()
        .filter(|&&n| n % 2 == 1)
        .count() as f64
        / RED_COUNT as f64;
    let odd_deviation = (odd_count - stats.odd_ratio).abs();
    score += (1.0 - odd_deviation) * 15.0;

    // 大小比匹配（15%）
    let big_count = pick
        .red
        .iter()
        .filter(|&&n| n >= 18)
        .count() as f64
        / RED_COUNT as f64;
    let big_deviation = (big_count - stats.big_ratio).abs();
    score += (1.0 - big_deviation) * 15.0;

    // 和值匹配（15%）
    let sum = pick.red.iter().map(|&n| n as f64).sum::<f64>();
    let sum_deviation =
        (sum - stats.sum_avg).abs() / stats.sum_stddev.max(0.001);
    let sum_score = (1.0 - (sum_deviation / 2.0).min(1.0)).max(0.0);
    score += sum_score * 15.0;

    // 区间分布匹配（15%）
    let mut zones = [0f64; 3];
    for &n in &pick.red {
        if n <= ZONE1_END {
            zones[0] += 1.0;
        } else if n <= ZONE2_END {
            zones[1] += 1.0;
        } else {
            zones[2] += 1.0;
        }
    }
    let total = zones.iter().sum::<f64>().max(0.001);
    for v in zones.iter_mut() {
        *v /= total;
    }

    let total_zone = stats.zone_avg.iter().sum::<f64>().max(0.001);
    let expected = [
        stats.zone_avg[0] / total_zone,
        stats.zone_avg[1] / total_zone,
        stats.zone_avg[2] / total_zone,
    ];
    let zone_deviation: f64 = zones
        .iter()
        .zip(expected.iter())
        .map(|(a, b)| (a - b).abs())
        .sum();
    let zone_score = (1.0 - zone_deviation).max(0.0);
    score += zone_score * 15.0;

    // 同尾惩罚（-10% per duplicate, max -20）
    let tail_dupes = count_tail_duplicates(&pick.red);
    let tail_penalty = (tail_dupes as f64 * 10.0).min(20.0);
    score -= tail_penalty;

    // 连号匹配（±10%）
    let has_consecutive = has_consecutive_pair(&pick.red);
    if has_consecutive && stats.consecutive_rate > 0.4 {
        score += 10.0;
    } else if !has_consecutive && stats.consecutive_rate > 0.4 {
        // 历史连号率高但未选中，不加分也不扣分
    }

    score.max(0.0)
}

/// 蓝球评分（满分100）
///
/// 维度权重：
/// - 奇偶比例匹配 40%
/// - 加权得分 30%
/// - 小区占比匹配 30%
fn score_blue_pick(pick: &Pick, stats: &Stats) -> f64 {
    let mut score = 0.0;

    // 奇偶比例匹配（40%）
    let odd_count = pick
        .blue
        .iter()
        .filter(|&&n| n % 2 == 1)
        .count() as f64
        / BLUE_COUNT as f64;
    let odd_deviation = (odd_count - stats.blue_odd_ratio).abs();
    let odd_score = (1.0 - odd_deviation).max(0.0);
    score += odd_score * 40.0;

    // 加权得分（30%）
    let blue_total: f64 = pick
        .blue
        .iter()
        .map(|&n| stats.blue_weighted.get(&n).copied().unwrap_or(0.0))
        .sum();
    let max_possible = stats.blue_weighted.values().copied().fold(
        0.0f64,
        f64::max,
    ) * BLUE_COUNT as f64;
    if max_possible > 0.0 {
        score += (blue_total / max_possible) * 30.0;
    }

    // 小区占比匹配（30%）
    let small_count = pick
        .blue
        .iter()
        .filter(|&&n| n <= 6)
        .count() as f64
        / BLUE_COUNT as f64;
    let small_deviation = (small_count - stats.blue_zone_ratio).abs();
    let small_score = (1.0 - small_deviation).max(0.0);
    score += small_score * 30.0;

    score
}

/// 计算红球中同尾重复组数
fn count_tail_duplicates(red: &[u8]) -> usize {
    let mut tails: Vec<u8> = red.iter().map(|&n| n % 10).collect();
    tails.sort();
    let mut dupes = 0;
    for i in 1..tails.len() {
        if tails[i] == tails[i - 1] {
            dupes += 1;
        }
    }
    dupes
}

/// 判断红球中是否存在相邻号码
fn has_consecutive_pair(red: &[u8]) -> bool {
    let mut sorted = red.to_vec();
    sorted.sort();
    for i in 1..sorted.len() {
        if sorted[i] == sorted[i - 1] + 1 {
            return true;
        }
    }
    false
}

/// 判断标签是否为完全随机类型
pub fn is_completely_random(label: &str) -> bool {
    label.starts_with("完全随机")
}
