# GET /api/pick

返回指定策略的单个推荐方案。

## 请求

```
GET /api/pick?strategy=<策略名>
```

## 查询参数

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `strategy` | string | 是 | 策略名称，见下方可用策略列表 |

## 可用策略

| 策略名 | 标签 | 说明 |
|--------|------|------|
| `hot` | 纯热号 | 选取近期出现频率最高的号码 |
| `hot_cold` | 冷热混合 | 热号为主，搭配少量遗漏号码 |
| `zone` | 区间均衡 | 三个区间按比例选取 |
| `sum` | 和值约束 | 选取和值在历史常见范围内的组合 |
| `tail` | 同尾约束 | 包含同尾号组合 |
| `consecutive` | 连号策略 | 包含连号组合 |
| `weighted_random_a` | 加权随机A | 基于频率加权随机 |
| `weighted_random_b` | 加权随机B | 另一种加权随机策略 |
| `random_a` ~ `random_e` | 完全随机A~E | 基于操作系统级加密随机源 |
| `fixed` | 固定号码 | 固定号码 02 22 30 33 34 + 08 12 |

## 响应

**状态码**: `200 OK`

返回单个 Pick 对象（见 `/api/picks` 文档中的 Pick 对象定义）：

```json
{
  "index": 1,
  "red": [5, 12, 18, 27, 33],
  "blue": [3, 9],
  "score": 78.5,
  "label": "纯热号",
  "prize_stats": {
    "1": 2,
    "2": 5,
    "3": 10,
    "4": 15,
    "5": 25,
    "6": 30,
    "7": 50,
    "8": 80
  }
}
```

**固定号码策略** 格式与策略一致：

```json
{
  "index": 1,
  "red": [2, 22, 30, 33, 34],
  "blue": [8, 12],
  "score": 65.2,
  "label": "固定号码",
  "prize_stats": {
    "1": 1,
    "2": 3,
    "3": 8,
    "4": 12,
    "5": 20,
    "6": 25,
    "7": 40,
    "8": 65
  }
}
```

## 错误响应

**400 Bad Request** — 缺少 strategy 参数
```json
{
  "error": "缺少 strategy 参数，例如: /api/pick?strategy=hot"
}
```

**404 Not Found** — 策略不存在
```json
{
  "error": "策略 'unknown' 不存在"
}
```
```json
{
  "error": "未知策略: xxx\n可用策略: hot, hot_cold, zone, sum, tail, consecutive, weighted_random_a, weighted_random_b, random_a~e, fixed"
}
```

## 示例

```bash
# 获取纯热号推荐
curl 'http://localhost:8888/api/pick?strategy=hot'

# 获取固定号码推荐
curl 'http://localhost:8888/api/pick?strategy=fixed'

# 获取完全随机A
curl 'http://localhost:8888/api/pick?strategy=random_a'

# 获取和值约束推荐
curl 'http://localhost:8888/api/pick?strategy=sum'
```
