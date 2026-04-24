use std::collections::HashMap;

use super::scoring::Pick;
use super::stats::Stats;

/// 红球最大值
const RED_MAX: u8 = 35;
/// 蓝球最大值
const BLUE_MAX: u8 = 12;
/// 红球选取数量
const RED_COUNT: usize = 5;
/// 蓝球选取数量
const BLUE_COUNT: usize = 2;
/// 红球一区(01-12)终点
const ZONE1_END: u8 = 12;
/// 红球二区(13-24)终点
const ZONE2_END: u8 = 24;

/// 完全随机标签
const BOXES: [&str; 5] = [
    "完全随机A",
    "完全随机B",
    "完全随机C",
    "完全随机D",
    "完全随机E",
];

/// 线性同余随机数生成器（种子可控，用于加权随机策略的扰动）
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E3779B97F4A7C15),
        }
    }

    /// 生成 [0, 1) 之间的 f64
    fn next_f64(&mut self) -> f64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state ^= self.state >> 30;
        (self.state >> 11) as f64 / (1u64 << 53) as f64
    }

    /// 加权随机抽样：按权重比例从 items 中抽取 count 个不重复的元素
    fn weighted_sample(
        &mut self,
        items: &[(u8, f64)],
        count: usize,
    ) -> Vec<u8> {
        let mut result = Vec::new();
        let mut remaining = items.to_vec();

        while result.len() < count && !remaining.is_empty() {
            let total: f64 =
                remaining.iter().map(|(_, w)| w.max(0.001)).sum();
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

// ============================================================================
// 公共辅助函数
// ============================================================================

/// 按加权得分从高到低排序，返回前N个号码
fn pick_top(weighted: &HashMap<u8, f64>, count: usize) -> Vec<u8> {
    let mut items: Vec<(u8, f64)> =
        weighted.iter().map(|(&k, &v)| (k, v)).collect();
    items.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap()
            .then_with(|| a.0.cmp(&b.0))
    });
    items.into_iter().take(count).map(|(k, _)| k).collect()
}

// ============================================================================
// 6组确定性策略
// ============================================================================

/// 方案1: 纯热号 — 加权得分TOP5红球 + TOP2蓝球
fn pick_pure_hot(stats: &Stats) -> Pick {
    let red = pick_top(&stats.red_weighted, RED_COUNT);
    let blue = pick_top(&stats.blue_weighted, BLUE_COUNT);
    Pick::new(red, blue, "纯热号")
}

/// 方案2: 冷热混合 — 3个高加权 + 2个高遗漏回补
fn pick_hot_cold(stats: &Stats) -> Pick {
    let red = pick_red_hot_cold(
        &stats.red_weighted,
        &stats.red_omission,
        3,
        2,
    );
    let blue = pick_blue_hot_cold(
        &stats.blue_weighted,
        &stats.blue_omission,
    );
    Pick::new(red, blue, "冷热混合")
}

/// 方案3: 区间均衡 — 按历史各区间比例分配球数
fn pick_zone_balanced(stats: &Stats) -> Pick {
    let red =
        pick_red_by_zone(&stats.red_weighted, stats.zone_avg);
    let blue = pick_blue_odd_even(&stats.blue_weighted);
    Pick::new(red, blue, "区间均衡")
}

/// 方案4: 和值约束 — 在和值均值±0.5σ范围内找加权最优组合
fn pick_sum_constrained(stats: &Stats) -> Pick {
    let red = pick_red_by_sum(
        &stats.red_weighted,
        stats.sum_avg,
        stats.sum_stddev,
    );
    let blue = pick_top(&stats.blue_weighted, BLUE_COUNT);
    Pick::new(red, blue, "和值约束")
}

/// 方案5: 同尾约束（严格模式） — 5个红球尾数全部不同
fn pick_tail_constrained(stats: &Stats) -> Pick {
    let red = pick_red_strict_tail(&stats.red_weighted);
    let blue = pick_blue_different_tail(&stats.blue_weighted);
    Pick::new(red, blue, "同尾约束")
}

/// 方案6: 连号策略 — 包含一组最高加权相邻对，其余从TOP20中排除连号附近选取
fn pick_consecutive(stats: &Stats) -> Pick {
    let red = pick_red_consecutive(&stats.red_weighted);
    let blue = pick_blue_hot(&stats.blue_recent_hot);
    Pick::new(red, blue, "连号策略")
}

// ============================================================================
// 红球策略实现
// ============================================================================

