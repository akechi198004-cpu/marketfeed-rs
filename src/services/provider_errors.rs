use crate::db::Database;
use crate::providers::{ProviderError, ProviderErrorKind};
use anyhow::{Error, Result};
use chrono::NaiveDate;

pub fn record_provider_error(
    db: &Database,
    provider: &str,
    instrument_id: &str,
    range_start: Option<NaiveDate>,
    range_end: Option<NaiveDate>,
    err: &Error,
) -> Result<()> {
    if let Some(provider_err) = err.downcast_ref::<ProviderError>() {
        db.insert_provider_error(
            provider_err.provider,
            instrument_id,
            range_start,
            range_end,
            provider_err.kind,
            &provider_err.raw_message,
        )
    } else {
        db.insert_provider_error(
            provider,
            instrument_id,
            range_start,
            range_end,
            ProviderErrorKind::ProviderMessage,
            &err.to_string(),
        )
    }
}

pub fn record_empty_response(
    db: &Database,
    provider: &str,
    instrument_id: &str,
    range_start: Option<NaiveDate>,
    range_end: Option<NaiveDate>,
    message: &str,
) -> Result<()> {
    db.insert_provider_error(
        provider,
        instrument_id,
        range_start,
        range_end,
        ProviderErrorKind::NoData,
        message,
    )
}

/// 错误类型中文标签（用于控制台与报告）。
pub fn kind_label_zh(kind: &str) -> &'static str {
    match kind {
        "http" => "网络错误",
        "rate_limited" => "频率限制",
        "auth" => "认证失败",
        "no_data" => "无数据",
        "malformed_response" => "响应格式错误",
        "unsupported_instrument" => "标的不支持",
        "provider_message" => "数据源提示",
        _ => "未知错误",
    }
}

/// 数据源名称中文标签。
pub fn provider_label_zh(provider: &str) -> &str {
    match provider {
        "stooq" => "Stooq",
        "eastmoney" | "eastmoney_fund" => "东方财富",
        "alpha_vantage" => "Alpha Vantage",
        "disabled" => "已禁用",
        "unknown" => "未知",
        other => other,
    }
}

/// 将原始错误消息转为用户可读说明。
pub fn friendly_message(kind: &str, raw: &str) -> String {
    match kind {
        "http" => friendly_http_message(raw),
        "rate_limited" => format!("API 请求频率受限：{}", truncate(raw, 120)),
        "auth" => friendly_auth_message(raw),
        "no_data" => friendly_no_data_message(raw),
        "malformed_response" => friendly_malformed_message(raw),
        "unsupported_instrument" => truncate(raw, 120),
        "provider_message" => friendly_provider_message(raw),
        _ => truncate(&sanitize_urls(raw), 120),
    }
}

/// 格式化 `anyhow::Error`，供控制台输出。
pub fn format_error(err: &Error) -> String {
    if let Some(provider_err) = err.downcast_ref::<ProviderError>() {
        return format!(
            "[{}] {}",
            kind_label_zh(provider_err.kind.as_str()),
            friendly_message(provider_err.kind.as_str(), &provider_err.raw_message)
        );
    }
    truncate(&sanitize_urls(&err.to_string()), 160)
}

/// 若标的已有足够新的行情，则历史拉取失败可忽略，不必在报告中提示。
pub fn error_still_relevant(latest_bar_date: Option<NaiveDate>, report_date: NaiveDate) -> bool {
    match latest_bar_date {
        None => true,
        Some(latest) => (report_date - latest).num_days() > 2,
    }
}

/// 格式化报告中的错误时间戳。
pub fn format_error_time(created_at: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(created_at)
        .map(|dt| dt.format("%Y-%m-%d %H:%M UTC").to_string())
        .unwrap_or_else(|_| created_at.to_string())
}

fn friendly_http_message(raw: &str) -> String {
    if raw.contains("error sending request") {
        if raw.contains("eastmoney.com") {
            return "无法连接东方财富 API（网络不可达或请求超时）".to_string();
        }
        if raw.contains("stooq.com") {
            return "无法连接 Stooq API（网络不可达或请求超时）".to_string();
        }
        if raw.contains("alphavantage.co") {
            return "无法连接 Alpha Vantage API（网络不可达或请求超时）".to_string();
        }
        return "网络请求失败（连接超时或目标不可达）".to_string();
    }
    if raw.contains("HTTP status client error") {
        return format!(
            "HTTP 客户端错误{}",
            extract_http_status_suffix(raw)
        );
    }
    if raw.contains("HTTP status server error") {
        return format!(
            "HTTP 服务端错误{}",
            extract_http_status_suffix(raw)
        );
    }
    if raw.contains("timed out") || raw.contains("timeout") {
        return "请求超时".to_string();
    }
    truncate(&sanitize_urls(raw), 120)
}

