# daletou

大乐透开奖信息查询 Rust 包，支持实时获取最新开奖号码和历史开奖记录。

## 安装

`Cargo.toml` 中添加依赖：

```toml
[dependencies]
daletou = { git = "https://github.com/shrek82/daletou_history.git" }
# 或本地引用: daletou = { path = "../daletou_kaijiang" }
```

## 快速开始

### 获取最新一期

```rust
use daletou::Client;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let record = client.get_latest()?;

    println!("{}期: {}", record.issue, record.date);
    print!("红球: ");
    for n in &record.balls.red { print!("{:02} ", n); }
    print!("\n蓝球: ");
    for n in &record.balls.blue { print!("{:02} ", n); }
    println!("\n奖池: {}", record.prize_pool);

    Ok(())
}
```

### 获取单页记录

```rust
let client = Client::new();
let page = client.get_page(1)?;

println!("共 {} 页", page.total_pages);
for record in &page.records {
    println!("{}期 {} {:?}", record.issue, record.date, record.balls);
}
```

### 获取指定数量的最新记录

```rust
let client = Client::new();

// 获取最新 50 条（自动计算需要请求几页）
let records = client.get_latest_n(50)?;

for (i, r) in records.iter().enumerate() {
    println!("{:3}. {} {}: {:?}", i + 1, r.issue, r.date, r.balls);
}
```

### 获取前 N 页所有记录

```rust
let client = Client::new();
let records = client.get_pages(3)?;  // 约 90 条

println!("共 {} 条记录", records.len());
```

### 导出为 JSON

```rust
let client = Client::new();
let record = client.get_latest()?;
let json = serde_json::to_string_pretty(&record)?;
println!("{}", json);
```

### 自定义请求间隔

默认请求间隔为 2 秒，可通过构造器调整：

```rust
use std::time::Duration;

let client = Client::new()
    .with_request_interval(Duration::from_secs(5));  // 改为 5 秒
```

`get_pages` 和 `get_latest_n` 在页间自动等待，防止被封。

## API 文档

| 方法 | 参数 | 返回值 | 说明 |
|------|------|--------|------|
| `Client::new()` | - | `Client` | 创建查询客户端 |
| `get_latest()` | - | `Result<DrawRecord>` | 获取最新一期开奖信息 |
| `get_page(page)` | `page: u32` | `Result<DrawPage>` | 获取指定页的开奖记录 |
| `get_latest_n(n)` | `n: usize` | `Result<Vec<DrawRecord>>` | 获取最新 N 条记录 |
| `get_pages(n)` | `n: u32` | `Result<Vec<DrawRecord>>` | 获取前 N 页所有记录 |

## 数据结构

```rust
struct DrawRecord {
    issue: String,      // 期号，如 "2026043"
    date: String,       // 开奖日期，如 "2026-04-22"
    weekday: String,    // 星期，如 "三"
    balls: BallSet,     // 开奖号码
    prize_pool: String, // 奖池金额
}

struct BallSet {
    red: Vec<u8>,   // 5个红球 (1-35)
    blue: Vec<u8>,  // 2个蓝球 (1-12)
}

struct DrawPage {
    current_page: u32,     // 当前页码
    total_pages: u32,      // 总页数
    records: Vec<DrawRecord>,
}
```

## 运行示例

```bash
cargo run --example latest                # 最新开奖
cargo run --example history               # 默认第1页
cargo run --example history -- -p 5       # 前5页
cargo run --example history -- -n 50      # 最新50条
cargo run --example latest_n              # 获取最新50条（代码示例）
cargo run --example history -- --help     # 查看帮助
```

## 运行测试

```bash
cargo test    # 需要联网
```

## License

MIT