/// 冷热混合：hot_count个高加权 + cold_count个高遗漏
fn pick_red_hot_cold(
    weighted: &HashMap<u8, f64>,
    omission: &[u32],
    hot_count: usize,
    cold_count: usize,
) -> Vec<u8> {
    let hot = pick_top(weighted, hot_count);
    let mut omission_items: Vec<(u8, u32)> = omission
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as u8 + 1, v))
        .filter(|(n, _)| !hot.contains(n))
        .collect();
    omission_items
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let mut cold: Vec<u8> = omission_items
        .into_iter()
        .take(cold_count)
        .map(|(k, _)| k)
        .collect();

    let mut result = hot;
    result.append(&mut cold);

    // 不足时用加权补足
    let needed = hot_count + cold_count;
    while result.len() < needed {
        let top = pick_top(weighted, result.len() + 1);
        for n in top {
            if !result.contains(&n) {
                result.push(n);
                break;
            }
        }
    }
    result.truncate(needed);
    result.sort();
    result
}

/// 区间均衡：按历史各区间出现比例分配球数，各区内按加权选取
fn pick_red_by_zone(
    weighted: &HashMap<u8, f64>,
    zone_avg: [f64; 3],
) -> Vec<u8> {
    let total_zone: f64 = zone_avg.iter().sum();
    if total_zone == 0.0 {
        return pick_top(weighted, RED_COUNT);
    }

    let mut zone_counts = [0usize; 3];
    let mut assigned = 0usize;
    for i in 0..3 {
        zone_counts[i] =
            (zone_avg[i] / total_zone * RED_COUNT as f64).round()
                as usize;
        assigned += zone_counts[i];
    }
    // 调整到正好等于RED_COUNT
    while assigned > RED_COUNT {
        let max_zone = zone_counts
            .iter()
            .enumerate()
            .max_by_key(|&(_, v)| v)
            .unwrap()
            .0;
        zone_counts[max_zone] -= 1;
        assigned -= 1;
    }
    while assigned < RED_COUNT {
        let max_zone = zone_counts
            .iter()
            .enumerate()
            .max_by_key(|&(_, v)| v)
            .unwrap()
            .0;
        zone_counts[max_zone] += 1;
        assigned += 1;
    }

    let mut result = Vec::new();
    let zones = [
        (1, ZONE1_END),
        (ZONE1_END + 1, ZONE2_END),
        (ZONE2_END + 1, RED_MAX),
    ];
    for (i, &(start, end)) in zones.iter().enumerate() {
        let mut zone_items: Vec<(u8, f64)> = weighted
            .iter()
            .filter(|(k, _)| **k >= start && **k <= end)
            .map(|(k, v)| (*k, *v))
            .collect();
        zone_items.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap()
                .then_with(|| a.0.cmp(&b.0))
        });
        for (k, _) in zone_items.into_iter().take(zone_counts[i]) {
            result.push(k);
        }
    }
    result.sort();
    result
}

/// 和值约束：在历史均值±σ范围内找加权最优组合
fn pick_red_by_sum(
    weighted: &HashMap<u8, f64>,
    sum_avg: f64,
    sum_stddev: f64,
) -> Vec<u8> {
    // 先尝试±0.5σ，无解则放宽到±1σ
    if let Some(pick) = find_sum_in_range(
        weighted,
        sum_avg - sum_stddev * 0.5,
        sum_avg + sum_stddev * 0.5,
    ) {
        return pick;
    }
    if let Some(pick) = find_sum_in_range(
        weighted,
        sum_avg - sum_stddev,
        sum_avg + sum_stddev,
    ) {
        return pick;
    }
    // 仍无解则退化为纯热号
    pick_top(weighted, RED_COUNT)
}

/// 在和值范围内找加权得分最高的组合（递归组合生成器）
fn find_sum_in_range(
    weighted: &HashMap<u8, f64>,
    min_sum: f64,
    max_sum: f64,
) -> Option<Vec<u8>> {
    let candidates = pick_top(weighted, 15);
    let mut best_pick: Option<Vec<u8>> = None;
    let mut best_score = f64::MIN;

    for combo in combinations(&candidates, 5) {
        let sum = combo.iter().map(|&n| n as f64).sum::<f64>();
        if sum >= min_sum && sum <= max_sum {
            let score: f64 = combo
                .iter()
                .map(|&n| weighted.get(&n).copied().unwrap_or(0.0))
                .sum();
            if score > best_score {
                best_score = score;
                best_pick = Some(combo);
            }
        }
    }
    best_pick.map(|mut p| {
        p.sort();
        p
    })
}

/// 递归组合生成器：从items中选k个元素的所有组合
fn combinations(items: &[u8], k: usize) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    combinations_helper(items, k, 0, &mut vec![], &mut result);
    result
}

fn combinations_helper(
    items: &[u8],
    k: usize,
    start: usize,
    current: &mut Vec<u8>,
    result: &mut Vec<Vec<u8>>,
) {
    if current.len() == k {
        result.push(current.clone());
        return;
    }
    for i in start..items.len() {
        current.push(items[i]);
        combinations_helper(items, k, i + 1, current, result);
        current.pop();
    }
}

