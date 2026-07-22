# Provider 配置说明

## 全局开关

`config.toml` 的 `[providers]` 段：

```toml
[providers]
stooq_enabled = true           # 默认 true
alpha_vantage_enabled = true
eastmoney_enabled = true
```

设为 `false` 时，对应 provider 被请求会返回 `UnsupportedInstrument` 错误，不会静默跳过。

## 各 Provider 连接配置

### Stooq

```toml
[providers.stooq]
api_key = ""                     # 可选，非空则优先于环境变量
api_key_env = "STOOQ_API_KEY"
base_url = "https://stooq.com/q/d/l/"
```

### Alpha Vantage

```toml
[providers.alpha_vantage]
api_key = ""
api_key_env = "ALPHA_VANTAGE_API_KEY"
base_url = "https://www.alphavantage.co/query"
```

### Eastmoney

```toml
[providers.eastmoney]
base_url = "https://push2his.eastmoney.com/api/qt/stock/kline/get"
fund_base_url = "https://fund.eastmoney.com/pingzhongdata"
```

## 更新行为配置

```toml
[update]
default_lookback_days = 7       # update 默认回溯天数
max_bootstrap_days = 5000       # bootstrap 最大历史深度
retry_count = 2                 # provider 请求重试次数
retry_delay_ms = 1000           # 重试间隔（毫秒）
```

## 标的（Instrument）字段

每个 `[[instruments]]` 条目：

| 字段 | 必填 | 说明 |
|------|------|------|
| `id` | 是 | 唯一标识，如 `sh000001`、`fund501203` |
| `name` | 是 | 显示名称 |
| `kind` | 是 | `index` / `stock` / `commodity` / `etf` |
| `market` | 是 | 市场代码，如 `SH`、`FUND`、`GLOBAL` |
| `currency` | 否 | 默认 `USD` |
| `timezone` | 否 | 默认 `UTC` |
| `provider` | 是 | 主数据源：`stooq` / `alpha_vantage` / `eastmoney` |
| `stooq_symbol` | 条件 | `provider=stooq` 时必填；其他 provider 可选，供 bootstrap |
| `alpha_vantage_symbol` | 条件 | `provider=alpha_vantage` 时必填；别名 `alpha_symbol` |
| `eastmoney_secid` | 否 | 如 `1.000001`；未填则从 id 推断 |
| `eastmoney_fund_code` | 否 | 基金代码；或设 `market=FUND` 自动走基金接口 |

### 校验规则（`Config::validate`）

- `provider = "stooq"` → 必须配置 `stooq_symbol`
- `provider = "alpha_vantage"` → 必须配置 `alpha_vantage_symbol`
- `provider = "eastmoney"` → 无额外必填（可从 id 推断 secid）
- 其他 provider 名称 → 启动时报错

## 符号映射速查

| 标的类型 | provider | 推荐配置字段 | 自动推断 |
|----------|----------|--------------|----------|
| A 股指数 | eastmoney | `eastmoney_secid = "1.000001"` | sh000001 → 1.000001 |
| A 股指数 | stooq | `stooq_symbol = "^shc"` | sh000001 → ^shc |
| A 股股票 | eastmoney | — | sh600000 → 1.600000 |
| 美股 | stooq | — | id → `{id}.us` |
| 黄金 | alpha_vantage | `alpha_symbol = "GOLD"` | — |
| 黄金 | stooq | `stooq_symbol = "xauusd"` | id=gold → xauusd |
| 基金 | eastmoney | `eastmoney_fund_code` 或 `market=FUND` | 6 位 id → fund code |

## 环境变量

| 变量 | 用途 |
|------|------|
| `STOOQ_API_KEY` | Stooq API Key（config 未填时） |
| `ALPHA_VANTAGE_API_KEY` | Alpha Vantage API Key（config 未填时） |
| `RUST_LOG` | 日志级别，默认 `marketfeed=info` |
