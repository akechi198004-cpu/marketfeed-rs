# 数据代理库（Providers）

marketfeed-rs 通过 **Provider** 抽象从外部数据源拉取日线 OHLCV 数据，统一转换为 `DailyBar` 后写入 SQLite。

## 支持的 Provider

| 名称 | 标识符 | 典型用途 | 需要 API Key |
|------|--------|----------|--------------|
| [Stooq](./stooq.md) | `stooq` | 全球股票/指数/商品，Bootstrap 优先 | 可选 |
| [Eastmoney](./eastmoney.md) | `eastmoney` | A 股 K 线、基金净值 | 否 |
| [Alpha Vantage](./alpha_vantage.md) | `alpha_vantage` | 美股、贵金属等 | 是 |

## 核心概念

### 统一输出：`DailyBar`

所有 Provider 返回相同结构：

```rust
pub struct DailyBar {
    pub instrument_id: String,
    pub trade_date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub amount: Option<f64>,
    pub source: String,   // 实际使用的 provider 名称
}
```

对于仅有单一价格的数据（基金净值、贵金属历史），会将该价格复制到 OHLC 四个字段，以便下游信号与报告流水线统一处理。

### 统一接口：`MarketDataProvider`

```rust
#[async_trait]
pub trait MarketDataProvider {
    fn name(&self) -> &'static str;
    async fn fetch_daily_bars(
        &self,
        instrument: &InstrumentConfig,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyBar>>;
}
```

每个 Provider 实现该 trait，由 `updater::fetch_once` 按名称实例化并调用。

### 标的配置：`InstrumentConfig`

每个标的在 `config.toml` 的 `[[instruments]]` 中声明 **主 provider**（`provider` 字段），以及各数据源所需的符号映射字段。详见 [configuration.md](./configuration.md)。

## 调用时机

| 场景 | 使用的 Provider | 说明 |
|------|-----------------|------|
| 增量更新 (`update`) | `instrument.provider` | 按配置的主 provider 拉取 |
| 历史初始化 (`bootstrap`) | Stooq 优先，失败则 fallback 到 `instrument.provider` | 见 [integration.md](./integration.md) |

## 错误处理

Provider 失败时抛出 `ProviderError`（带分类 `ProviderErrorKind`），由 `provider_errors` 服务写入 `provider_errors` 表，并在日报中展示。详见 [architecture.md](./architecture.md#错误模型)。

## 快速配置示例

```toml
[providers]
stooq_enabled = true
alpha_vantage_enabled = true
eastmoney_enabled = true

[[instruments]]
id = "sh000001"
name = "上证指数"
kind = "index"
market = "SH"
provider = "eastmoney"
eastmoney_secid = "1.000001"
stooq_symbol = "^shc"    # 供 bootstrap 优先使用 Stooq 时使用

[[instruments]]
id = "gold"
name = "Gold Spot"
kind = "commodity"
market = "GLOBAL"
provider = "alpha_vantage"
alpha_symbol = "GOLD"
stooq_symbol = "xauusd"
```
