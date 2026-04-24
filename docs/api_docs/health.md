# GET /health

健康检查接口，用于确认服务是否正常运行。

## 请求

```
GET /health
```

无请求参数。

## 响应

**状态码**: `200 OK`

```json
{
  "status": "ok",
  "records": 365
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | string | 服务状态，固定值 `"ok"` |
| `records` | integer | 数据库当前存储的开奖记录条数 |

## 示例

```bash
curl http://localhost:8888/health
```

响应：
```json
{
  "status": "ok",
  "records": 365
}
```
