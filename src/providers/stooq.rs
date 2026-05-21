use crate::config::{InstrumentConfig, InstrumentKind};
use crate::models::DailyBar;
use crate::providers::{MarketDataProvider, ProviderError, ProviderErrorKind};
use crate::utils::{csv, date, http};
use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDate;
use url::Url;

pub struct StooqProvider {
    client: reqwest::Client,
    api_key: Option<String>,
    base_url: String,
}

impl StooqProvider {
    pub fn new(api_key: Option<String>, base_url: impl Into<String>) -> Self {
        Self {
            client: http::client(),
            api_key,
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl MarketDataProvider for StooqProvider {
    fn name(&self) -> &'static str {
        "stooq"
    }

    async fn fetch_daily_bars(
        &self,
        instrument: &InstrumentConfig,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyBar>> {
        let symbol = resolve_stooq_symbol(instrument).ok_or_else(|| {
            ProviderError::new(
                self.name(),
                ProviderErrorKind::UnsupportedInstrument,
                format!("missing or unresolvable stooq_symbol for {}", instrument.id),
            )
        })?;
        let mut url = Url::parse(&self.base_url)?;
        url.query_pairs_mut()
            .append_pair("s", &symbol)
            .append_pair("i", "d")
            .append_pair("d1", &date::yyyymmdd(start))
            .append_pair("d2", &date::yyyymmdd(end));
        if let Some(api_key) = &self.api_key {
            url.query_pairs_mut().append_pair("apikey", api_key);
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

        csv::parse_stooq_daily(&instrument.id, &body, self.name())
    }
}

pub(crate) fn resolve_stooq_symbol(instrument: &InstrumentConfig) -> Option<String> {
    if let Some(symbol) = instrument
        .stooq_symbol
        .as_deref()
        .filter(|symbol| !symbol.trim().is_empty())
    {
        return Some(symbol.to_string());
    }

    match instrument.kind {
        InstrumentKind::Commodity if instrument.id.eq_ignore_ascii_case("gold") => {
            Some("xauusd".to_string())
        }
        InstrumentKind::Commodity => None,
        _ => infer_exchange_symbol(instrument),
    }
}

fn infer_exchange_symbol(instrument: &InstrumentConfig) -> Option<String> {
    let id = instrument.id.to_ascii_lowercase();
    let market = instrument.market.to_ascii_uppercase();
    let digits: String = id.chars().filter(char::is_ascii_digit).collect();

    if matches!(market.as_str(), "US" | "NASDAQ" | "NYSE" | "AMEX") {
        return Some(format!("{}.us", id));
    }

    if digits == "000001"
        && (id.starts_with("sh") || matches!(market.as_str(), "SH" | "SSE" | "CN-SH"))
    {
        return Some("^shc".to_string());
    }

    if digits.len() == 6
        && (id.starts_with("sh")
            || id.starts_with("sz")
            || matches!(
                market.as_str(),
                "SH" | "SZ" | "CN" | "SSE" | "SZSE" | "CN-SH" | "CN-SZ"
            ))
    {
        return Some(format!("{digits}.cn"));
    }

    None
}

#[cfg(test)]
mod stooq_symbol_tests {
    use super::*;

    fn instrument(id: &str, market: &str, kind: InstrumentKind) -> InstrumentConfig {
        InstrumentConfig {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            market: market.to_string(),
            currency: "USD".to_string(),
            timezone: "UTC".to_string(),
            provider: "stooq".to_string(),
            alpha_vantage_symbol: None,
            stooq_symbol: None,
            eastmoney_secid: None,
            eastmoney_fund_code: None,
        }
    }

    #[test]
    fn infers_china_index_symbol() {
        assert_eq!(
            resolve_stooq_symbol(&instrument("sh000001", "SH", InstrumentKind::Index)),
            Some("^shc".to_string())
        );
    }

    #[test]
    fn infers_gold_symbol() {
        assert_eq!(
            resolve_stooq_symbol(&instrument("gold", "GLOBAL", InstrumentKind::Commodity)),
            Some("xauusd".to_string())
        );
    }
}
