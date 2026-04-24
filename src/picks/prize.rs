use daletou::DrawRecord;

/// 大乐透奖项等级
#[derive(Debug, Clone, Copy)]
pub enum PrizeLevel {
    First,   // 一等奖: 5红+2蓝
    Second,  // 二等奖: 5红+1蓝
    Third,   // 三等奖: 5红+0蓝
    Fourth,  // 四等奖: 4红+2蓝
    Fifth,   // 五等奖: 4红+1蓝
    Sixth,   // 六等奖: 3红+2蓝
    Seventh, // 七等奖: 4红+0蓝 / 2红+2蓝 / 3红+1蓝 / 1红+2蓝 / 0红+2蓝
    Eighth,  // 八等奖: 3红+0蓝 / 2红+1蓝 / 1红+1蓝 / 0红+1蓝
}

/// 中奖统计：索引1-8对应一至八等奖的命中次数
#[derive(Debug, Clone, Copy)]
pub struct PrizeStats {
    pub counts: [u32; 9],
}

impl PrizeStats {
    pub fn new() -> Self {
        Self { counts: [0; 9] }
    }

    pub fn record(&mut self, level: PrizeLevel) {
        let idx = match level {
            PrizeLevel::First => 1,
            PrizeLevel::Second => 2,
            PrizeLevel::Third => 3,
            PrizeLevel::Fourth => 4,
            PrizeLevel::Fifth => 5,
            PrizeLevel::Sixth => 6,
            PrizeLevel::Seventh => 7,
            PrizeLevel::Eighth => 8,
        };
        self.counts[idx] += 1;
    }

    /// 格式化输出：1等:0 2等:0 ... 8等:0
    pub fn display(&self) -> String {
        format!(
            "1等:{} 2等:{} 3等:{} 4等:{} 5等:{} 6等:{} 7等:{} 8等:{}",
            self.counts[1],
            self.counts[2],
            self.counts[3],
            self.counts[4],
            self.counts[5],
            self.counts[6],
            self.counts[7],
            self.counts[8],
        )
    }
}

/// 单期历史开奖位图
struct HistoryEntry {
    /// 红球位掩码：bit1..bit35 对应号码1..35
    red_mask: u64,
    /// 蓝球号码
    blue: [u8; 2],
}

/// 历史开奖位图索引（不透明类型）
pub struct PrizeIndex {
    entries: Vec<HistoryEntry>,
}

/// 从历史开奖记录构建位图索引（用于快速匹配）
pub fn build_prize_index(records: &[DrawRecord]) -> PrizeIndex {
    let entries = records
        .iter()
        .map(|r| {
            let mut red_mask: u64 = 0;
            for &n in &r.balls.red {
                red_mask |= 1u64 << n;
            }
            let blue = [r.balls.blue[0], r.balls.blue[1]];
            HistoryEntry { red_mask, blue }
        })
        .collect();
    PrizeIndex { entries }
}

/// 对一组推荐号码，在历史位图索引中统计各等奖命中次数
pub fn compute_prize_stats(
    index: &PrizeIndex,
    red: &[u8; 5],
    blue: &[u8; 2],
) -> PrizeStats {
    // 构建推荐号码的红球位掩码
    let pick_red_mask: u64 = {
        let mut m = 0u64;
        for &n in red {
            m |= 1u64 << n;
        }
        m
    };

    let mut stats = PrizeStats::new();

    for entry in &index.entries {
        // 位运算求红球命中数：按位与后统计1的个数
        let red_hits = (pick_red_mask & entry.red_mask).count_ones() as u8;
        // 蓝球命中数
        let blue_hits = count_blue_hits(&entry.blue, blue);

        if let Some(level) = classify_prize(red_hits, blue_hits) {
            stats.record(level);
        }
    }

    stats
}

/// 统计蓝球命中数（仅2个球，直接比较）
fn count_blue_hits(history: &[u8; 2], pick: &[u8; 2]) -> u8 {
    let mut hits = 0;
    for &b in pick {
        if history.contains(&b) {
            hits += 1;
        }
    }
    hits
}

