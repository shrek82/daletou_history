# API 接口文档

## 概述

大乐透开奖信息查询与 AI 选号推荐 HTTP 服务。

- 服务基于 `tiny_http` 同步 HTTP 服务器
- 默认监听地址: `http://0.0.0.0:8888`
- 所有接口返回 JSON 格式，编码 UTF-8

## 快速开始

| 文档 | 说明 |
|------|------|
| [启动参数](startup.md) | `--server`, `--port`, `--max-records` 参数说明 |
| [健康检查](health.md) | `GET /health` |
| [开奖记录查询](draws.md) | `GET /api/draws` 分页查询 / 按期号查询 |
| [全部推荐](picks.md) | `GET /api/picks` 返回全部 AI 推荐方案 |
| [指定策略推荐](pick.md) | `GET /api/pick?strategy=<name>` 返回单个策略方案 |
| [守号方案](persistent.md) | 守号方案的增删改查、分析、结果查询 |

## 接口索引

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | [健康检查](health.md) |
| GET | `/api/draws` | [开奖记录查询](draws.md) |
| GET | `/api/picks` | [全部推荐](picks.md) |
| GET | `/api/pick?strategy=<name>` | [指定策略推荐](pick.md) |
| POST/GET/PUT/DELETE | `/api/persistent[/<id>]` | [守号方案](persistent.md) |

## 通用错误响应

所有错误接口统一返回 `400`、`404` 或 `500` 状态码，响应体格式：

```json
{
  "error": "错误描述信息"
}
```

## 请求头要求

无特殊请求头要求。响应头包含：

```
Content-Type: application/json; charset=utf-8
```
