# GET /api/draws

开奖记录查询接口，支持分页查询和按期号精确查询。

## 请求

```
GET /api/draws[?page=<页码>&page_size=<每页条数>&issue=<期号>]
```

## 查询参数

| 参数 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `page` | integer | 否 | `1` | 页码，从 1 开始 |
| `page_size` | integer | 否 | `20` | 每页条数，范围 1~100 |
| `issue` | string | 否 | — | 期号，如 `"26001"`。指定后忽略分页参数 |

**优先级**: `issue` > `page` + `page_size`。如果同时指定了 `issue` 和分页参数，只按期号查询。

## 响应

### 分页查询（未指定 issue）

**状态码**: `200 OK`

```json
{
  "total": 365,
  "page": 1,
  "page_size": 20,
  "total_pages": 19,
  "records": [
    {
      "issue": "26030",
      "date": "2026-04-22",
      "weekday": "三",
      "red": [5, 12, 18, 27, 33],
      "blue": [3, 9],
      "prize_pool": "123456789元"
    },
    {
      "issue": "26029",
      "date": "2026-04-20",
      "weekday": "一",
      "red": [2, 15, 20, 28, 35],
      "blue": [1, 11],
      "prize_pool": "98765432元"
    }
  ]
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `total` | integer | 数据库总记录数 |
| `page` | integer | 当前页码 |
| `page_size` | integer | 每页条数 |
| `total_pages` | integer | 总页数（按 page_size 计算） |
| `records` | array | 开奖记录列表，按日期倒序排列 |

#### 开奖记录对象 (DrawRecordJson)

| 字段 | 类型 | 说明 |
|------|------|------|
| `issue` | string | 期号，如 `"26030"` |
| `date` | string | 开奖日期，格式 `YYYY-MM-DD` |
| `weekday` | string | 星期，如 `"三"` |
| `red` | array[int] | 5 个红球号码 |
| `blue` | array[int] | 2 个蓝球号码 |
| `prize_pool` | string | 奖池金额（原始字符串） |

### 按期号查询（指定 issue）

**状态码**: `200 OK`

```json
{
  "issue": "26030",
  "date": "2026-04-22",
  "weekday": "三",
  "red": [5, 12, 18, 27, 33],
  "blue": [3, 9],
  "prize_pool": "123456789元"
}
```

返回单个开奖记录对象。如果期号不存在，返回 `404`。

## 错误响应

**400 Bad Request** — 参数校验失败
```json
{
  "error": "page_size 必须在 1~100 之间"
}
```
```json
{
  "error": "page 必须 >= 1"
}
```

**404 Not Found** — 指定期号不存在
```json
{
  "error": "未找到期号为 '99999' 的记录"
}
```

**500 Internal Server Error** — 数据库查询失败
```json
{
  "error": "数据库查询失败: ..."
}
```

## 示例

```bash
# 分页查询：第 1 页，每页 20 条
curl 'http://localhost:8888/api/draws?page=1&page_size=20'

# 分页查询：第 3 页，每页 10 条
curl 'http://localhost:8888/api/draws?page=3&page_size=10'

# 按期号查询
curl 'http://localhost:8888/api/draws?issue=26030'

# 获取第 1 页全部数据
curl 'http://localhost:8888/api/draws'
```
