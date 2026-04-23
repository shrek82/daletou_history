use daletou::Client;

/// 集成测试：验证真实网页解析
#[test]
fn test_fetch_latest_draw() {
    let client = Client::new();
    let record = client.get_latest().expect("获取最新开奖失败");

    assert!(!record.issue.is_empty(), "期号不应为空");
    assert!(record.issue.chars().all(|c| c.is_ascii_digit()), "期号应为纯数字");
    assert_eq!(record.balls.red.len(), 5, "红球应为5个");
    assert_eq!(record.balls.blue.len(), 2, "蓝球应为2个");
    // 红球范围 01-35
    for r in &record.balls.red {
        assert!(*r >= 1 && *r <= 35, "红球应在 1-35 范围内: {}", r);
    }
    // 蓝球范围 01-12
    for b in &record.balls.blue {
        assert!(*b >= 1 && *b <= 12, "蓝球应在 1-12 范围内: {}", b);
    }
}

#[test]
fn test_fetch_page() {
    let client = Client::new();
    let page = client.get_page(1).expect("获取第1页失败");

    assert_eq!(page.current_page, 1);
    assert!(!page.records.is_empty(), "第1页应有开奖记录");
    assert!(page.total_pages > 0, "总页数应大于0");
}

#[test]
fn test_fetch_page_2() {
    let client = Client::new();
    let page = client.get_page(2).expect("获取第2页失败");

    assert_eq!(page.current_page, 2);
    assert!(!page.records.is_empty(), "第2页应有开奖记录");
}

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
