use crate::config::InstrumentConfig;
use crate::models::DailyBar;
use crate::providers::{MarketDataProvider, ProviderError, ProviderErrorKind};
use crate::utils::{date, http};
use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDate;
use serde::Deserialize;
use url::Url;

pub struct EastmoneyProvider {
    client: reqwest::Client,
    base_url: String,
    fund_base_url: String,
}

impl EastmoneyProvider {
    pub fn new(base_url: impl Into<String>, fund_base_url: impl Into<String>) -> Self {
        Self {
            client: http::client(),
            base_url: base_url.into(),
            fund_base_url: fund_base_url.into(),
        }
    }
}

impl EastmoneyProvider {
    async fn fetch_fund_bars(
        &self,
        instrument: &InstrumentConfig,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyBar>> {
        let code = fund_code(instrument).ok_or_else(|| {
            ProviderError::new(
                self.name(),
                ProviderErrorKind::UnsupportedInstrument,
                format!("cannot derive Eastmoney fund code from {}", instrument.id),
            )
        })?;
        let mut all = Vec::new();
        let mut page = 1;
        let per = 200;

        loop {
            let mut url = Url::parse(&self.fund_base_url)?;
            url.query_pairs_mut()
                .append_pair("type", "lsjz")
                .append_pair("code", &code)
                .append_pair("page", &page.to_string())
                .append_pair("per", &per.to_string());

            let body = self
                .client
                .get(url)
                .header(reqwest::header::REFERER, "https://fund.eastmoney.com/")
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

            let page_bars = parse_eastmoney_fund_nav(&instrument.id, &body, "eastmoney_fund")?;
            let oldest = page_bars.last().map(|bar| bar.trade_date);
            all.extend(
                page_bars
                    .into_iter()
                    .filter(|bar| bar.trade_date >= start && bar.trade_date <= end),
            );

            if oldest.is_none_or(|date| date < start) || !fund_response_has_next_page(&body, page) {
                break;
            }
            page += 1;
        }

        if all.is_empty() {
            return Err(ProviderError::new(
                self.name(),
                ProviderErrorKind::NoData,
                format!("Eastmoney fund returned no NAV rows for {code}"),
            )
            .into());
        }
        all.sort_by_key(|bar| bar.trade_date);
        all.dedup_by_key(|bar| bar.trade_date);
        Ok(all)
    }
}

#[async_trait]
impl MarketDataProvider for EastmoneyProvider {
    fn name(&self) -> &'static str {
        "eastmoney"
    }

    async fn fetch_daily_bars(
        &self,
        instrument: &InstrumentConfig,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyBar>> {
        if is_fund_instrument(instrument) {
            return self.fetch_fund_bars(instrument, start, end).await;
        }

        let secid = resolve_secid(instrument, self.name())?;
        let mut url = Url::parse(&self.base_url)?;
        url.query_pairs_mut()
            .append_pair("secid", &secid)
            .append_pair("klt", "101")
            .append_pair("fqt", "0")
            .append_pair("beg", &date::yyyymmdd(start))
            .append_pair("end", &date::yyyymmdd(end))
            .append_pair("fields1", "f1,f2,f3,f4,f5,f6")
            .append_pair("fields2", "f51,f52,f53,f54,f55,f56,f57,f58,f59,f60,f61");

        let body = self
            .client
            .get(url)
            .header(reqwest::header::REFERER, "https://quote.eastmoney.com/")
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

        parse_eastmoney_daily(&instrument.id, &body, self.name())
    }
}

pub(crate) fn parse_eastmoney_daily(
    instrument_id: &str,
    body: &str,
    source: &'static str,
) -> Result<Vec<DailyBar>> {
    let response: EastmoneyResponse = serde_json::from_str(body).map_err(|err| {
        ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            format!("invalid Eastmoney JSON: {err}"),
        )
    })?;

    let data = response.data.ok_or_else(|| {
        ProviderError::new(
            source,
            ProviderErrorKind::NoData,
            "Eastmoney returned data=null",
        )
    })?;
    if data.klines.is_empty() {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::NoData,
            "Eastmoney returned empty klines",
        )
        .into());
    }

    let mut bars = Vec::new();
    for line in data.klines {
        bars.push(parse_kline(instrument_id, &line, source)?);
    }
    bars.sort_by_key(|bar| bar.trade_date);
    Ok(bars)
}

