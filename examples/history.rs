use daletou::Client;
use std::env;
use std::time::Duration;

fn print_record(r: &daletou::DrawRecord) {
    print!("{}期 {}: ", r.issue, r.date);
    for b in &r.balls.red {
        print!("{:02} ", b);
    }
    print!("+ ");
    for b in &r.balls.blue {
        print!("{:02} ", b);
    }
    println!();
}

fn main() {
    let args: Vec<String> = env::args().collect();
    // 请求间延迟 2 秒，防止被封
    let delay = Duration::from_secs(2);

    let mut pages: Option<u32> = None;
    let mut count: Option<usize> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-p" | "--pages" => {
                if i + 1 < args.len() {
                    pages = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    eprintln!("错误: {} 需要参数值", args[i]);
                    return;
                }
            }
            "-n" | "--count" => {
                if i + 1 < args.len() {
                    count = args[i + 1].parse().ok();
                    i += 2;
                } else {
                    eprintln!("错误: {} 需要参数值", args[i]);
                    return;
                }
            }
            "-h" | "--help" => {
                println!("用法: history [选项]");
                println!();
                println!("选项:");
                println!("  -p, --pages <N>    获取前 N 页历史记录（每页约30条）");
                println!("  -n, --count <N>    获取最新 N 条记录");
                println!("  -h, --help         显示帮助");
                println!();
                println!("示例:");
                println!("  cargo run --example history");
                println!("  cargo run --example history -- -p 5");
                println!("  cargo run --example history -- -n 50");
                return;
            }
            _ => {
                eprintln!("未知参数: {}", args[i]);
                eprintln!("使用 --help 查看用法");
                return;
            }
        }
    }

    let client = Client::new();

    if let Some(n) = count {
        match client.get_latest_n(n) {
            Ok(records) => {
                println!("=== 最新 {} 条记录 ===", records.len());
                for r in &records {
                    print_record(r);
                }
            }
            Err(e) => eprintln!("获取失败: {}", e),
        }
    } else {
        let num_pages = pages.unwrap_or(1);
        for page_num in 1..=num_pages {
            if page_num > 1 {
                std::thread::sleep(delay);
            }
            match client.get_page(page_num) {
                Ok(page) => {
                    println!("\n=== 第 {} 页 (共 {} 页) ===", page.current_page, page.total_pages);
                    for record in &page.records {
                        print_record(record);
                    }
                }
                Err(e) => eprintln!("第 {} 页获取失败: {}", page_num, e),
            }
        }
    }
}
