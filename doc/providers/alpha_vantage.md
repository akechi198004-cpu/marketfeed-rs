# Alpha Vantage Provider

**标识符：** `alpha_vantage`  
**源码：** `src/providers/alpha_vantage.rs`  
**CSV 解析：** `src/utils/csv.rs::parse_alpha_vantage_csv_daily`

## 概述

Alpha Vantage 提供 REST JSON（及部分 CSV）日线数据，适用于美股与贵金属等。 **必须配置 API Key**。

免费 tier 有严格速率限制（25 次/天等），响应中的 `Note` / `Information` 字段会被识别为 `RateLimited`。

## API

| 项 | 值 |
|----|-----|
| 默认 URL | `https://www.alphavantage.co/query` |
| 方法 | GET |
| 响应 | JSON 或 CSV |
| API Key | 必填（query 参数 `apikey`） |

### 请求参数

按 `instrument.kind` 分支：

| kind | function | 额外参数 |
|------|----------|----------|
| `commodity` | `GOLD_SILVER_HISTORY` | `symbol`, `interval=daily` |
| 其他 | `TIME_SERIES_DAILY` | `symbol` |

公共参数：`apikey`

`symbol` 来自 `instrument.alpha_vantage_symbol`（配置别名 `alpha_symbol`），缺失则报 `UnsupportedInstrument`。

## 响应格式

入口函数 `parse_alpha_vantage_daily` 根据 body 首字符分流：

- 以 `{` 开头 → JSON 解析
- 否则 → CSV 解析

### JSON — 股票日线

```json
{
  "Time Series (Daily)": {
    "2024-01-03": {
      "1. open": "10",
      "2. high": "12",
      "3. low": "9",
      "4. close": "11",
      "5. volume": "1000"
    }
  }
}
```

字段名兼容带序号前缀（`1. open`）与无前缀（`open`）两种形式。

### JSON — 贵金属历史

```json
{
  "data": [
    {"date": "2024-01-02", "value": "2060.50"}
  ]
}
```

也支持 `Data`（大写）数组。仅有 `value` / `close` 时，OHLC 四字段均设为该值。

### JSON — 上游消息

| JSON 键 | 映射错误类型 |
|---------|--------------|
| `Note` | `RateLimited` |
| `Information` | `RateLimited` |
| `Error Message` | `ProviderMessage` |

### CSV

期望 header（大小写不敏感）：

```
timestamp,open,high,low,close,volume
```

或

```
date,open,high,low,close,volume
```

## 日期过滤

Alpha Vantage API 通常返回完整历史，客户端在 fetch 后执行：

```rust
bars.retain(|bar| bar.trade_date >= start && bar.trade_date <= end);
```

过滤后为空 → `NoData`。

## 配置

```toml
[providers.alpha_vantage]
api_key = ""                              # 或直接填写
api_key_env = "ALPHA_VANTAGE_API_KEY"
base_url = "https://www.alphavantage.co/query"

[[instruments]]
id = "gold"
kind = "commodity"
market = "GLOBAL"
provider = "alpha_vantage"
alpha_symbol = "GOLD"                     # 等价于 alpha_vantage_symbol
stooq_symbol = "xauusd"                   # 供 bootstrap Stooq 使用
```

API Key 读取顺序：`providers.alpha_vantage.api_key` → 环境变量 `api_key_env`。

## 错误场景

| 场景 | ProviderErrorKind |
|------|-------------------|
| 缺少 API Key | `Auth` |
| 缺少 alpha_vantage_symbol | `UnsupportedInstrument` |
| HTTP 失败 | `Http` |
| JSON/CSV 格式错误 | `MalformedResponse` |
| Note / Information | `RateLimited` |
| Error Message | `ProviderMessage` |
| 日期范围内无数据 | `NoData` |
