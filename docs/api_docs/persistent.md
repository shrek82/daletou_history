# 守号方案 API

守号方案允许用户保存一组或多组号码，进行评分、中奖统计分析和历史结果查询。

## 接口索引

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/persistent` | 新增守号方案 |
| GET | `/api/persistent` | 守号方案列表 |
| GET | `/api/persistent/<id>` | 查询单个方案 |
| PUT | `/api/persistent/<id>` | 修改方案 |
| DELETE | `/api/persistent/<id>` | 删除方案 |
| GET | `/api/persistent/<id>/analysis` | 方案分析（评分 + 中奖统计） |
| GET | `/api/persistent/<id>/history?n=30` | 结果查询（最新 N 期是否中奖） |

---

## 新增守号方案

**POST** `/api/persistent`

### 请求体

```json
{
  "name": "我的守号",
  "red": [2, 22, 30, 33, 34],
  "blue": [8, 12]
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `name` | string | 是 | 方案名称 |
| `red` | array[int] | 是 | 5 个红球号码（1-35，不重复） |
| `blue` | array[int] | 是 | 2 个蓝球号码（1-12，不重复） |

### 响应

**状态码**: `201 Created`

```json
{
  "id": 1,
  "name": "我的守号",
  "red": [2, 22, 30, 33, 34],
  "blue": [8, 12],
  "created_at": 1713700000
}
```

### 错误响应

**400 Bad Request** — 号码校验失败
```json
{
  "error": "红球必须为 5 个，当前 4 个"
}
```
```json
{
  "error": "红球存在重复号码: 22"
}
```

---

## 守号方案列表

**GET** `/api/persistent`

### 响应

**状态码**: `200 OK`

```json
{
  "total": 3,
  "picks": [
    {
      "id": 1,
      "name": "我的守号",
      "red": [2, 22, 30, 33, 34],
      "blue": [8, 12],
      "created_at": 1713700000
    },
    {
      "id": 2,
      "name": "生日号码",
      "red": [1, 6, 15, 20, 28],
      "blue": [3, 9],
      "created_at": 1713700100
    }
  ]
}
```

---

## 查询单个方案

**GET** `/api/persistent/<id>`

### 响应

**状态码**: `200 OK`

```json
{
  "id": 1,
  "name": "我的守号",
  "red": [2, 22, 30, 33, 34],
  "blue": [8, 12],
  "created_at": 1713700000
}
```

**404 Not Found** — 方案不存在
```json
{
  "error": "守号方案 ID=999 不存在"
}
```

---

## 修改守号方案

**PUT** `/api/persistent/<id>`

### 请求体

所有字段可选，只更新提供的字段：

```json
{
  "name": "新名称",
  "red": [5, 12, 18, 27, 33],
  "blue": [4, 10]
}
```

### 响应

**状态码**: `200 OK`

返回更新后的完整方案对象。

---

## 删除守号方案

**DELETE** `/api/persistent/<id>`

### 响应

**状态码**: `200 OK`

```json
{
  "ok": true,
  "id": 1
}
```

**404 Not Found** — 方案不存在

---

## 方案分析

**GET** `/api/persistent/<id>/analysis`

基于历史数据对守号方案进行评分和中奖统计。

### 响应

**状态码**: `200 OK`

```json
{
  "id": 1,
  "name": "我的守号",
  "red": [2, 22, 30, 33, 34],
  "blue": [8, 12],
  "score": 65.2,
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

| 字段 | 类型 | 说明 |
|------|------|------|
| `score` | float | 综合评分，基于频率、遗漏、和值等维度 |
| `prize_stats` | object | 各等奖在历史记录中的命中次数 |

### prize_stats 对象

| 字段 | 说明 |
|------|------|
| `1` | 一等奖（5+2）命中次数 |
| `2` | 二等奖（5+1）命中次数 |
| `3` | 三等奖（5+0）命中次数 |
| `4` | 四等奖（4+2）命中次数 |
| `5` | 五等奖（4+1）命中次数 |
| `6` | 六等奖（4+0）命中次数 |
| `7` | 七等奖（3+2）命中次数 |
| `8` | 八等奖（2+2 或 3+1）命中次数 |

---

## 结果查询

**GET** `/api/persistent/<id>/history?n=30`

查询该守号方案在最新 N 期开奖记录中的中奖情况。

### 查询参数

| 参数 | 类型 | 默认值 | 说明 |
|------|------|--------|------|
| `n` | integer | `30` | 查询最近多少期 |

### 响应

**状态码**: `200 OK`

```json
{
  "id": 1,
  "name": "我的守号",
  "n_checked": 30,
  "results": [
    {
      "issue": "26030",
      "date": "2026-04-22",
      "red": [5, 12, 18, 27, 33],
      "blue": [3, 9],
      "red_match": 2,
      "blue_match": 1,
      "prize_level": null
    },
    {
      "issue": "26025",
      "date": "2026-04-15",
      "red": [2, 22, 30, 33, 34],
      "blue": [8, 12],
      "red_match": 5,
      "blue_match": 2,
      "prize_level": 1
    }
  ],
  "summary": {
    "1": 1,
    "2": 0,
    "3": 0,
    "4": 0,
    "5": 2,
    "6": 0,
    "7": 0,
    "8": 5
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `n_checked` | integer | 实际查询的期数 |
| `results` | array | 每期开奖号码及匹配结果，按日期倒序 |
| `summary` | object | 中奖汇总统计 |

#### 匹配结果对象

| 字段 | 类型 | 说明 |
|------|------|------|
| `issue` | string | 期号 |
| `date` | string | 开奖日期 |
| `red` | array[int] | 开奖红球 |
| `blue` | array[int] | 开奖蓝球 |
| `red_match` | integer | 红球命中数（0-5） |
| `blue_match` | integer | 蓝球命中数（0-2） |
| `prize_level` | integer \| null | 奖项等级，未中奖为 null |

#### 奖项等级对照

| prize_level | 条件 | 说明 |
|-------------|------|------|
| 1 | 5红+2蓝 | 一等奖 |
| 2 | 5红+1蓝 | 二等奖 |
| 3 | 5红+0蓝 | 三等奖 |
| 4 | 4红+2蓝 | 四等奖 |
| 5 | 4红+1蓝 | 五等奖 |
| 6 | 4红+0蓝 | 六等奖 |
| 7 | 3红+2蓝 | 七等奖 |
| 8 | 2红+2蓝 或 3红+1蓝 | 八等奖 |

## 示例

```bash
# 新增守号方案
curl -X POST http://localhost:8888/api/persistent \
  -H 'Content-Type: application/json' \
  -d '{"name":"我的守号","red":[2,22,30,33,34],"blue":[8,12]}'

# 查看守号方案列表
curl http://localhost:8888/api/persistent

# 查询单个方案
curl http://localhost:8888/api/persistent/1

# 修改方案
curl -X PUT http://localhost:8888/api/persistent/1 \
  -H 'Content-Type: application/json' \
  -d '{"name":"新名称"}'

# 删除方案
curl -X DELETE http://localhost:8888/api/persistent/1

# 方案分析
curl http://localhost:8888/api/persistent/1/analysis

# 查询最近 50 期中奖情况
curl 'http://localhost:8888/api/persistent/1/history?n=50'
```
