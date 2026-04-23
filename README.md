# daletou

大乐透开奖信息查询 Rust 包，支持实时获取最新开奖号码和历史开奖记录。

## 安装

`Cargo.toml` 中添加依赖：

```toml
[dependencies]
daletou = { path = "../daletou_kaijiang" }
# 或发布后: daletou = "0.1"
```

## 使用

### 获取最新开奖

```rust
use daletou::Client;

let client = Client::new();
let record = client.get_latest()?;
println!("{}期: 红球 {:?} 蓝球 {:?}", record.issue, record.balls.red, record.balls.blue);
```

### 获取历史记录

```rust
use daletou::Client;

let client = Client::new();
// 获取第1页（最新）
let page = client.get_page(1)?;
println!("共 {} 页", page.total_pages);

for record in &page.records {
    println!("{}期 {} {:?}", record.issue, record.date, record.balls);
}
```

### 导出为 JSON

```rust
use daletou::Client;

let client = Client::new();
let record = client.get_latest()?;
let json = serde_json::to_string_pretty(&record)?;
println!("{}", json);
```

## 数据结构

- `DrawRecord`: 单期开奖记录（期号、日期、红球、蓝球、奖池）
- `DrawPage`: 一页记录（当前页、总页数、记录列表）
- `BallSet`: 红球+蓝球组合

## 运行示例

```bash
cargo run --example latest    # 最新开奖
cargo run --example history   # 历史记录
```

## 运行测试

```bash
cargo test    # 需要联网
```

## License

MIT
