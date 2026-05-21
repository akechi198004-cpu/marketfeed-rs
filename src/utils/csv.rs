use crate::models::DailyBar;
use crate::providers::{ProviderError, ProviderErrorKind};
use anyhow::Result;
use chrono::NaiveDate;

pub fn parse_stooq_daily(
    instrument_id: &str,
    body: &str,
    source: &'static str,
) -> Result<Vec<DailyBar>> {
    if looks_like_html(body) {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            "Stooq returned HTML instead of CSV",
        )
        .into());
    }

    let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty());
    let header = lines.next().ok_or_else(|| {
        ProviderError::new(source, ProviderErrorKind::NoData, "empty Stooq response")
    })?;

    if header != "Date,Open,High,Low,Close,Volume" {
        let kind = if header.contains(',') {
            ProviderErrorKind::MalformedResponse
        } else {
            ProviderErrorKind::ProviderMessage
        };
        return Err(
            ProviderError::new(source, kind, format!("invalid Stooq header: {header}")).into(),
        );
    }

    let mut bars = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.len() != 6 {
            return Err(ProviderError::new(
                source,
                ProviderErrorKind::MalformedResponse,
                format!("invalid Stooq row: {line}"),
            )
            .into());
        }
        bars.push(DailyBar {
            instrument_id: instrument_id.to_string(),
            trade_date: parse_date(source, fields[0])?,
            open: parse_f64(source, fields[1], "open")?,
            high: parse_f64(source, fields[2], "high")?,
            low: parse_f64(source, fields[3], "low")?,
            close: parse_f64(source, fields[4], "close")?,
            volume: parse_optional_f64(fields[5]),
            amount: None,
            source: source.to_string(),
        });
    }

    if bars.is_empty() {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::NoData,
            "Stooq CSV contains no rows",
        )
        .into());
    }
    bars.sort_by_key(|bar| bar.trade_date);
    Ok(bars)
}

pub fn parse_alpha_vantage_csv_daily(
    instrument_id: &str,
    body: &str,
    source: &'static str,
) -> Result<Vec<DailyBar>> {
    let mut lines = body.lines().map(str::trim).filter(|line| !line.is_empty());
    let header = lines.next().ok_or_else(|| {
        ProviderError::new(
            source,
            ProviderErrorKind::NoData,
            "empty Alpha Vantage CSV response",
        )
    })?;
    let normalized = header.to_ascii_lowercase();
    if normalized != "timestamp,open,high,low,close,volume"
        && normalized != "date,open,high,low,close,volume"
    {
        return Err(ProviderError::new(
            source,
            ProviderErrorKind::MalformedResponse,
            format!("invalid Alpha Vantage CSV header: {header}"),
        )
        .into());
    }

    let mut bars = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        if fields.len() < 5 {
            return Err(ProviderError::new(
                source,
                ProviderErrorKind::MalformedResponse,
                format!("invalid Alpha Vantage CSV row: {line}"),
            )
            .into());
        }
        bars.push(DailyBar {
            instrument_id: instrument_id.to_string(),
            trade_date: parse_date(source, fields[0])?,
            open: parse_f64(source, fields[1], "open")?,
            high: parse_f64(source, fields[2], "high")?,
            low: parse_f64(source, fields[3], "low")?,
            close: parse_f64(source, fields[4], "close")?,
            volume: fields.get(5).and_then(|value| parse_optional_f64(value)),
            amount: None,
            source: source.to_string(),
        });
    }
    bars.sort_by_key(|bar| bar.trade_date);
    Ok(bars)
}

fn looks_like_html(body: &str) -> bool {
    let head = body.trim_start().to_ascii_lowercase();
    head.starts_with("<!doctype html") || head.starts_with("<html") || head.contains("<body")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderError;

    #[test]
    fn parses_stooq_csv() {
        let fixture = "Date,Open,High,Low,Close,Volume\n2024-01-02,10,11,9,10.5,123\n\n2024-01-03,10.5,12,10,11,-\n";
        let bars = parse_stooq_daily("foo", fixture, "stooq").unwrap();
        assert_eq!(bars.len(), 2);
        assert_eq!(
            bars[0].trade_date,
            NaiveDate::from_ymd_opt(2024, 1, 2).unwrap()
        );
        assert_eq!(bars[1].volume, None);
    }

    #[test]
    fn rejects_stooq_html() {
        let err = parse_stooq_daily("foo", "<html>nope</html>", "stooq").unwrap_err();
        let provider_err = err.downcast_ref::<ProviderError>().unwrap();
        assert_eq!(provider_err.kind, ProviderErrorKind::MalformedResponse);
    }
}
