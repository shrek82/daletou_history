use daletou::Client;

fn main() {
    let client = Client::new();

    // 获取前3页历史记录
    for page_num in 1..=3 {
        match client.get_page(page_num) {
            Ok(page) => {
                println!("\n=== 第 {} 页 (共 {} 页) ===", page.current_page, page.total_pages);
                for record in &page.records {
                    print!("{}期 {}: ", record.issue, record.date);
                    for r in &record.balls.red {
                        print!("{:02} ", r);
                    }
                    print!("+ ");
                    for b in &record.balls.blue {
                        print!("{:02} ", b);
                    }
                    println!();
                }
            }
            Err(e) => eprintln!("第 {} 页获取失败: {}", page_num, e),
        }
    }
}