fn resolve_secid(instrument: &InstrumentConfig, source: &'static str) -> Result<String> {
    if let Some(secid) = instrument
        .eastmoney_secid
        .as_deref()
        .filter(|secid| !secid.trim().is_empty())
    {
        return Ok(secid.to_string());
    }

    let symbol = numeric_symbol(&instrument.id).ok_or_else(|| {
        ProviderError::new(
            source,
            ProviderErrorKind::UnsupportedInstrument,
            format!(
                "cannot derive Eastmoney secid from instrument id {}",
                instrument.id
            ),
        )
    })?;
    let market = instrument.market.to_ascii_uppercase();
    let prefix = if instrument.id.to_ascii_lowercase().starts_with("sh")
        || matches!(market.as_str(), "SH" | "SSE" | "CN-SH")
        || symbol.starts_with('6')
        || symbol.starts_with('9')
    {
        "1"
    } else if instrument.id.to_ascii_lowercase().starts_with("sz")
        || matches!(market.as_str(), "SZ" | "SZSE" | "CN-SZ")
        || symbol.starts_with('0')
        || symbol.starts_with('2')
        || symbol.starts_with('3')
    {
        "0"
    } else {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::UnsupportedInstrument,
            format!(
                "cannot infer Eastmoney market prefix for {} ({})",
                instrument.id, instrument.market
            ),
        )
        .into());
    };
    Ok(format!("{prefix}.{symbol}"))
}

fn numeric_symbol(id: &str) -> Option<String> {
    let digits: String = id.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == 6 {
        Some(digits)
    } else {
        None
    }
}

#[derive(Debug, Deserialize)]
struct EastmoneyResponse {
    data: Option<EastmoneyData>,
}

#[derive(Debug, Deserialize)]
struct EastmoneyData {
    #[serde(default)]
    klines: Vec<String>,
}

fn parse_kline(instrument_id: &str, line: &str, source: &'static str) -> Result<DailyBar> {
    let fields: Vec<&str> = line.split(',').map(str::trim).collect();
    if fields.len() != 11 {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            format!(
                "invalid Eastmoney kline field count {}, row: {line}",
                fields.len()
            ),
        )
        .into());
    }
    Ok(DailyBar {
        instrument_id: instrument_id.to_string(),
        trade_date: parse_date(source, fields[0])?,
        open: parse_f64(source, fields[1], "open")?,
        close: parse_f64(source, fields[2], "close")?,
        high: parse_f64(source, fields[3], "high")?,
        low: parse_f64(source, fields[4], "low")?,
        volume: parse_optional_f64(fields[5]),
        amount: parse_optional_f64(fields[6]),
        source: source.to_string(),
    })
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

fn parse_f64(source: &'static str, value: &str, field: &str) -> Result<f64> {
    value.parse::<f64>().map_err(|err| {
        ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            format!("invalid {field} value {value}: {err}"),
        )
        .into()
    })
}

fn parse_optional_f64(value: &str) -> Option<f64> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        None
    } else {
        trimmed.parse().ok()
    }
}

fn is_fund_instrument(instrument: &InstrumentConfig) -> bool {
    instrument.eastmoney_fund_code.is_some() || instrument.market.eq_ignore_ascii_case("FUND")
}

fn fund_code(instrument: &InstrumentConfig) -> Option<String> {
    instrument
        .eastmoney_fund_code
        .as_deref()
        .filter(|code| !code.trim().is_empty())
        .map(ToString::to_string)
        .or_else(|| numeric_symbol(&instrument.id))
}

pub(crate) fn parse_eastmoney_fund_nav(
    instrument_id: &str,
    body: &str,
    source: &'static str,
) -> Result<Vec<DailyBar>> {
    if !body.contains("var apidata") || !body.contains("<tbody>") {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            "Eastmoney fund response missing apidata table",
        )
        .into());
    }

    let mut bars = Vec::new();
    for row in body.split("<tr>").skip(1) {
        let cells = extract_td_texts(row);
        if cells.len() < 2 {
            continue;
        }
        let trade_date = parse_date(source, &cells[0])?;
        let nav = parse_f64(source, &cells[1], "unit_nav")?;
        bars.push(DailyBar {
            instrument_id: instrument_id.to_string(),
            trade_date,
            // Fund NAV history does not provide OHLC. We mirror unit NAV into all
            // OHLC fields so the shared signal/report pipeline can process it.
            open: nav,
            high: nav,
            low: nav,
            close: nav,
            volume: None,
            amount: None,
            source: source.to_string(),
        });
    }

    if bars.is_empty() {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::NoData,
            "Eastmoney fund table has no NAV rows",
        )
        .into());
    }
    bars.sort_by_key(|bar| bar.trade_date);
    Ok(bars)
}

