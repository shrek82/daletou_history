use std::fs;
use std::time::Instant;

fn main() {
    // 读取本地保存的HTML文件，测试纯解析性能
    let bytes = fs::read("/tmp/bench_page1.html")
        .expect("请先运行: curl -s https://www.cjcp.cn/dltkaijiang/ -o /tmp/bench_page1.html");
    let (cow, _, _) = encoding_rs::GB18030.decode(&bytes);
    let html: String = cow.into_owned();

    let iterations = 100;
    let start = Instant::now();

    for _ in 0..iterations {
        let doc = scraper::Html::parse_document(&html);

        let line_sel = scraper::Selector::parse("div.table-line").unwrap();
        let red_sel = scraper::Selector::parse("div.red-ball").unwrap();
        let blue_sel = scraper::Selector::parse("div.blue-ball").unwrap();

        let mut count = 0;
        for line in doc.select(&line_sel) {
            let red: Vec<u8> = line.select(&red_sel)
                .filter_map(|el| el.text().next()?.trim().parse().ok())
                .collect();
            let blue: Vec<u8> = line.select(&blue_sel)
                .filter_map(|el| el.text().next()?.trim().parse().ok())
                .collect();
            if !red.is_empty() || !blue.is_empty() {
                count += 1;
            }
        }
        assert_eq!(count, 30);
    }

    let elapsed = start.elapsed();
    println!("解析 {} 次, 总耗时: {:.2}ms, 平均: {:.3}μs/次",
        iterations,
        elapsed.as_micros() as f64 / 1000.0,
        elapsed.as_micros() as f64 / iterations as f64);
}
