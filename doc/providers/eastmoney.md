# Eastmoney Provider

**标识符：** `eastmoney`  
**源码：** `src/providers/eastmoney.rs`

## 概述

东方财富数据源，支持两类标的：

1. **股票 / 指数 K 线** — JSON API
2. **基金净值** — `pingzhongdata/{code}.js`（嵌入 `Data_netWorthTrend`）

无需 API Key，但基金接口需设置 `Referer: https://fund.eastmoney.com/`。

## 股票 K 线

### API

| 项 | 值 |
|----|-----|
| 默认 URL | `https://push2his.eastmoney.com/api/qt/stock/kline/get` |
| 方法 | GET |
| 响应 | JSON |

### 请求参数

| 参数 | 说明 | 值 |
|------|------|-----|
| `secid` | 市场.代码 | 如 `1.000001`（沪）、`0.000001`（深） |
| `klt` | K 线类型 | `101`（日 K） |
| `fqt` | 复权 | `0`（不复权） |
| `beg` / `end` | 日期范围 | YYYYMMDD |
| `fields1` | 元数据字段 | `f1,f2,f3,f4,f5,f6` |
| `fields2` | K 线字段 | `f51,f52,...,f61` |

### secid 解析

`resolve_secid(instrument)` 优先级：

1. **显式配置** — `instrument.eastmoney_secid`
2. **从 id 推断** — 提取 6 位数字，按市场前缀拼接：

| 条件 | 前缀 | 示例 |
|------|------|------|
| sh 前缀 / SH 市场 / 6/9 开头 | `1` | `sh600000` → `1.600000` |
| sz 前缀 / SZ 市场 / 0/2/3 开头 | `0` | `sz000001` → `0.000001` |
| 无法判断 | 报错 `UnsupportedInstrument` | |

### JSON 响应结构

```json
{
  "data": {
    "klines": [
      "2024-01-02,10.00,10.50,10.80,9.90,12345,67890,1.2,0.3,0.03,0.5"
    ]
  }
}
```

每行 kline 为逗号分隔的 11 个字段：

| 索引 | 字段 | 映射到 DailyBar |
|------|------|-----------------|
| 0 | 日期 | `trade_date` |
| 1 | 开盘 | `open` |
| 2 | 收盘 | `close` |
| 3 | 最高 | `high` |
| 4 | 最低 | `low` |
| 5 | 成交量 | `volume` |
| 6 | 成交额 | `amount` |
| 7–10 | 其他 | 忽略 |

`data` 为 `null` 或 `klines` 为空 → `NoData`。

---

## 基金净值

### 触发条件

满足任一条件即走基金分支：

- `instrument.eastmoney_fund_code` 已配置
- `instrument.market == "FUND"`（忽略大小写）

### API

| 项 | 值 |
|----|-----|
| 默认 URL | `https://fund.eastmoney.com/pingzhongdata` |
| 实际请求 | `{fund_base_url}/{code}.js` |
| 方法 | GET |
| 响应 | JS，含 `Data_netWorthTrend = [{x,y,...}, ...]` |

> 旧接口 `F10DataApi.aspx?type=lsjz` 已空返回，不可再用。

### 拉取逻辑

`fetch_fund_bars` 一次下载全量历史净值，再按 `[start, end]` 过滤、排序、去重。

### JS 解析

从响应中提取 `Data_netWorthTrend` 数组：

| 字段 | 含义 | 映射 |
|------|------|------|
| `x` | 毫秒时间戳（**北京时间**当日 00:00） | `trade_date` |
| `y` | 单位净值 | open/high/low/close（基金无 OHLC，四处同值） |

### 基金代码解析

`fund_code(instrument)`：

1. `eastmoney_fund_code` 非空 → 直接使用
2. 否则从 `id` 提取 6 位数字

## 配置示例

```toml
[providers.eastmoney]
base_url = "https://push2his.eastmoney.com/api/qt/stock/kline/get"
fund_base_url = "https://fund.eastmoney.com/pingzhongdata"

# 指数
[[instruments]]
id = "sh000001"
provider = "eastmoney"
eastmoney_secid = "1.000001"

# 基金
[[instruments]]
id = "fund501203"
kind = "etf"
market = "FUND"
provider = "eastmoney"
eastmoney_fund_code = "501203"
```

## 错误场景

| 场景 | ProviderErrorKind |
|------|-------------------|
| 无法推导 secid / fund code | `UnsupportedInstrument` |
| HTTP 失败 | `Http` |
| JSON 解析失败 / kline 字段数不对 | `MalformedResponse` |
| data=null / 空 klines / 空 NAV | `NoData` |
| 基金响应缺少 `Data_netWorthTrend` | `MalformedResponse` |
