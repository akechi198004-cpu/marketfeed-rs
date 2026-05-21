use crate::config::Config;
use crate::db::Database;
use crate::providers::stooq::resolve_stooq_symbol;
use crate::services::provider_errors::{format_error, record_empty_response, record_provider_error};
use crate::services::updater::{
    fetch_with_retries, select_instruments, InstrumentUpdateSummary, UpdateSummary,
};
use crate::utils::date;
use anyhow::Result;
use chrono::{Duration, NaiveDate};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub struct BootstrapOptions {
    pub instrument_id: Option<String>,
    pub from: Option<NaiveDate>,
    pub dry_run: bool,
}

pub async fn bootstrap_with_options(
    config: &Config,
    db: &Database,
    options: BootstrapOptions,
) -> Result<UpdateSummary> {
    let end = date::today_utc();
    let earliest = end - Duration::days(config.update.max_bootstrap_days);
    let start = options.from.unwrap_or(earliest).max(earliest);
    let instruments = select_instruments(config, options.instrument_id.as_deref())?;
    let mut summaries = Vec::new();

    for instrument in instruments {
        let provider = bootstrap_provider(config, instrument);
        let fetch_end = match db.earliest_bar_date(&instrument.id)? {
            Some(earliest_existing) if earliest_existing <= start => {
                info!(instrument_id = %instrument.id, earliest = %earliest_existing, "history already covers bootstrap range");
                summaries.push(InstrumentUpdateSummary {
                    instrument_id: instrument.id.clone(),
                    provider: provider.to_string(),
                    bars_written: 0,
                    error: None,
                });
                continue;
            }
            Some(earliest_existing) => earliest_existing - Duration::days(1),
            None => end,
        };

        info!(instrument_id = %instrument.id, provider = provider, %start, end = %fetch_end, dry_run = options.dry_run, "bootstrap planned");
        if options.dry_run {
            summaries.push(InstrumentUpdateSummary {
                instrument_id: instrument.id.clone(),
                provider: provider.to_string(),
                bars_written: 0,
                error: None,
            });
            continue;
        }

        match fetch_with_retries(config, instrument, provider, start, fetch_end).await {
            Ok(bars) => {
                if bars.is_empty() {
                    warn!(instrument_id = %instrument.id, provider = provider, "bootstrap provider returned empty bars");
                    record_empty_response(
                        db,
                        provider,
                        &instrument.id,
                        Some(start),
                        Some(fetch_end),
                        "历史初始化返回空数据",
                    )?;
                    summaries.push(InstrumentUpdateSummary {
                        instrument_id: instrument.id.clone(),
                        provider: provider.to_string(),
                        bars_written: 0,
                        error: Some("[无数据] 历史初始化返回空数据".to_string()),
                    });
                } else {
                    let count = db.upsert_daily_bars(&bars)?;
                    info!(instrument_id = %instrument.id, provider = provider, count, "bars upserted");
                    summaries.push(InstrumentUpdateSummary {
                        instrument_id: instrument.id.clone(),
                        provider: provider.to_string(),
                        bars_written: count,
                        error: None,
                    });
                }
            }
            Err(err) if provider != instrument.history_provider() => {
                warn!(instrument_id = %instrument.id, provider = provider, error = %err, fallback = instrument.history_provider(), "bootstrap primary provider failed, trying fallback");
                match fetch_with_retries(
                    config,
                    instrument,
                    instrument.history_provider(),
                    start,
                    fetch_end,
                )
                .await
                {
                    Ok(bars) if !bars.is_empty() => {
                        let count = db.upsert_daily_bars(&bars)?;
                        summaries.push(InstrumentUpdateSummary {
                            instrument_id: instrument.id.clone(),
                            provider: instrument.history_provider().to_string(),
                            bars_written: count,
                            error: None,
                        });
                    }
                    Ok(_) => {
                        record_empty_response(
                            db,
                            instrument.history_provider(),
                            &instrument.id,
                            Some(start),
                            Some(fetch_end),
                            "历史初始化 fallback 返回空数据",
                        )?;
                        summaries.push(InstrumentUpdateSummary {
                            instrument_id: instrument.id.clone(),
                            provider: instrument.history_provider().to_string(),
                            bars_written: 0,
                            error: Some("[无数据] 历史初始化返回空数据".to_string()),
                        });
                    }
                    Err(fallback_err) => {
                        record_provider_error(
                            db,
                            provider,
                            &instrument.id,
                            Some(start),
                            Some(fetch_end),
                            &err,
                        )?;
                        record_provider_error(
                            db,
                            instrument.history_provider(),
                            &instrument.id,
                            Some(start),
                            Some(fetch_end),
                            &fallback_err,
                        )?;
                        summaries.push(InstrumentUpdateSummary {
                            instrument_id: instrument.id.clone(),
                            provider: provider.to_string(),
                            bars_written: 0,
                            error: Some(format!(
                                "{}；fallback: {}",
                                format_error(&err),
                                format_error(&fallback_err)
                            )),
                        });
                    }
                }
            }
            Err(err) => {
                warn!(instrument_id = %instrument.id, provider = provider, error = %err, "provider error recorded");
                record_provider_error(
                    db,
                    provider,
                    &instrument.id,
                    Some(start),
                    Some(fetch_end),
                    &err,
                )?;
                summaries.push(InstrumentUpdateSummary {
                    instrument_id: instrument.id.clone(),
                    provider: provider.to_string(),
                    bars_written: 0,
                    error: Some(format_error(&err)),
                });
            }
        }
    }

    Ok(UpdateSummary {
        start,
        end,
        dry_run: options.dry_run,
        instruments: summaries,
    })
}

fn bootstrap_provider<'a>(
    config: &Config,
    instrument: &'a crate::config::InstrumentConfig,
) -> &'a str {
    if config.providers.stooq_enabled && resolve_stooq_symbol(instrument).is_some() {
        "stooq"
    } else {
        instrument.history_provider()
    }
}
