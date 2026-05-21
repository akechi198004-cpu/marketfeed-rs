use crate::config::{InstrumentConfig, InstrumentKind};
use crate::models::DailyBar;
use crate::providers::{MarketDataProvider, ProviderError, ProviderErrorKind};
use crate::utils::{csv, http};
use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDate;
use serde_json::Value;
use url::Url;

pub struct AlphaVantageProvider {
    client: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
}

impl AlphaVantageProvider {
    pub fn new(api_key: Option<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: http::client(),
            api_key,
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl MarketDataProvider for AlphaVantageProvider {
    fn name(&self) -> &'static str {
        "alpha_vantage"
    }

    async fn fetch_daily_bars(
        &self,
        instrument: &InstrumentConfig,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyBar>> {
        let api_key = self.api_key.as_deref().ok_or_else(|| {
            ProviderError::new(
                self.name(),
                ProviderErrorKind::Auth,
                "missing Alpha Vantage API key",
            )
        })?;
        let symbol = instrument.alpha_vantage_symbol.as_deref().ok_or_else(|| {
            ProviderError::new(
                self.name(),
                ProviderErrorKind::UnsupportedInstrument,
                format!("missing alpha_vantage_symbol for {}", instrument.id),
            )
        })?;

        let mut url = Url::parse(&self.base_url)?;
        {
            let mut query = url.query_pairs_mut();
            match instrument.kind {
                InstrumentKind::Commodity => {
                    query
                        .append_pair("function", "GOLD_SILVER_HISTORY")
                        .append_pair("symbol", symbol)
                        .append_pair("interval", "daily");
                }
                _ => {
                    query
                        .append_pair("function", "TIME_SERIES_DAILY")
                        .append_pair("symbol", symbol);
                }
            }
            query.append_pair("apikey", api_key);
        }

        let body = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|err| {
                ProviderError::new(self.name(), ProviderErrorKind::Http, err.to_string())
            })?
            .error_for_status()
            .map_err(|err| {
                ProviderError::new(self.name(), ProviderErrorKind::Http, err.to_string())
            })?
            .text()
            .await
            .map_err(|err| {
                ProviderError::new(self.name(), ProviderErrorKind::Http, err.to_string())
            })?;

        let mut bars = parse_alpha_vantage_daily(&instrument.id, &body, self.name())?;
        bars.retain(|bar| bar.trade_date >= start && bar.trade_date <= end);
        if bars.is_empty() {
            return Err(ProviderError::new(
                self.name(),
                ProviderErrorKind::NoData,
                "Alpha Vantage returned no rows in requested date range",
            )
            .into());
        }
        Ok(bars)
    }
}

pub(crate) fn parse_alpha_vantage_daily(
    instrument_id: &str,
    body: &str,
    source: &'static str,
) -> Result<Vec<DailyBar>> {
    let trimmed = body.trim_start();
    if trimmed.starts_with('{') {
        parse_alpha_vantage_json(instrument_id, body, source)
    } else {
        csv::parse_alpha_vantage_csv_daily(instrument_id, body, source)
    }
}

fn parse_alpha_vantage_json(
    instrument_id: &str,
    body: &str,
    source: &'static str,
) -> Result<Vec<DailyBar>> {
    let value: Value = serde_json::from_str(body).map_err(|err| {
        ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            format!("invalid Alpha Vantage JSON: {err}"),
        )
    })?;

    if let Some((kind, message)) = alpha_provider_message(&value) {
        return Err(ProviderError::new(source, kind, message).into());
    }

    if let Some(series) = value.get("Time Series (Daily)").and_then(Value::as_object) {
        let mut bars = Vec::new();
        for (date, row) in series {
            let trade_date = parse_date(source, date)?;
            bars.push(DailyBar {
                instrument_id: instrument_id.to_string(),
                trade_date,
                open: read_number(source, row, &["1. open", "open"])?,
                high: read_number(source, row, &["2. high", "high"])?,
                low: read_number(source, row, &["3. low", "low"])?,
                close: read_number(source, row, &["4. close", "close"])?,
                volume: read_optional_number(row, &["5. volume", "volume"]),
                amount: None,
                source: source.to_string(),
            });
        }
        return finish_bars(source, bars);
    }

    let rows = value
        .get("data")
        .or_else(|| value.get("Data"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ProviderError::new(
                source,
                ProviderErrorKind::MalformedResponse,
                "Alpha Vantage JSON missing Time Series (Daily) or data array",
            )
        })?;

