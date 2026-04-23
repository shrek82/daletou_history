use daletou::Client;

fn main() {
    let client = Client::new();

    // 获取前 50 条最新记录
    match client.get_latest_n(50) {
        Ok(records) => {
            println!("=== 最新 {} 条记录 ===", records.len());
            for (i, r) in records.iter().enumerate() {
                print!("{:3}. {} {}: ", i + 1, r.issue, r.date);
                for b in &r.balls.red {
                    print!("{:02} ", b);
                }
                print!("+ ");
                for b in &r.balls.blue {
                    print!("{:02} ", b);
                }
                println!();
            }
        }
        Err(e) => eprintln!("获取失败: {}", e),
    }

    // 获取前 3 页所有记录
    match client.get_pages(3) {
        Ok(records) => {
            println!("\n=== 前 3 页共 {} 条记录 ===", records.len());
        }
        Err(e) => eprintln!("获取失败: {}", e),
    }
}
