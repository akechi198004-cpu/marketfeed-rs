use crate::models::{Signal, SignalAction};

/// 从最近交易日信号（trade_date 降序）统计与最新一日相同的连续天数。
pub fn consecutive_action_streak(recent: &[Signal]) -> (usize, Option<SignalAction>) {
    let first = match recent.first() {
        Some(s) => s,
        None => return (0, None),
    };
    let action = first.action;
    let mut days = 0usize;
    for signal in recent {
        if signal.action == action {
            days += 1;
        } else {
            break;
        }
    }
    (days, Some(action))
}

pub fn streak_label_zh(action: SignalAction, days: usize) -> String {
    if days == 0 {
        return "-".to_string();
    }
    let name = match action {
        SignalAction::Buy => "买入",
        SignalAction::Sell => "卖出",
        SignalAction::Hold => "观望",
    };
    format!("连续{days}日{name}")
}

/// 连续 ≥2 日买入/卖出时附加操作提示（邮件辅助阅读，非交易指令）。
pub fn streak_hint_zh(action: SignalAction, days: usize) -> Option<String> {
    match (action, days) {
        (SignalAction::Sell, n) if n >= 2 => Some(format!(
            "已连续 {n} 个交易日为卖出，可考虑分批撤退（建议结合 ≥3 日再加大减仓力度）。"
        )),
        (SignalAction::Buy, n) if n >= 2 => Some(format!(
            "已连续 {n} 个交易日为买入，可考虑分批进入或加仓。"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sig(day: i64, action: SignalAction) -> Signal {
        Signal {
            instrument_id: "x".to_string(),
            trade_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap() + chrono::Duration::days(day),
            action,
            score: 0,
            reasons: vec![],
            source: "t".to_string(),
            close: 1.0,
            ma20: None,
            ma60: None,
            ma120: None,
            deviation_ma60_pct: None,
            deviation_ma120_pct: None,
            change_60d_pct: None,
            drawdown_120d_pct: None,
            generated_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn counts_consecutive_sell_days() {
        let recent = vec![
            sig(5, SignalAction::Sell),
            sig(4, SignalAction::Sell),
            sig(3, SignalAction::Hold),
        ];
        let (days, action) = consecutive_action_streak(&recent);
        assert_eq!(days, 2);
        assert_eq!(action, Some(SignalAction::Sell));
        assert_eq!(streak_label_zh(SignalAction::Sell, 2), "连续2日卖出");
    }
}