/// 同尾约束（严格模式）：5个红球尾数全部不同，无解则放宽到最多1组同尾
fn pick_red_strict_tail(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let candidates = pick_top(weighted, 20);
    // 严格模式：每个尾数只取加权最高的那个
    if let Some(result) =
        greedy_strict_tail(&candidates, RED_COUNT)
    {
        return result;
    }
    // 放宽：允许最多1组同尾
    greedy_relaxed_tail(&candidates, RED_COUNT)
}

/// 严格同尾：每个尾数只选1个
fn greedy_strict_tail(
    candidates: &[u8],
    count: usize,
) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut used_tail = [false; 10];

    for &n in candidates {
        if result.len() >= count {
            break;
        }
        let tail = (n % 10) as usize;
        if !used_tail[tail] {
            result.push(n);
            used_tail[tail] = true;
        }
    }

    if result.len() >= count {
        result.truncate(count);
        result.sort();
        Some(result)
    } else {
        None
    }
}

/// 放宽同尾：最多允许1组同尾（即最多2个球共享一个尾数）
fn greedy_relaxed_tail(candidates: &[u8], count: usize) -> Vec<u8> {
    let mut result = Vec::new();
    let mut tail_count = [0u32; 10];

    for &n in candidates {
        if result.len() >= count {
            break;
        }
        let tail = (n % 10) as usize;
        if tail_count[tail] < 2 {
            // 检查是否已经有其他尾数占用了2个
            let double_tails =
                tail_count.iter().filter(|&&c| c >= 2).count();
            if double_tails == 0 || tail_count[tail] < 2 {
                result.push(n);
                tail_count[tail] += 1;
            }
        }
    }
    // 不足时放宽
    if result.len() < count {
        for &n in candidates {
            if !result.contains(&n) {
                result.push(n);
                if result.len() == count {
                    break;
                }
            }
        }
    }
    result.sort();
    result
}

/// 连号策略：锁定一组最高加权相邻对，剩余球从TOP20排除连号附近选取
fn pick_red_consecutive(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let top20 = pick_top(weighted, 20);

    // 找TOP20中加权最高的相邻对
    let mut best_pair: Option<(u8, u8)> = None;
    let mut best_pair_score = 0.0;

    for i in 0..top20.len() {
        for j in (i + 1)..top20.len() {
            if top20[j] == top20[i] + 1 {
                let score = weighted.get(&top20[i]).copied().unwrap_or(0.0)
                    + weighted.get(&top20[j]).copied().unwrap_or(0.0);
                if score > best_pair_score {
                    best_pair_score = score;
                    best_pair = Some((top20[i], top20[j]));
                }
            }
        }
    }

    let mut result = match best_pair {
        Some((a, b)) => vec![a, b],
        None => return pick_top(weighted, RED_COUNT), // 无连号则退化为纯热号
    };

    // 从TOP20中排除已选的和连号附近的号码，补足剩余
    let exclude_min = result[0].saturating_sub(1);
    let exclude_max = (result[1] + 2).min(RED_MAX);

    for &n in top20.iter() {
        if result.len() >= RED_COUNT {
            break;
        }
        if !result.contains(&n) && (n < exclude_min || n > exclude_max) {
            result.push(n);
        }
    }
    // 如果还不够，放宽排除范围
    if result.len() < RED_COUNT {
        for &n in top20.iter() {
            if result.len() >= RED_COUNT {
                break;
            }
            if !result.contains(&n) {
                result.push(n);
            }
        }
    }
    // 极端情况：TOP20仍不够，从全量补足
    if result.len() < RED_COUNT {
        let top35 = pick_top(weighted, RED_MAX as usize);
        for n in top35 {
            if result.len() >= RED_COUNT {
                break;
            }
            if !result.contains(&n) {
                result.push(n);
            }
        }
    }
    result.sort();
    result
}

// ============================================================================
// 蓝球策略实现
// ============================================================================

/// 蓝球：1高加权 + 1高遗漏
fn pick_blue_hot_cold(
    weighted: &HashMap<u8, f64>,
    omission: &[u32],
) -> Vec<u8> {
    let hot = pick_top(weighted, 1);
    let mut omission_items: Vec<(u8, u32)> = omission
        .iter()
        .enumerate()
        .map(|(i, &v)| (i as u8 + 1, v))
        .filter(|(n, _)| !hot.contains(n))
        .collect();
    omission_items.sort_by(|a, b| b.1.cmp(&a.1));
    let mut cold: Vec<u8> = omission_items
        .into_iter()
        .take(1)
        .map(|(k, _)| k)
        .collect();

    let mut result = hot;
    result.append(&mut cold);
    while result.len() < 2 {
        for i in 1..=BLUE_MAX {
            if !result.contains(&i) {
                result.push(i);
                break;
            }
        }
    }
    result.sort();
    result
}

