use crate::config::{Config, InstrumentConfig};
use crate::db::Database;
use crate::models::DailyBar;
use crate::providers::{
    alpha_vantage::AlphaVantageProvider,
    eastmoney::EastmoneyProvider,
    stooq::{resolve_stooq_symbol, StooqProvider},
    MarketDataProvider, ProviderError, ProviderErrorKind,
};
use crate::services::provider_errors::{format_error, record_provider_error};
use crate::utils::date;
use anyhow::Result;
use chrono::{Duration, NaiveDate};
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct UpdateOptions {
    pub instrument_id: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct InstrumentUpdateSummary {
    pub instrument_id: String,
    pub provider: String,
    pub bars_written: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateSummary {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub dry_run: bool,
    pub instruments: Vec<InstrumentUpdateSummary>,
}

impl UpdateSummary {
    pub fn provider_error_count(&self) -> usize {
        self.instruments
            .iter()
            .filter(|item| item.error.is_some())
            .count()
    }
}

pub async fn update_recent(config: &Config, db: &Database) -> Result<UpdateSummary> {
    update(
        config,
        db,
        UpdateOptions {
            instrument_id: None,
            from: None,
            to: None,
            dry_run: false,
        },
    )
    .await
}

pub async fn update(
    config: &Config,
    db: &Database,
    options: UpdateOptions,
) -> Result<UpdateSummary> {
    let instruments = select_instruments(config, options.instrument_id.as_deref())?;
    let mut summaries = Vec::new();
    let mut range_start: Option<NaiveDate> = options.from;
    let mut range_end: Option<NaiveDate> = options.to;

    for instrument in instruments {
        let end = options
            .to
            .unwrap_or_else(|| date::incremental_end_date(&instrument.market));
        let start = options
            .from
            .unwrap_or_else(|| end - Duration::days(config.update.default_lookback_days));
        range_start = Some(range_start.map_or(start, |v| v.min(start)));
        range_end = Some(range_end.map_or(end, |v| v.max(end)));

        info!(instrument_id = %instrument.id, provider = %instrument.provider, %start, %end, dry_run = options.dry_run, "update planned");
        if options.dry_run {
            summaries.push(InstrumentUpdateSummary {
                instrument_id: instrument.id.clone(),
                provider: instrument.provider.clone(),
                bars_written: 0,
                error: None,
            });
            continue;
        }

        match fetch_for_update(config, db, instrument, start, end).await {
            UpdateFetchResult::Success {
                bars,
                provider_used,
            } => {
                let count = db.upsert_daily_bars(&bars)?;
                info!(
                    instrument_id = %instrument.id,
                    provider = %provider_used,
                    count,
                    "bars upserted"
                );
                summaries.push(InstrumentUpdateSummary {
                    instrument_id: instrument.id.clone(),
                    provider: provider_used,
                    bars_written: count,
                    error: None,
                });
            }
            UpdateFetchResult::SkippedFresh => {
                warn!(
                    instrument_id = %instrument.id,
                    "update failed or empty but existing data is fresh enough, skipping error record"
                );
                summaries.push(InstrumentUpdateSummary {
                    instrument_id: instrument.id.clone(),
                    provider: instrument.provider.clone(),
                    bars_written: 0,
                    error: None,
                });
            }
            UpdateFetchResult::Failed { err } => {
                warn!(instrument_id = %instrument.id, error = %err, "provider error recorded");
                record_provider_error(
                    db,
                    instrument.daily_provider(),
                    &instrument.id,
                    Some(start),
                    Some(end),
                    &err,
                )?;
                summaries.push(InstrumentUpdateSummary {
                    instrument_id: instrument.id.clone(),
                    provider: instrument.provider.clone(),
                    bars_written: 0,
                    error: Some(format_error(&err)),
                });
            }
        }
    }

    let end = range_end.unwrap_or_else(date::today_utc);
    let start =
        range_start.unwrap_or_else(|| end - Duration::days(config.update.default_lookback_days));

    Ok(UpdateSummary {
        start,
        end,
        dry_run: options.dry_run,
        instruments: summaries,
    })
}

/// 已有行情与请求截止日相差不超过 2 个自然日，视为数据仍可用（如 bootstrap 已写入）。
fn data_is_fresh_enough(latest_bar_date: Option<NaiveDate>, end: NaiveDate) -> bool {
    latest_bar_date.is_some_and(|latest| (end - latest).num_days() <= 2)
}

enum UpdateFetchResult {
    Success {
        bars: Vec<DailyBar>,
        provider_used: String,
    },
    SkippedFresh,
    Failed {
        err: anyhow::Error,
    },
}

/// 先走配置的主 provider；对可解析 Stooq 符号的标的（如 sh000001），失败或空数据时回退 Stooq。
async fn fetch_for_update(
    config: &Config,
    db: &Database,
    instrument: &InstrumentConfig,
    start: NaiveDate,
    end: NaiveDate,
) -> UpdateFetchResult {
    let primary = instrument.daily_provider();
    let primary_result = fetch_with_retries(config, instrument, primary, start, end).await;

    if let Ok(ref bars) = primary_result {
        if !bars.is_empty() {
            return UpdateFetchResult::Success {
                bars: bars.clone(),
                provider_used: primary.to_string(),
            };
        }
    }

    if let Some(fallback) = stooq_fallback_provider(config, instrument) {
        info!(
            instrument_id = %instrument.id,
            primary,
            fallback,
            "primary provider empty or failed, trying stooq fallback"
        );
        if let Ok(bars) = fetch_with_retries(config, instrument, fallback, start, end).await {
            if !bars.is_empty() {
                return UpdateFetchResult::Success {
                    bars,
                    provider_used: fallback.to_string(),
                };
            }
        }
    }

    let latest = db.latest_bar_date(&instrument.id).ok().flatten();
    if data_is_fresh_enough(latest, end) {
        return UpdateFetchResult::SkippedFresh;
    }

    match primary_result {
        Err(err) => UpdateFetchResult::Failed { err },
        Ok(_) => UpdateFetchResult::Failed {
            err: ProviderError::new(
                provider_static_name(primary),
                ProviderErrorKind::NoData,
                if stooq_fallback_provider(config, instrument).is_some() {
                    "主数据源与 Stooq 回退均未返回数据"
                } else {
                    "增量更新返回空数据"
                },
            )
            .into(),
        },
    }
}

fn provider_static_name(name: &str) -> &'static str {
    match name {
        "stooq" => "stooq",
        "eastmoney" => "eastmoney",
        "alpha_vantage" => "alpha_vantage",
        _ => "unknown",
    }
}

fn stooq_fallback_provider(config: &Config, instrument: &InstrumentConfig) -> Option<&'static str> {
    if config.providers.stooq_enabled
        && instrument.daily_provider() != "stooq"
        && resolve_stooq_symbol(instrument).is_some()
    {
        Some("stooq")
    } else {
        None
    }
}

