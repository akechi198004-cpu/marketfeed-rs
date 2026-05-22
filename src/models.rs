use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyBar {
    pub instrument_id: String,
    pub trade_date: NaiveDate,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub amount: Option<f64>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub instrument_id: String,
    pub trade_date: NaiveDate,
    pub action: SignalAction,
    pub score: i32,
    pub reasons: Vec<String>,
    pub source: String,
    pub close: f64,
    pub ma20: Option<f64>,
    pub ma60: Option<f64>,
    pub ma120: Option<f64>,
    pub deviation_ma60_pct: Option<f64>,
    pub deviation_ma120_pct: Option<f64>,
    pub change_60d_pct: Option<f64>,
    pub drawdown_120d_pct: Option<f64>,
    pub generated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SignalAction {
    Buy,
    Sell,
    Hold,
}

impl SignalAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
            Self::Hold => "HOLD",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReportInstrument {
    pub instrument_id: String,
    pub name: String,
    pub latest_close: Option<f64>,
    pub action: String,
    pub score: i32,
    pub reason: String,
    pub source: Option<String>,
    pub recent_trade_date: Option<NaiveDate>,
    /// 与当日信号相同、按交易日连续的天数（来自库内历史信号）
    pub action_streak_days: u32,
    /// 例如「连续3日卖出」
    pub action_streak_label: String,
    /// 连续 ≥2 日同向时的辅助提示（可为空）
    pub action_streak_hint: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderErrorRecord {
    pub provider: String,
    pub instrument_id: String,
    pub kind: String,
    pub raw_message: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct DataFreshness {
    pub instrument_id: String,
    pub latest_bar_date: Option<NaiveDate>,
    pub days_behind: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct MarketReport {
    pub report_date: NaiveDate,
    pub data_updated_at: chrono::DateTime<Utc>,
    pub timezone: String,
    pub instruments: Vec<ReportInstrument>,
    pub provider_errors: Vec<ProviderErrorRecord>,
    pub freshness: Vec<DataFreshness>,
    pub disclaimer: String,
}
