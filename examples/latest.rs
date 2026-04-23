use daletou::Client;

fn main() {
    let client = Client::new();

    // 获取最新一期
    match client.get_latest() {
        Ok(record) => {
            println!("=== 最新开奖 ===");
            println!("期号: {}", record.issue);
            println!("日期: {} ({})", record.date, record.weekday);
            print!("红球: ");
            for r in &record.balls.red {
                print!("{:02} ", r);
            }
            println!();
            print!("蓝球: ");
            for b in &record.balls.blue {
                print!("{:02} ", b);
            }
            println!();
            println!("奖池: {}", record.prize_pool);
        }
        Err(e) => eprintln!("获取失败: {}", e),
    }
}