/// 蓝球：一奇一偶优先
fn pick_blue_odd_even(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let top5 = pick_top(weighted, 5);
    let odd: Vec<u8> =
        top5.iter().filter(|&&n| n % 2 == 1).copied().collect();
    let even: Vec<u8> =
        top5.iter().filter(|&&n| n % 2 == 0).copied().collect();

    if !odd.is_empty() && !even.is_empty() {
        let mut r = vec![odd[0], even[0]];
        r.sort();
        return r;
    }
    top5.into_iter().take(2).collect::<Vec<_>>()
}

/// 蓝球：不同尾优先
fn pick_blue_different_tail(weighted: &HashMap<u8, f64>) -> Vec<u8> {
    let top = pick_top(weighted, BLUE_COUNT + 2);
    for i in 0..top.len() {
        for j in (i + 1)..top.len() {
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
fn pick_blue_hot(recent_hot: &[u8]) -> Vec<u8> {
    let mut result: Vec<u8> =
        recent_hot.iter().take(BLUE_COUNT).copied().collect();
    while result.len() < BLUE_COUNT {
        for i in 1..=BLUE_MAX {
            if !result.contains(&i) {
                result.push(i);
                break;
            }
        }
    }
    result.sort();
    result
}

// ============================================================================
// 加权随机策略
// ============================================================================

/// 加权随机抽样：从TOP pool_size候选中按权重随机抽取count个
fn pick_weighted_random(
    weighted: &HashMap<u8, f64>,
    pool_size: usize,
    count: usize,
    rng: &mut Rng,
) -> Vec<u8> {
    let candidates = pick_top(weighted, pool_size);
    let items: Vec<(u8, f64)> = candidates
        .iter()
        .map(|&n| (n, *weighted.get(&n).unwrap_or(&0.0)))
        .collect();
    rng.weighted_sample(&items, count)
}

// ============================================================================
// 完全随机策略
// ============================================================================

/// 完全随机选号：使用操作系统级加密安全随机源，模拟真实摇奖
fn pick_completely_random() -> (Vec<u8>, Vec<u8>) {
    let red = fisher_yates_sample(1, RED_MAX, RED_COUNT);
    let blue = fisher_yates_sample(1, BLUE_MAX, BLUE_COUNT);
    (red, blue)
}

/// Fisher-Yates 洗牌：从 [min, max] 中均匀随机抽取 count 个不重复的数
fn fisher_yates_sample(min: u8, max: u8, count: usize) -> Vec<u8> {
    let range = (max - min + 1) as usize;
    let mut pool: Vec<u8> = (min..=max).collect();

    for i in 0..count {
        let remaining = range - i;
        let mut buf = [0u8; 4];
        getrandom::fill(&mut buf).expect("获取系统随机失败");
        let j = i + (u32::from_ne_bytes(buf) as usize % remaining);
        pool.swap(i, j);
    }

    let mut result: Vec<u8> = pool.into_iter().take(count).collect();
    result.sort();
    result
}

// ============================================================================
// 主入口：生成全部推荐
// ============================================================================

/// 生成多组推荐号码（6组确定性 + 2组加权随机 + 5组完全随机）
pub fn generate_picks(stats: &Stats, seed: u64) -> Vec<Pick> {
    let mut picks = Vec::with_capacity(13);

    // 6组确定性策略
    picks.push(pick_pure_hot(stats));
    picks.push(pick_hot_cold(stats));
    picks.push(pick_zone_balanced(stats));
    picks.push(pick_sum_constrained(stats));
    picks.push(pick_tail_constrained(stats));
    picks.push(pick_consecutive(stats));

    // 2组加权随机扰动
    let mut rng_a = Rng::new(seed);
    let red7 = pick_weighted_random(&stats.red_weighted, 20, RED_COUNT, &mut rng_a);
    let blue7 = pick_weighted_random(&stats.blue_weighted, 8, BLUE_COUNT, &mut rng_a);
    picks.push(Pick::new(red7, blue7, "加权随机A"));

    let mut rng_b = Rng::new(seed.wrapping_add(1));
    let red8 = pick_weighted_random(&stats.red_weighted, 20, RED_COUNT, &mut rng_b);
    let blue8 = pick_weighted_random(&stats.blue_weighted, 8, BLUE_COUNT, &mut rng_b);
    picks.push(Pick::new(red8, blue8, "加权随机B"));

    // 5组完全随机
    for i in 0..5 {
        let (red, blue) = pick_completely_random();
        picks.push(Pick::new(red, blue, BOXES[i]));
    }

    picks
}