fn friendly_auth_message(raw: &str) -> String {
    if raw.contains("Alpha Vantage") {
        "缺少 Alpha Vantage API Key，请配置 api_key 或环境变量 ALPHA_VANTAGE_API_KEY".to_string()
    } else {
        truncate(raw, 120)
    }
}

fn friendly_no_data_message(raw: &str) -> String {
    if raw.contains("Eastmoney") {
        "东方财富未返回该日期范围内的 K 线数据".to_string()
    } else if raw.contains("Stooq") {
        "Stooq 未返回该日期范围内的 CSV 数据".to_string()
    } else if raw.contains("Alpha Vantage") {
        "Alpha Vantage 在请求日期范围内无数据".to_string()
    } else if raw.contains("历史") || raw.contains("空数据") || raw.contains("NAV") {
        raw.to_string()
    } else {
        truncate(raw, 120)
    }
}

fn friendly_malformed_message(raw: &str) -> String {
    if raw.contains("HTML instead of CSV") {
        return "Stooq 返回网页而非 CSV（symbol 可能无效或被限流）".to_string();
    }
    if raw.contains("invalid Eastmoney JSON") {
        return "东方财富返回的 JSON 无法解析".to_string();
    }
    if raw.contains("invalid Eastmoney kline") {
        return "东方财富 K 线字段格式异常".to_string();
    }
    if raw.contains("Eastmoney fund") {
        return "东方财富基金净值页面格式异常".to_string();
    }
    if raw.contains("invalid Alpha Vantage") {
        return "Alpha Vantage 响应格式异常".to_string();
    }
    truncate(&sanitize_urls(raw), 120)
}

fn friendly_provider_message(raw: &str) -> String {
    if let Some(detail) = raw.strip_prefix("invalid Stooq header: ") {
        let detail = detail.trim();
        if detail.eq_ignore_ascii_case("no data") {
            return "Stooq 返回「No data」，该标的暂无可用历史数据".to_string();
        }
        return format!("Stooq 返回异常响应：{detail}");
    }
    if raw.contains("Thank you for using Alpha Vantage") {
        return "Alpha Vantage 免费 API 调用次数已用完".to_string();
    }
    truncate(raw, 120)
}

fn extract_http_status_suffix(raw: &str) -> String {
    if let Some(start) = raw.find('(') {
        if let Some(end) = raw[start..].find(')') {
            return format!(" ({})", &raw[start + 1..start + end]);
        }
    }
    String::new()
}

fn sanitize_urls(raw: &str) -> String {
    if let Some(idx) = raw.find("for url (") {
        let prefix = raw[..idx].trim_end_matches("for").trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    raw.to_string()
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        format!("{}…", value.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderError;

    #[test]
    fn friendly_http_eastmoney() {
        let raw = "error sending request for url (https://push2his.eastmoney.com/api/qt/stock/kline/get?secid=1.000001)";
        let msg = friendly_message("http", raw);
        assert_eq!(msg, "无法连接东方财富 API（网络不可达或请求超时）");
    }

    #[test]
    fn friendly_stooq_no_data() {
        let msg = friendly_message("provider_message", "invalid Stooq header: No data");
        assert_eq!(msg, "Stooq 返回「No data」，该标的暂无可用历史数据");
    }

    #[test]
    fn format_provider_error_struct() {
        let err: Error = ProviderError::new(
            "eastmoney",
            ProviderErrorKind::Http,
            "error sending request for url (https://example.com)",
        )
        .into();
        let formatted = format_error(&err);
        assert!(formatted.starts_with("[网络错误]"));
        assert!(!formatted.contains("https://"));
    }

    #[test]
    fn kind_label_covers_all_kinds() {
        assert_eq!(kind_label_zh("http"), "网络错误");
        assert_eq!(kind_label_zh("no_data"), "无数据");
    }

    #[test]
    fn stale_error_not_relevant_when_data_fresh() {
        use chrono::NaiveDate;
        let report = NaiveDate::from_ymd_opt(2026, 5, 21).unwrap();
        let latest = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        assert!(!error_still_relevant(Some(latest), report));
    }
}
