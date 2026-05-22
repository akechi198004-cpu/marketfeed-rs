use crate::config::Config;
use crate::db::Database;
use crate::models::{DailyBar, Signal, SignalAction};
use anyhow::Result;
use chrono::Utc;
use tracing::info;

pub const MIN_HISTORY: usize = 120;

/// 默认只回补最近 120 个交易日的信号（连续统计最多用到 10 条，留足余量）。
pub const DEFAULT_BACKFILL_TRADING_DAYS: usize = 120;

#[derive(Debug, Clone, Default)]
pub struct BackfillOptions {
    pub instrument_id: Option<String>,
    /// 从最新交易日向前回补的交易日数量；`None` 表示自第 120 根 K 线起全部回补。
    pub trading_days: Option<usize>,
}

#[derive(Debug)]
pub struct BackfillSummary {
    pub instruments: Vec<BackfillInstrumentSummary>,
}

#[derive(Debug)]
pub struct BackfillInstrumentSummary {
    pub instrument_id: String,
    pub bars_total: usize,
    pub signals_written: usize,
    pub first_trade_date: Option<chrono::NaiveDate>,
    pub last_trade_date: Option<chrono::NaiveDate>,
}

pub fn calculate_and_store(config: &Config, db: &Database) -> Result<()> {
    for instrument in &config.instruments {
        let bars = db.latest_bars_for_signal(&instrument.id, 260)?;
        let signal = calculate_signal(&instrument.id, &bars);
        db.upsert_signal(&signal)?;
        info!(instrument_id = %instrument.id, action = signal.action.as_str(), "stored signal");
    }
    Ok(())
}

/// 用已有日线，按每个交易日重算并写入 `signals`（供「连续 N 日」统计）。
pub fn backfill_signals(config: &Config, db: &Database, options: BackfillOptions) -> Result<BackfillSummary> {
    let trading_days = options
        .trading_days
        .unwrap_or(DEFAULT_BACKFILL_TRADING_DAYS);
    let mut instruments = Vec::new();

    for instrument in &config.instruments {
        if let Some(filter) = &options.instrument_id {
            if &instrument.id != filter {
                continue;
            }
        }

        let bars = db.daily_bars_ascending(&instrument.id)?;
        if bars.len() < MIN_HISTORY {
            info!(
                instrument_id = %instrument.id,
                bars = bars.len(),
                "skip backfill: insufficient daily bars"
            );
            instruments.push(BackfillInstrumentSummary {
                instrument_id: instrument.id.clone(),
                bars_total: bars.len(),
                signals_written: 0,
                first_trade_date: None,
                last_trade_date: bars.last().map(|b| b.trade_date),
            });
            continue;
        }

        let start_idx = bars.len().saturating_sub(trading_days).max(MIN_HISTORY - 1);
        let mut written = 0usize;
        let mut first_date = None;
        let mut last_date = None;

        for i in start_idx..bars.len() {
            let signal = calculate_signal(&instrument.id, &bars[..=i]);
            db.upsert_signal(&signal)?;
            written += 1;
            if first_date.is_none() {
                first_date = Some(signal.trade_date);
            }
            last_date = Some(signal.trade_date);
        }

        info!(
            instrument_id = %instrument.id,
            written,
            %start_idx,
            total = bars.len(),
            "backfilled signals"
        );
        instruments.push(BackfillInstrumentSummary {
            instrument_id: instrument.id.clone(),
            bars_total: bars.len(),
            signals_written: written,
            first_trade_date: first_date,
            last_trade_date: last_date,
        });
    }

    Ok(BackfillSummary { instruments })
}