pub(crate) fn select_instruments<'a>(
    config: &'a Config,
    id: Option<&str>,
) -> Result<Vec<&'a InstrumentConfig>> {
    if let Some(id) = id {
        Ok(vec![config.instrument(id)?])
    } else {
        Ok(config.instruments.iter().collect())
    }
}

pub(crate) async fn fetch_with_retries(
    config: &Config,
    instrument: &InstrumentConfig,
    provider_name: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<DailyBar>> {
    let mut last_error = None;
    for attempt in 0..=config.update.retry_count {
        debug!(instrument_id = %instrument.id, provider = provider_name, attempt, "provider request start");
        match fetch_once(config, instrument, provider_name, start, end).await {
            Ok(bars) => {
                debug!(instrument_id = %instrument.id, provider = provider_name, rows = bars.len(), "provider request end");
                return Ok(bars);
            }
            Err(err) => {
                last_error = Some(err);
                if attempt < config.update.retry_count {
                    sleep(TokioDuration::from_millis(config.update.retry_delay_ms)).await;
                }
            }
        }
    }
    Err(last_error.expect("retry loop runs at least once"))
}

async fn fetch_once(
    config: &Config,
    instrument: &InstrumentConfig,
    provider_name: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<Vec<DailyBar>> {
    match provider_name {
        "stooq" if config.providers.stooq_enabled => {
            StooqProvider::new(config.stooq_api_key(), &config.providers.stooq.base_url)
                .fetch_daily_bars(instrument, start, end)
                .await
        }
        "alpha_vantage" if config.providers.alpha_vantage_enabled => {
            AlphaVantageProvider::new(
                config.alpha_api_key(),
                &config.providers.alpha_vantage.base_url,
            )
            .fetch_daily_bars(instrument, start, end)
            .await
        }
        "eastmoney" if config.providers.eastmoney_enabled => {
            EastmoneyProvider::new(
                &config.providers.eastmoney.base_url,
                &config.providers.eastmoney.fund_base_url,
            )
            .fetch_daily_bars(instrument, start, end)
            .await
        }
        "stooq" | "alpha_vantage" | "eastmoney" => Err(ProviderError::new(
            "disabled",
            ProviderErrorKind::UnsupportedInstrument,
            format!("provider {provider_name} is disabled"),
        )
        .into()),
        other => Err(ProviderError::new(
            "unknown",
            ProviderErrorKind::UnsupportedInstrument,
            format!("unknown provider: {other}"),
        )
        .into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[tokio::test]
    async fn dry_run_does_not_write_daily_bars() {
        let config = Config::from_file("config.example.toml").unwrap();
        let path = std::env::temp_dir().join(format!(
            "marketfeed-dry-run-{}.sqlite",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db.upsert_instruments(&config.instruments).unwrap();
        let summary = update(
            &config,
            &db,
            UpdateOptions {
                instrument_id: Some("fund501203".to_string()),
                from: NaiveDate::from_ymd_opt(2024, 1, 1),
                to: NaiveDate::from_ymd_opt(2024, 1, 5),
                dry_run: true,
            },
        )
        .await
        .unwrap();
        assert!(summary.dry_run);
        assert_eq!(db.count_daily_bars("fund501203").unwrap(), 0);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn update_continues_when_provider_fails() {
        let mut config = Config::from_file("config.example.toml").unwrap();
        config.providers.stooq_enabled = false;
        config.providers.alpha_vantage_enabled = false;
        config.providers.eastmoney_enabled = false;
        config.update.retry_count = 0;
        let path = std::env::temp_dir().join(format!(
            "marketfeed-provider-fail-{}.sqlite",
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db.upsert_instruments(&config.instruments).unwrap();
        let summary = update(
            &config,
            &db,
            UpdateOptions {
                instrument_id: None,
                from: NaiveDate::from_ymd_opt(2024, 1, 1),
                to: NaiveDate::from_ymd_opt(2024, 1, 2),
                dry_run: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(summary.instruments.len(), config.instruments.len());
        assert_eq!(summary.provider_error_count(), config.instruments.len());
        let _ = std::fs::remove_file(path);
    }
}
