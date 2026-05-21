use crate::config::Config;
use crate::db::Database;
use crate::models::{DataFreshness, MarketReport, ReportInstrument};
use crate::services::provider_errors::{
    error_still_relevant, format_error_time, friendly_message, kind_label_zh, provider_label_zh,
};
use anyhow::{Context, Result};
use chrono::{NaiveDate, Utc};
use std::{fs, path::Path};
use tracing::info;

pub fn write_json_report_for_date(
    config: &Config,
    db: &Database,
    date: NaiveDate,
) -> Result<String> {
    let report = build_report(config, db, date)?;
    let path = config.report_json_path(date);
    ensure_parent(&path)?;
    let json = serde_json::to_string_pretty(&report)?;
    fs::write(&path, json).with_context(|| format!("failed to write {path}"))?;
    info!(path = %path, "report written");
    Ok(path)
}

pub fn write_markdown_report_for_date(
    config: &Config,
    db: &Database,
    date: NaiveDate,
) -> Result<String> {
    let report = build_report(config, db, date)?;
    let path = config.report_markdown_path(date);
    ensure_parent(&path)?;
    fs::write(&path, render_markdown(&report))
        .with_context(|| format!("failed to write {path}"))?;
    info!(path = %path, "report written");
    Ok(path)
}

pub(crate) fn build_report(
    config: &Config,
    db: &Database,
    date: NaiveDate,
) -> Result<MarketReport> {
    let mut instruments = Vec::new();
    let mut freshness = Vec::new();
    for instrument in &config.instruments {
        let signal = db.latest_signal(&instrument.id)?;
        instruments.push(ReportInstrument {
            instrument_id: instrument.id.clone(),
            name: instrument.name.clone(),
            latest_close: signal.as_ref().map(|signal| signal.close),
            action: signal
                .as_ref()
                .map(|signal| signal.action.as_str().to_string())
                .unwrap_or_else(|| "HOLD".to_string()),
            score: signal
                .as_ref()
                .map(|signal| signal.score)
                .unwrap_or_default(),
            reason: signal
                .as_ref()
                .map(|signal| signal.reasons.join("；"))
                .unwrap_or_else(|| "尚未生成信号。".to_string()),
            source: signal.as_ref().map(|signal| signal.source.clone()),
            recent_trade_date: signal.as_ref().map(|signal| signal.trade_date),
        });

        let latest_bar_date = db.latest_bar_date(&instrument.id)?;
        freshness.push(DataFreshness {
            instrument_id: instrument.id.clone(),
            latest_bar_date,
            days_behind: latest_bar_date.map(|latest| (date - latest).num_days().max(0)),
        });
    }

    Ok(MarketReport {
        report_date: date,
        data_updated_at: Utc::now(),
        timezone: config.report.timezone.clone(),
        instruments,
        provider_errors: filter_relevant_provider_errors(db, date)?,
        freshness,
        disclaimer: "本报告为规则型行情整理输出，不构成任何投资建议。"
            .to_string(),
    })
}

pub(crate) fn render_markdown(report: &MarketReport) -> String {
    let buy_count = report
        .instruments
        .iter()
        .filter(|item| item.action == "BUY")
        .count();
    let sell_count = report
        .instruments
        .iter()
        .filter(|item| item.action == "SELL")
        .count();
    let hold_count = report
        .instruments
        .iter()
        .filter(|item| item.action == "HOLD")
        .count();

    let name_by_id: std::collections::HashMap<&str, &str> = report
        .instruments
        .iter()
        .map(|item| (item.instrument_id.as_str(), item.name.as_str()))
        .collect();

    let mut output = String::new();
    output.push_str("# 市场行情日报\n\n");
    output.push_str("## 概览\n");
    output.push_str(&format!("- 报告日期：{}\n", report.report_date));
    output.push_str(&format!("- 标的数量：{}\n", report.instruments.len()));
    output.push_str(&format!(
        "- 信号统计（买入 / 卖出 / 观望）：{buy_count} / {sell_count} / {hold_count}\n"
    ));
    output.push_str(&format!(
        "- 数据源错误：{}\n\n",
        report.provider_errors.len()
    ));

    output.push_str("## 交易信号\n\n");
    output.push_str("| 代码 | 名称 | 收盘价 | 信号 | 得分 | 说明 |\n");
    output.push_str("| --- | --- | ---: | --- | ---: | --- |\n");
    for item in &report.instruments {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            escape_md(&item.instrument_id),
            escape_md(&item.name),
            item.latest_close
                .map(|value| format!("{value:.4}"))
                .unwrap_or_else(|| "-".to_string()),
            action_label_zh(&item.action),
            item.score,
            escape_md(&item.reason),
        ));
    }

    output.push_str("\n## 数据源错误\n\n");
    if report.provider_errors.is_empty() {
        output.push_str("无需要关注的数据源错误（或已有足够新的行情，历史失败记录已忽略）。\n");
    } else {
        output.push_str("| 数据源 | 标的 | 类型 | 说明 | 时间 |\n");
        output.push_str("| --- | --- | --- | --- | --- |\n");
        for error in &report.provider_errors {
            output.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                escape_md(provider_label_zh(&error.provider)),
                escape_md(&error.instrument_id),
                escape_md(kind_label_zh(&error.kind)),
                escape_md(&friendly_message(&error.kind, &error.raw_message)),
                escape_md(&format_error_time(&error.created_at)),
            ));
        }
    }

    output.push_str("\n## 数据新鲜度\n\n");
    output.push_str("| 代码 | 名称 | 最新行情日 | 滞后天数 |\n");
    output.push_str("| --- | --- | --- | ---: |\n");
    for item in &report.freshness {
        let name = name_by_id
            .get(item.instrument_id.as_str())
            .copied()
            .unwrap_or("-");
        output.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            escape_md(&item.instrument_id),
            escape_md(name),
            item.latest_bar_date
                .map(|date| date.to_string())
                .unwrap_or_else(|| "-".to_string()),
            item.days_behind
                .map(|days| days.to_string())
                .unwrap_or_else(|| "-".to_string()),
        ));
    }

    output.push_str(&format!("\n{}\n", report.disclaimer));
    output
}