pub(crate) fn calculate_signal(instrument_id: &str, bars: &[DailyBar]) -> Signal {
    let fallback_date = bars
        .last()
        .map(|bar| bar.trade_date)
        .unwrap_or_else(|| Utc::now().date_naive());
    let latest = bars.last();

    if bars.len() < MIN_HISTORY {
        return Signal {
            instrument_id: instrument_id.to_string(),
            trade_date: fallback_date,
            action: SignalAction::Hold,
            score: 0,
            reasons: vec![format!(
                "insufficient_history: 中长期信号至少需要 {} 条日线，目前仅有 {} 条。",
                MIN_HISTORY,
                bars.len()
            )],
            source: latest
                .map(|bar| bar.source.clone())
                .unwrap_or_else(|| "none".to_string()),
            close: latest.map(|bar| bar.close).unwrap_or_default(),
            ma20: None,
            ma60: None,
            ma120: None,
            deviation_ma60_pct: None,
            deviation_ma120_pct: None,
            change_60d_pct: None,
            drawdown_120d_pct: None,
            generated_at: Utc::now(),
        };
    }

    let latest = latest.expect("checked len").clone();
    let ma20 = ma(&bars, 20);
    let ma60 = ma(&bars, 60);
    let ma120 = ma(&bars, 120);
    let deviation_ma60_pct = pct(latest.close, ma60);
    let deviation_ma120_pct = pct(latest.close, ma120);
    let base_60 = bars[bars.len() - 60].close;
    let change_60d_pct = pct(latest.close, base_60);
    let window_120 = &bars[bars.len() - 120..];
    let high_120 = window_120
        .iter()
        .map(|bar| bar.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let drawdown_120d_pct = pct(latest.close, high_120);

    let mut score = 0;
    let mut reasons = Vec::new();

    if latest.close > ma60 && ma20 > ma60 && ma60 > ma120 {
        score += 45;
        reasons.push("中长期趋势偏多：close > ma60，且 ma20 > ma60 > ma120。".to_string());
    } else if latest.close < ma60 && ma20 < ma60 && ma60 < ma120 {
        score -= 45;
        reasons.push("中长期趋势偏空：close < ma60，且 ma20 < ma60 < ma120。".to_string());
    } else if latest.close > ma120 && ma60 > ma120 {
        score += 25;
        reasons.push("长期结构偏多：close 与 ma60 均在 ma120 上方。".to_string());
    } else if latest.close < ma120 && ma60 < ma120 {
        score -= 25;
        reasons.push("长期结构偏空：close 与 ma60 均在 ma120 下方。".to_string());
    } else {
        reasons.push("长期均线结构交织，趋势方向不明确。".to_string());
    }

    if change_60d_pct > 15.0 {
        score += 20;
        reasons.push(format!(
            "近 60 日涨幅 {:.2}%，中期动量偏强。",
            change_60d_pct
        ));
    } else if change_60d_pct < -15.0 {
        score -= 20;
        reasons.push(format!(
            "近 60 日跌幅 {:.2}%，中期动量偏弱。",
            change_60d_pct.abs()
        ));
    } else if change_60d_pct > 5.0 {
        score += 10;
        reasons.push(format!(
            "近 60 日上涨 {:.2}%，动量温和偏强。",
            change_60d_pct
        ));
    } else if change_60d_pct < -5.0 {
        score -= 10;
        reasons.push(format!(
            "近 60 日下跌 {:.2}%，动量温和偏弱。",
            change_60d_pct.abs()
        ));
    } else {
        reasons.push(format!(
            "近 60 日涨跌幅 {:.2}%，中期动量中性。",
            change_60d_pct
        ));
    }

    if deviation_ma120_pct > 35.0 {
        score -= 25;
        reasons.push(format!(
            "close 高于 ma120 {:.2}%，中长期位置过热，显著降低追高分数。",
            deviation_ma120_pct
        ));
    } else if deviation_ma120_pct > 20.0 {
        score -= 10;
        reasons.push(format!(
            "close 高于 ma120 {:.2}%，位置偏高，降低追高分数。",
            deviation_ma120_pct
        ));
    } else if deviation_ma120_pct < -35.0 {
        score += 10;
        reasons.push(format!(
            "close 低于 ma120 {:.2}%，长期位置偏低，但仍需等待趋势确认。",
            deviation_ma120_pct.abs()
        ));
    } else if deviation_ma120_pct < -20.0 {
        score += 5;
        reasons.push(format!(
            "close 低于 ma120 {:.2}%，长期位置偏低，降低杀跌倾向。",
            deviation_ma120_pct.abs()
        ));
    }

    if drawdown_120d_pct > -5.0 && deviation_ma120_pct > 20.0 {
        score -= 10;
        reasons.push("价格接近 120 日高点且相对 ma120 偏高，控制追高。".to_string());
    } else if drawdown_120d_pct < -25.0 && latest.close > ma120 {
        score += 10;
        reasons.push(format!(
            "较 120 日高点回撤 {:.2}%，但仍在 ma120 上方，属于趋势内回撤。",
            drawdown_120d_pct.abs()
        ));
    } else {
        reasons.push(format!(
            "较 120 日高点回撤 {:.2}%。",
            drawdown_120d_pct.abs()
        ));
    }

    let score = score.clamp(-100, 100);
    let action = if score >= 35 {
        SignalAction::Buy
    } else if score <= -35 {
        SignalAction::Sell
    } else {
        SignalAction::Hold
    };

    Signal {
        instrument_id: instrument_id.to_string(),
        trade_date: latest.trade_date,
        action,
        score,
        reasons,
        source: latest.source,
        close: latest.close,
        ma20: Some(ma20),
        ma60: Some(ma60),
        ma120: Some(ma120),
        deviation_ma60_pct: Some(deviation_ma60_pct),
        deviation_ma120_pct: Some(deviation_ma120_pct),
        change_60d_pct: Some(change_60d_pct),
        drawdown_120d_pct: Some(drawdown_120d_pct),
        generated_at: Utc::now(),
    }
}

fn ma(bars: &[DailyBar], n: usize) -> f64 {
    let start = bars.len().saturating_sub(n);
    let slice = &bars[start..];
    slice.iter().map(|bar| bar.close).sum::<f64>() / slice.len() as f64
}

fn pct(value: f64, base: f64) -> f64 {
    if base == 0.0 {
        0.0
    } else {
        (value / base - 1.0) * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn backfill_window_matches_full_series_at_end() {
        let bars = make_bars(100.0, 0.12, 180);
        let full = calculate_signal("foo", &bars);
        let window = calculate_signal("foo", &bars[..180]);
        assert_eq!(full.trade_date, window.trade_date);
        assert_eq!(full.action, window.action);
        assert_eq!(full.score, window.score);
    }

    #[test]
    fn rising_trend_gets_positive_score() {
        let bars = make_bars(100.0, 0.12, 180);
        let signal = calculate_signal("foo", &bars);
        assert_eq!(signal.action, SignalAction::Buy);
        assert!(signal.score > 0);
        assert!(signal
            .reasons
            .iter()
            .any(|reason| reason.contains("中长期趋势偏多")));
    }

    #[test]
    fn falling_trend_gets_negative_score() {
        let bars = make_bars(100.0, -0.12, 180);
        let signal = calculate_signal("foo", &bars);
        assert_eq!(signal.action, SignalAction::Sell);
        assert!(signal.score < 0);
        assert!(signal
            .reasons
            .iter()
            .any(|reason| reason.contains("中长期趋势偏空")));
    }

    #[test]
    fn insufficient_history_holds() {
        let bars = make_bars(1.0, 1.0, 80);
        let signal = calculate_signal("foo", &bars);
        assert_eq!(signal.action, SignalAction::Hold);
        assert_eq!(signal.score, 0);
        assert!(signal.reasons[0].contains("insufficient_history"));
    }

    #[test]
    fn overextended_uptrend_is_tempered() {
        let mut bars = make_bars(100.0, 0.15, 179);
        let mut last = bars.last().unwrap().clone();
        last.trade_date += chrono::Duration::days(1);
        last.close *= 1.8;
        last.open = last.close;
        last.high = last.close;
        last.low = last.close;
        bars.push(last);
        let signal = calculate_signal("foo", &bars);
        assert!(signal.score < 45);
        assert!(signal
            .reasons
            .iter()
            .any(|reason| reason.contains("过热") || reason.contains("偏高")));
    }

    fn make_bars(start: f64, step: f64, len: usize) -> Vec<DailyBar> {
        let base = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        (0..len)
            .map(|idx| {
                let close = start + step * idx as f64;
                DailyBar {
                    instrument_id: "foo".to_string(),
                    trade_date: base + chrono::Duration::days(idx as i64),
                    open: close,
                    high: close + 1.0,
                    low: close - 1.0,
                    close,
                    volume: Some(100.0),
                    amount: None,
                    source: "test".to_string(),
                }
            })
            .collect()
    }
}