/// 根据红球和蓝球命中数判定奖项等级
/// 返回 None 表示未中奖
fn classify_prize(red_hits: u8, blue_hits: u8) -> Option<PrizeLevel> {
    match (red_hits, blue_hits) {
        (5, 2) => Some(PrizeLevel::First),
        (5, 1) => Some(PrizeLevel::Second),
        (5, 0) => Some(PrizeLevel::Third),
        (4, 2) => Some(PrizeLevel::Fourth),
        (4, 1) => Some(PrizeLevel::Fifth),
        (3, 2) => Some(PrizeLevel::Sixth),
        // 七等奖
        (4, 0) | (2, 2) | (3, 1) | (1, 2) | (0, 2) => Some(PrizeLevel::Seventh),
        // 八等奖
        (3, 0) | (2, 1) | (1, 1) | (0, 1) => Some(PrizeLevel::Eighth),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prize_classification() {
        assert!(matches!(classify_prize(5, 2), Some(PrizeLevel::First)));
        assert!(matches!(classify_prize(5, 1), Some(PrizeLevel::Second)));
        assert!(matches!(classify_prize(5, 0), Some(PrizeLevel::Third)));
        assert!(matches!(classify_prize(4, 2), Some(PrizeLevel::Fourth)));
        assert!(matches!(classify_prize(4, 1), Some(PrizeLevel::Fifth)));
        assert!(matches!(classify_prize(3, 2), Some(PrizeLevel::Sixth)));
        // 七等奖多种情况
        assert!(matches!(classify_prize(4, 0), Some(PrizeLevel::Seventh)));
        assert!(matches!(classify_prize(2, 2), Some(PrizeLevel::Seventh)));
        assert!(matches!(classify_prize(3, 1), Some(PrizeLevel::Seventh)));
        assert!(matches!(classify_prize(1, 2), Some(PrizeLevel::Seventh)));
        assert!(matches!(classify_prize(0, 2), Some(PrizeLevel::Seventh)));
        // 八等奖
        assert!(matches!(classify_prize(3, 0), Some(PrizeLevel::Eighth)));
        assert!(matches!(classify_prize(2, 1), Some(PrizeLevel::Eighth)));
        assert!(matches!(classify_prize(1, 1), Some(PrizeLevel::Eighth)));
        assert!(matches!(classify_prize(0, 1), Some(PrizeLevel::Eighth)));
        // 未中奖
        assert!(classify_prize(0, 0).is_none());
        assert!(classify_prize(1, 0).is_none());
        assert!(classify_prize(2, 0).is_none());
    }

    #[test]
    fn test_exact_match_first_prize() {
        let records: Vec<DrawRecord> = vec![
            DrawRecord {
                issue: "26001".to_string(),
                date: "2026-01-01".to_string(),
                weekday: "四".to_string(),
                balls: daletou::BallSet {
                    red: vec![1, 2, 3, 4, 5],
                    blue: vec![6, 7],
                },
                prize_pool: "1000万".to_string(),
            },
        ];

        let index = build_prize_index(&records);
        let stats = compute_prize_stats(
            &index,
            &[1, 2, 3, 4, 5],
            &[6, 7],
        );
        assert_eq!(stats.counts[1], 1); // 一等奖命中1次
    }

    #[test]
    fn test_no_match() {
        let records: Vec<DrawRecord> = vec![
            DrawRecord {
                issue: "26001".to_string(),
                date: "2026-01-01".to_string(),
                weekday: "四".to_string(),
                balls: daletou::BallSet {
                    red: vec![1, 2, 3, 4, 5],
                    blue: vec![6, 7],
                },
                prize_pool: "1000万".to_string(),
            },
        ];

        let index = build_prize_index(&records);
        let stats = compute_prize_stats(
            &index,
            &[30, 31, 32, 33, 34],
            &[11, 12],
        );
        // 完全不中
        assert_eq!(stats.counts.iter().sum::<u32>(), 0);
    }

    #[test]
    fn test_eighth_prize_one_blue() {
        let records: Vec<DrawRecord> = vec![
            DrawRecord {
                issue: "26001".to_string(),
                date: "2026-01-01".to_string(),
                weekday: "四".to_string(),
                balls: daletou::BallSet {
                    red: vec![1, 2, 3, 4, 5],
                    blue: vec![6, 7],
                },
                prize_pool: "1000万".to_string(),
            },
        ];

        let index = build_prize_index(&records);
        // 红球不中，蓝球中1个 -> 八等奖
        let stats = compute_prize_stats(
            &index,
            &[30, 31, 32, 33, 34],
            &[6, 12],
        );
        assert_eq!(stats.counts[8], 1);
    }
}
