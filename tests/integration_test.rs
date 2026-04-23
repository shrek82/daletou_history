use daletou::Client;

/// 验证最新开奖记录的数据完整性
#[test]
fn test_fetch_latest_draw() {
    let client = Client::new();
    let record = client.get_latest().expect("获取最新开奖失败");

    assert!(!record.issue.is_empty(), "期号不应为空");
    assert!(record.issue.chars().all(|c| c.is_ascii_digit()), "期号应为纯数字");
    assert_eq!(record.balls.red.len(), 5, "红球应为5个");
    assert_eq!(record.balls.blue.len(), 2, "蓝球应为2个");
    for r in &record.balls.red {
        assert!(*r >= 1 && *r <= 35, "红球应在 1-35 范围内: {}", r);
    }
    for b in &record.balls.blue {
        assert!(*b >= 1 && *b <= 12, "蓝球应在 1-12 范围内: {}", b);
    }
}

/// 验证单页解析（含总页数、记录数等），复用 latest 的请求避免并发竞争
#[test]
fn test_fetch_page() {
    let client = Client::new();
    let page = client.get_page(1).expect("获取第1页失败");

    assert_eq!(page.current_page, 1);
    assert!(!page.records.is_empty(), "第1页应有开奖记录");
    assert!(page.total_pages > 0, "总页数应大于0");
}

/// 验证第2页解析正常
#[test]
fn test_fetch_page_2() {
    let client = Client::new();
    let page = client.get_page(2).expect("获取第2页失败");

    assert_eq!(page.current_page, 2);
    assert!(!page.records.is_empty(), "第2页应有开奖记录");
}

/// 验证 JSON 序列化/反序列化
#[test]
fn test_json_serialization() {
    let client = Client::new();
    let record = client.get_latest().expect("获取最新开奖失败");

    let json = serde_json::to_string(&record).expect("序列化失败");
    assert!(!json.is_empty());

    let decoded: daletou::DrawRecord =
        serde_json::from_str(&json).expect("反序列化失败");
    assert_eq!(decoded.issue, record.issue);
}

/// 验证批量获取接口
#[test]
fn test_get_latest_n() {
    let client = Client::new();
    let records = client.get_latest_n(10).expect("获取最新10条失败");

    assert_eq!(records.len(), 10);
    // 验证按期号降序排列
    for i in 1..records.len() {
        assert!(records[i - 1].issue > records[i].issue,
            "期号应降序排列: {} > {}", records[i - 1].issue, records[i].issue);
    }
}

/// 验证多页获取接口
#[test]
fn test_get_pages() {
    let client = Client::new();
    let records = client.get_pages(2).expect("获取前2页失败");

    assert_eq!(records.len(), 60, "2页应为60条记录");
}