    let mut bars = Vec::new();
    for row in rows {
        let date = read_string(row, &["date", "timestamp", "time"])?;
        let trade_date = parse_date(source, date)?;
        let close = read_number(source, row, &["close", "4. close", "value", "price"])?;
        // GOLD_SILVER_HISTORY responses may contain only one daily value. Until the
        // upstream payload offers full OHLC, we keep all four OHLC fields equal to close.
        let open = read_optional_number(row, &["open", "1. open"]).unwrap_or(close);
        let high = read_optional_number(row, &["high", "2. high"]).unwrap_or(close);
        let low = read_optional_number(row, &["low", "3. low"]).unwrap_or(close);
        bars.push(DailyBar {
            instrument_id: instrument_id.to_string(),
            trade_date,
            open,
            high,
            low,
            close,
            volume: read_optional_number(row, &["volume", "5. volume"]),
            amount: None,
            source: source.to_string(),
        });
    }
    finish_bars(source, bars)
}

fn alpha_provider_message(value: &Value) -> Option<(ProviderErrorKind, String)> {
    for key in ["Note", "Information", "Error Message"] {
        if let Some(message) = value.get(key).and_then(Value::as_str) {
            let kind = match key {
                "Note" | "Information" => ProviderErrorKind::RateLimited,
                "Error Message" => ProviderErrorKind::ProviderMessage,
                _ => ProviderErrorKind::ProviderMessage,
            };
            return Some((kind, message.to_string()));
        }
    }
    None
}

fn finish_bars(source: &'static str, mut bars: Vec<DailyBar>) -> Result<Vec<DailyBar>> {
    if bars.is_empty() {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::NoData,
            "Alpha Vantage response contains no rows",
        )
        .into());
    }
    bars.sort_by_key(|bar| bar.trade_date);
    Ok(bars)
}

fn parse_date(source: &'static str, value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|err| {
        ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            format!("invalid date {value}: {err}"),
        )
        .into()
    })
}

fn read_string<'a>(row: &'a Value, keys: &[&str]) -> Result<&'a str> {
    keys.iter()
        .find_map(|key| row.get(*key).and_then(Value::as_str))
        .ok_or_else(|| {
            ProviderError::new(
                "alpha_vantage",
                ProviderErrorKind::MalformedResponse,
                format!("missing string field, tried {keys:?}"),
            )
            .into()
        })
}

fn read_number(source: &'static str, row: &Value, keys: &[&str]) -> Result<f64> {
    read_optional_number(row, keys).ok_or_else(|| {
        ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            format!("missing numeric field, tried {keys:?}"),
        )
        .into()
    })
}

fn read_optional_number(row: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        row.get(*key).and_then(|value| match value {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.parse().ok(),
            _ => None,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gold_history_value_only() {
        let fixture = r#"{
            "data": [
                {"date":"2024-01-02","value":"2060.50"},
                {"date":"2024-01-03","value":"2075.25"}
            ]
        }"#;
        let bars = parse_alpha_vantage_daily("xauusd", fixture, "alpha_vantage").unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(bars[0].open, 2060.50);
        assert_eq!(bars[0].high, 2060.50);
        assert_eq!(bars[0].low, 2060.50);
        assert_eq!(bars[0].close, 2060.50);
    }

    #[test]
    fn parses_stock_daily_json() {
        let fixture = r#"{
            "Time Series (Daily)": {
                "2024-01-03": {"1. open":"10","2. high":"12","3. low":"9","4. close":"11","5. volume":"1000"}
            }
        }"#;
        let bars = parse_alpha_vantage_daily("ibm", fixture, "alpha_vantage").unwrap();
        assert_eq!(bars[0].close, 11.0);
        assert_eq!(bars[0].volume, Some(1000.0));
    }

    #[test]
    fn classifies_rate_limit_message() {
        let fixture = r#"{"Note":"Thank you for using Alpha Vantage!"}"#;
        let err = parse_alpha_vantage_daily("ibm", fixture, "alpha_vantage").unwrap_err();
        let provider_err = err.downcast_ref::<ProviderError>().unwrap();
        assert_eq!(provider_err.kind, ProviderErrorKind::RateLimited);
    }

    #[test]
    fn classifies_error_message() {
        let fixture = r#"{"Error Message":"Invalid API call."}"#;
        let err = parse_alpha_vantage_daily("ibm", fixture, "alpha_vantage").unwrap_err();
        let provider_err = err.downcast_ref::<ProviderError>().unwrap();
        assert_eq!(provider_err.kind, ProviderErrorKind::ProviderMessage);
    }
}
