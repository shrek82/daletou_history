# daletou

大乐透开奖信息查询 Rust 包，一行代码获取最新开奖号码和历史记录。

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

    println!("{}期 红球{:?} 蓝球{:?} 奖池:{}",
        record.issue, record.balls.red, record.balls.blue, record.prize_pool);

    Ok(())
}
```

### 获取指定数量的最新记录

```rust
let client = Client::new();
let records = client.get_latest_n(100)?;  // 最新100条，页间自动请求

for r in &records {
    println!("{}期 {:?} + {:?}", r.date, r.balls.red, r.balls.blue);
}
```

### 获取前 N 页所有记录

```rust
let client = Client::new();
let records = client.get_pages(5)?;  // 约150条
```

### 获取指定页码

```rust
let client = Client::new();
let page = client.get_page(3)?;

println!("第{}页/共{}页，本{}条记录",
    page.current_page, page.total_pages, page.records.len());
```

### 自定义请求间隔

```rust
use std::time::Duration;

let client = Client::new()
    .with_request_interval(Duration::from_secs(5));
```

`get_pages` 和 `get_latest_n` 在页间自动等待，防止被封。

### 导出为 JSON

```rust
let json = serde_json::to_string_pretty(&client.get_latest()?)?;
```

## API 一览

```rust
Client::new()                                         -> Client
Client.with_request_interval(Duration)                -> Client          // 链式调用

Client.get_latest()                                   -> Result<DrawRecord>
Client.get_page(page: u32)                            -> Result<DrawPage>
Client.get_latest_n(n: usize)                         -> Result<Vec<DrawRecord>>
Client.get_pages(count: u32)                          -> Result<Vec<DrawRecord>>
```

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
cargo run --example history -- -p 5       # 前5页（页间自动延迟2秒）
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