fn extract_td_texts(row: &str) -> Vec<String> {
    let mut cells = Vec::new();
    for part in row.split("<td").skip(1) {
        if let Some(after_open) = part.split_once('>') {
            if let Some((raw, _)) = after_open.1.split_once("</td>") {
                cells.push(strip_tags(raw).trim().to_string());
            }
        }
    }
    cells
}

fn strip_tags(value: &str) -> String {
    let mut output = String::new();
    let mut in_tag = false;
    for ch in value.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn fund_response_has_next_page(body: &str, page: usize) -> bool {
    let Some(pages_start) = body.find("pages:") else {
        return false;
    };
    let pages_text = body[pages_start + "pages:".len()..]
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    pages_text.parse::<usize>().is_ok_and(|pages| page < pages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_normal_json() {
        let fixture = r#"{
            "data": {
                "klines": ["2024-01-02,10.00,10.50,10.80,9.90,12345,67890,1.2,0.3,0.03,0.5"]
            }
        }"#;
        let bars = parse_eastmoney_daily("sh000001", fixture, "eastmoney").unwrap();
        assert_eq!(bars.len(), 1);
        assert_eq!(bars[0].open, 10.0);
        assert_eq!(bars[0].close, 10.5);
        assert_eq!(bars[0].high, 10.8);
        assert_eq!(bars[0].low, 9.9);
        assert_eq!(bars[0].amount, Some(67890.0));
    }

    #[test]
    fn classifies_data_null() {
        let fixture = r#"{"data": null}"#;
        let err = parse_eastmoney_daily("sh000001", fixture, "eastmoney").unwrap_err();
        let provider_err = err.downcast_ref::<ProviderError>().unwrap();
        assert_eq!(provider_err.kind, ProviderErrorKind::NoData);
    }

    #[test]
    fn rejects_bad_field_count() {
        let fixture = r#"{"data":{"klines":["2024-01-02,10,11"]}}"#;
        let err = parse_eastmoney_daily("sh000001", fixture, "eastmoney").unwrap_err();
        let provider_err = err.downcast_ref::<ProviderError>().unwrap();
        assert_eq!(provider_err.kind, ProviderErrorKind::MalformedResponse);
    }

    #[test]
    fn derives_secid_from_config_id() {
        let instrument = InstrumentConfig {
            id: "sh600000".to_string(),
            name: "浦发银行".to_string(),
            kind: crate::config::InstrumentKind::Stock,
            market: "SH".to_string(),
            currency: "CNY".to_string(),
            timezone: "Asia/Shanghai".to_string(),
            stooq_symbol: None,
            alpha_vantage_symbol: None,
            eastmoney_secid: None,
            eastmoney_fund_code: None,
            provider: "eastmoney".to_string(),
        };
        assert_eq!(resolve_secid(&instrument, "eastmoney").unwrap(), "1.600000");
    }

    #[test]
    fn parses_fund_nav_response() {
        let fixture = r#"var apidata={ content:"<table><tbody><tr><td>2026-05-20</td><td class='tor bold'>1.5739</td><td>1.5739</td></tr><tr><td>2026-05-19</td><td class='tor bold'>1.5539</td><td>1.5539</td></tr></tbody></table>",records:2,pages:1,curpage:1};"#;
        let bars = parse_eastmoney_fund_nav("fund501203", fixture, "eastmoney_fund").unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(
            bars[0].trade_date,
            NaiveDate::from_ymd_opt(2026, 5, 19).unwrap()
        );
        assert_eq!(bars[1].close, 1.5739);
        assert_eq!(bars[1].volume, None);
    }

    #[test]
    fn detects_next_fund_page() {
        assert!(fund_response_has_next_page(
            "var apidata={pages:3,curpage:1};",
            1
        ));
        assert!(!fund_response_has_next_page(
            "var apidata={pages:3,curpage:3};",
            3
        ));
    }
}
