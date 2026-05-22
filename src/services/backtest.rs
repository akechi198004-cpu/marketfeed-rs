use crate::db::Database;
use crate::models::{BacktestResult, DailyBar, SignalAction};
use crate::services::signal::calculate_signal;
use anyhow::{bail, Result};
use chrono::NaiveDate;

const INITIAL_CASH: f64 = 100_000.0;

pub fn backtest(
    db: &Database,
    instrument_id: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Result<BacktestResult> {
    let bars = db.daily_bars_in_range(instrument_id, start, end)?;
    backtest_bars(&bars, start, end)
}

pub(crate) fn backtest_bars(
    bars: &[DailyBar],
    start: NaiveDate,
    end: NaiveDate,
) -> Result<BacktestResult> {
    if bars.len() < 2 {
        bail!("backtest requires at least two bars");
    }

    let mut cash = INITIAL_CASH;
    let mut shares = 0.0;
    let mut holding = false;
    let mut trades = 0;
    let mut holding_days = 0;
    let mut peak_equity = INITIAL_CASH;
    let mut max_drawdown_pct = 0.0;
    let mut history = Vec::new();

    for bar in bars {
        history.push(bar.clone());
        let signal = calculate_signal(&bar.instrument_id, history.clone());
        match signal.action {
            SignalAction::Buy if !holding => {
                shares = cash / bar.close;
                cash = 0.0;
                holding = true;
                trades += 1;
            }
            SignalAction::Sell if holding => {
                cash = shares * bar.close;
                shares = 0.0;
                holding = false;
                trades += 1;
            }
            _ => {}
        }

        if holding {
            holding_days += 1;
        }
        let equity = cash + shares * bar.close;
        if equity > peak_equity {
            peak_equity = equity;
        }
        let drawdown = if peak_equity > 0.0 {
            (equity / peak_equity - 1.0) * 100.0
        } else {
            0.0
        };
        if drawdown < max_drawdown_pct {
            max_drawdown_pct = drawdown;
        }
    }

    let last_close = bars.last().expect("checked len").close;
    let first_close = bars.first().expect("checked len").close;
    let final_cash = cash + shares * last_close;
    let total_return_pct = (final_cash / INITIAL_CASH - 1.0) * 100.0;
    let buy_and_hold_return_pct = (last_close / first_close - 1.0) * 100.0;

    Ok(BacktestResult {
        start_date: start,
        end_date: end,
        initial_cash: INITIAL_CASH,
        final_cash,
        total_return_pct,
        max_drawdown_pct,
        trades,
        holding_days,
        buy_and_hold_return_pct,
    })
}

pub fn render_backtest(result: &BacktestResult) -> String {
    format!(
        "Backtest sanity check\nStart date: {}\nEnd date: {}\nInitial cash: {:.2}\nFinal cash: {:.2}\nTotal return: {:.2}%\nMax drawdown: {:.2}%\nTrades: {}\nHolding days: {}\nBuy-and-hold return: {:.2}%\nNote: rule-based sanity check only, not investment advice.",
        result.start_date,
        result.end_date,
        result.initial_cash,
        result.final_cash,
        result.total_return_pct,
        result.max_drawdown_pct,
        result.trades,
        result.holding_days,
        result.buy_and_hold_return_pct,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rising_sequence_returns_positive() {
        let bars = make_bars(100.0, 0.12, 180);
        let result = backtest_bars(&bars, bars[0].trade_date, bars[179].trade_date).unwrap();
        assert!(result.total_return_pct > 0.0);
    }

    #[test]
    fn falling_sequence_controls_loss_vs_buy_hold() {
        let bars = make_bars(100.0, -0.12, 180);
        let result = backtest_bars(&bars, bars[0].trade_date, bars[179].trade_date).unwrap();
        assert!(result.total_return_pct >= result.buy_and_hold_return_pct);
    }

    #[test]
    fn sideways_sequence_is_explainable() {
        let base = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let bars: Vec<_> = (0..180)
            .map(|idx| {
                let close = 100.0 + if idx % 2 == 0 { 1.0 } else { -1.0 };
                DailyBar {
                    instrument_id: "foo".to_string(),
                    trade_date: base + chrono::Duration::days(idx as i64),
                    open: close,
                    high: close + 1.0,
                    low: close - 1.0,
                    close,
                    volume: None,
                    amount: None,
                    source: "test".to_string(),
                }
            })
            .collect();
        let result = backtest_bars(&bars, bars[0].trade_date, bars[179].trade_date).unwrap();
        assert!(result.max_drawdown_pct <= 0.0);
        assert!(result.final_cash.is_finite());
        assert!(result.trades <= 2);
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
                    volume: None,
                    amount: None,
                    source: "test".to_string(),
                }
            })
            .collect()
    }
}
