# Stooq Provider

**标识符：** `stooq`  
**源码：** `src/providers/stooq.rs`  
**解析：** `src/utils/csv.rs::parse_stooq_daily`

## 概述

Stooq 提供 CSV 格式的日线数据，覆盖全球多个市场。在本项目中主要用于 **Bootstrap 历史初始化**（优先数据源），也可作为标的的主 provider。

## API

| 项 | 值 |
|----|-----|
| 默认 URL | `https://stooq.com/q/d/l/` |
| 方法 | GET |
| 响应 | CSV 文本 |
| API Key | 可选（query 参数 `apikey`） |

### 请求参数

| 参数 | 说明 | 示例 |
|------|------|------|
| `s` | Stooq 符号 | `^shc`、`600000.cn`、`xauusd` |
| `i` | 间隔 | 固定 `d`（日线） |
| `d1` | 起始日期 | `20240101`（YYYYMMDD） |
| `d2` | 结束日期 | `20240521` |
| `apikey` | 可选密钥 | 来自配置或 `STOOQ_API_KEY` 环境变量 |

## 符号解析

`resolve_stooq_symbol(instrument)` 按以下优先级确定 `s` 参数：

1. **显式配置** — `instrument.stooq_symbol` 非空时直接使用
2. **商品默认** — `kind = commodity` 且 `id = gold` → `xauusd`
3. **市场推断** — `infer_exchange_symbol(instrument)`：

| 条件 | 符号 |
|------|------|
| 美股市场（US / NASDAQ / NYSE / AMEX） | `{id}.us` |
| 上证指数 sh000001 | `^shc` |
| A 股 6 位代码（sh/sz 前缀或 CN 市场） | `{6位数字}.cn` |
| 其他 | `None`（不支持） |

## CSV 响应格式

期望 header：

```
Date,Open,High,Low,Close,Volume
```

每行示例：

```
2024-01-02,10.00,10.80,9.90,10.50,12345
```

解析规则：

- 日期格式 `%Y-%m-%d`
- `Volume` 为空或 `-` 时映射为 `None`
- 响应以 HTML 开头时判定为 `MalformedResponse`（常见于错误页）
- header 不符合预期且无逗号时归类为 `ProviderMessage`

## 配置

```toml
[providers.stooq]
api_key = ""                          # 或直接填写
api_key_env = "STOOQ_API_KEY"
base_url = "https://stooq.com/q/d/l/"

[[instruments]]
id = "sh000001"
provider = "eastmoney"                # 日常更新用 eastmoney
stooq_symbol = "^shc"               # bootstrap 时 Stooq 可用
```

## Bootstrap 优先策略

`bootstrap::bootstrap_provider` 逻辑：

```
if stooq_enabled && resolve_stooq_symbol(instrument).is_some()
    → 使用 "stooq"
else
    → 使用 instrument.history_provider()
```

主 provider 失败且与 bootstrap provider 不同时，自动 fallback 到 `instrument.history_provider()`。

## 错误场景

| 场景 | ProviderErrorKind |
|------|-------------------|
| 缺少/无法解析 stooq_symbol | `UnsupportedInstrument` |
| HTTP 失败 | `Http` |
| 返回 HTML | `MalformedResponse` |
| 空 CSV / 无数据行 | `NoData` |
| header 异常 | `MalformedResponse` 或 `ProviderMessage` |