fn ensure_parent(path: &str) -> Result<()> {
    if let Some(parent) = Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create report directory {}", parent.display()))?;
    }
    Ok(())
}

fn filter_relevant_provider_errors(
    db: &Database,
    report_date: NaiveDate,
) -> Result<Vec<crate::models::ProviderErrorRecord>> {
    let mut out = Vec::new();
    for error in db.provider_errors_since(report_date)? {
        let latest = db.latest_bar_date(&error.instrument_id)?;
        if error_still_relevant(latest, report_date) {
            out.push(error);
        }
    }
    Ok(out)
}

fn action_label_zh(action: &str) -> &'static str {
    match action {
        "BUY" => "买入",
        "SELL" => "卖出",
        "HOLD" => "观望",
        _ => "未知",
    }
}

fn escape_md(value: &str) -> String {
    value.replace('|', "/").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::models::{DailyBar, Signal, SignalAction};

    #[test]
    fn report_markdown_contains_sections() {
        let config = Config::from_file("config.example.toml").unwrap();
        let path = std::env::temp_dir().join(format!(
            "marketfeed-report-{}.sqlite",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db.upsert_instruments(&config.instruments).unwrap();
        let signal = Signal {
            instrument_id: "fund501203".to_string(),
            trade_date: NaiveDate::from_ymd_opt(2026, 5, 20).unwrap(),
            action: SignalAction::Buy,
            score: 40,
            reasons: vec!["close > ma20".to_string()],
            source: "test".to_string(),
            close: 100.0,
            ma5: Some(99.0),
            ma20: Some(95.0),
            ma60: Some(90.0),
            deviation_ma20_pct: Some(5.0),
            change_20d_pct: Some(10.0),
            generated_at: Utc::now(),
        };
        db.upsert_signal(&signal).unwrap();
        let report =
            build_report(&config, &db, NaiveDate::from_ymd_opt(2026, 5, 21).unwrap()).unwrap();
        let markdown = render_markdown(&report);
        assert!(markdown.contains("# 市场行情日报"));
        assert!(markdown.contains("## 数据源错误"));
        assert!(markdown.contains("| 代码 | 名称 | 收盘价 | 信号 |"));
        assert!(markdown.contains("fund501203"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn report_formats_provider_errors_friendly() {
        let config = Config::from_file("config.example.toml").unwrap();
        let path = std::env::temp_dir().join(format!(
            "marketfeed-report-err-{}.sqlite",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db.insert_provider_error(
            "eastmoney",
            "sh000001",
            None,
            None,
            crate::providers::ProviderErrorKind::Http,
            "error sending request for url (https://push2his.eastmoney.com/api/qt/stock/kline/get?secid=1.000001)",
        )
        .unwrap();
        db.insert_provider_error(
            "stooq",
            "sh000001",
            None,
            None,
            crate::providers::ProviderErrorKind::ProviderMessage,
            "invalid Stooq header: No data",
        )
        .unwrap();
        let report =
            build_report(&config, &db, NaiveDate::from_ymd_opt(2026, 5, 21).unwrap()).unwrap();
        let markdown = render_markdown(&report);
        assert!(markdown.contains("无法连接东方财富 API"));
        assert!(markdown.contains("Stooq 返回「No data」"));
        assert!(!markdown.contains("push2his.eastmoney.com"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn data_freshness_days_behind_is_calculated() {
        let config = Config::from_file("config.example.toml").unwrap();
        let path = std::env::temp_dir().join(format!(
            "marketfeed-freshness-{}.sqlite",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db.upsert_instruments(&config.instruments).unwrap();
        db.upsert_daily_bars(&[DailyBar {
            instrument_id: "fund501203".to_string(),
            trade_date: NaiveDate::from_ymd_opt(2026, 5, 19).unwrap(),
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: None,
            amount: None,
            source: "test".to_string(),
        }])
        .unwrap();
        let report =
            build_report(&config, &db, NaiveDate::from_ymd_opt(2026, 5, 21).unwrap()).unwrap();
        let fund501203 = report
            .freshness
            .iter()
            .find(|item| item.instrument_id == "fund501203")
            .unwrap();
        assert_eq!(fund501203.days_behind, Some(2));
        let _ = std::fs::remove_file(path);
    }
}
