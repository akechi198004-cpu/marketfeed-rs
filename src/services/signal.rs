use crate::config::Config;
use crate::db::Database;
use crate::models::{DailyBar, Signal, SignalAction};
use anyhow::Result;
use chrono::Utc;
use tracing::info;

pub fn calculate_and_store(config: &Config, db: &Database) -> Result<()> {
    for instrument in &config.instruments {
        let bars = db.latest_bars_for_signal(&instrument.id, 80)?;
        let signal = calculate_signal(&instrument.id, bars);
        db.upsert_signal(&signal)?;
        info!(instrument_id = %instrument.id, action = signal.action.as_str(), "stored signal");
    }
    Ok(())
}

pub(crate) fn calculate_signal(instrument_id: &str, bars: Vec<DailyBar>) -> Signal {
    let fallback_date = bars
        .last()
        .map(|bar| bar.trade_date)
        .unwrap_or_else(|| Utc::now().date_naive());
    let latest = bars.last();

    if bars.len() < 60 {
        return Signal {
            instrument_id: instrument_id.to_string(),
            trade_date: fallback_date,
            action: SignalAction::Hold,
            score: 0,
            reasons: vec![format!(
                "insufficient_history: 历史日线不足 ma60 计算要求，目前仅有 {} 条。",
                bars.len()
            )],
            source: latest
                .map(|bar| bar.source.clone())
                .unwrap_or_else(|| "none".to_string()),
            close: latest.map(|bar| bar.close).unwrap_or_default(),
            ma5: None,
            ma20: None,
            ma60: None,
            deviation_ma20_pct: None,
            change_20d_pct: None,
            generated_at: Utc::now(),
        };
    }

    let latest = latest.expect("checked len").clone();
    let ma5 = ma(&bars, 5);
    let ma20 = ma(&bars, 20);
    let ma60 = ma(&bars, 60);
    let deviation_ma20_pct = (latest.close / ma20 - 1.0) * 100.0;
    let base_20 = bars[bars.len() - 20].close;
    let change_20d_pct = (latest.close / base_20 - 1.0) * 100.0;

    let mut score = 0;
    let mut reasons = Vec::new();

    if latest.close > ma20 && ma5 > ma20 {
        score += 45;
        reasons.push("close > ma20 且 ma5 > ma20，短中期趋势偏多。".to_string());
    } else if latest.close < ma20 && ma5 < ma20 {
        score -= 45;
        reasons.push("close < ma20 且 ma5 < ma20，短中期趋势偏空。".to_string());
    } else {
        reasons.push("价格与 ma20 / ma5 关系不一致，趋势信号中性。".to_string());
    }

    if change_20d_pct > 8.0 {
        score += 15;
        reasons.push(format!("近 20 日涨幅 {:.2}%，动量偏强。", change_20d_pct));
    } else if change_20d_pct < -8.0 {
        score -= 15;
        reasons.push(format!(
            "近 20 日跌幅 {:.2}%，动量偏弱。",
            change_20d_pct.abs()
        ));
    } else {
        reasons.push(format!("近 20 日涨跌幅 {:.2}%，动量温和。", change_20d_pct));
    }

    if deviation_ma20_pct > 12.0 {
        score -= 20;
        reasons.push(format!(
            "close 高于 ma20 {:.2}%，降低追高分数。",
            deviation_ma20_pct
        ));
    } else if deviation_ma20_pct < -12.0 {
        score += 20;
        reasons.push(format!(
            "close 低于 ma20 {:.2}%，降低杀跌倾向。",
            deviation_ma20_pct.abs()
        ));
    }

    let score = score.clamp(-100, 100);
    let action = if score >= 30 {
        SignalAction::Buy
    } else if score <= -30 {
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
        ma5: Some(ma5),
        ma20: Some(ma20),
        ma60: Some(ma60),
        deviation_ma20_pct: Some(deviation_ma20_pct),
        change_20d_pct: Some(change_20d_pct),
        generated_at: Utc::now(),
    }
}

fn ma(bars: &[DailyBar], n: usize) -> f64 {
    let start = bars.len().saturating_sub(n);
    let slice = &bars[start..];
    slice.iter().map(|bar| bar.close).sum::<f64>() / slice.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn rising_trend_gets_positive_score() {
        let bars = make_bars(1.0, 1.0, 80);
        let signal = calculate_signal("foo", bars);
        assert_eq!(signal.action, SignalAction::Buy);
        assert!(signal.score > 0);
    }

    #[test]
    fn falling_trend_gets_negative_score() {
        let bars = make_bars(100.0, -1.0, 80);
        let signal = calculate_signal("foo", bars);
        assert_eq!(signal.action, SignalAction::Sell);
        assert!(signal.score < 0);
    }

    #[test]
    fn insufficient_history_holds() {
        let bars = make_bars(1.0, 1.0, 20);
        let signal = calculate_signal("foo", bars);
        assert_eq!(signal.action, SignalAction::Hold);
        assert_eq!(signal.score, 0);
        assert!(signal.reasons[0].contains("insufficient_history"));
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
