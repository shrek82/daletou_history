# 启动参数

`ai_pick` 支持以下启动参数：

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `--server` | flag | 关闭 | 启动 HTTP 服务模式 |
| `--port` | integer | `8888` | HTTP 服务监听端口 |
| `--max-records` | integer | `365` | 数据库初始化时最多爬取条数，以及数据库最多保留记录数 |

## 启动示例

```bash
# 终端模式，打印分析结果和推荐
cargo run --example ai_pick

# HTTP 服务模式，默认端口 8888
cargo run --example ai_pick -- --server

# HTTP 服务模式，指定端口 9000
cargo run --example ai_pick -- --server --port 9000

# HTTP 服务模式，指定最多保留 100 条记录
cargo run --example ai_pick -- --server --max-records 100

# 组合使用
cargo run --example ai_pick -- --server --port 9000 --max-records 500
```

## 行为说明

### 数据库为空（首次启动）

启动后自动从网络爬取数据，爬取页数由 `--max-records` 决定：

- 默认 365 条 → 约 13 页（每页约 30 条）
- 指定 30 条 → 1 页

### 数据库有数据（后续启动）

- 检查是否到了爬取间隔（默认 1 小时）
- 如果到了间隔，自动更新第一页（增量更新）
- 从数据库读取 `--max-records` 条记录用于分析

### HTTP 服务模式

启动后台自动更新线程，每隔 1 小时自动更新第一页号码。

可用接口：

```
GET /health                  - 健康检查
GET /api/draws               - 分页查询开奖记录
GET /api/draws?issue=26001   - 按期号查询
GET /api/picks               - 返回全部推荐
GET /api/pick?strategy=hot   - 返回指定策略
```
