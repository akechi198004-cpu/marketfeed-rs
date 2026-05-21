use chrono::{Duration, NaiveDate, Utc};

pub fn today_utc() -> NaiveDate {
    Utc::now().date_naive()
}

/// A 股 / 基金等国内市场：增量更新默认截止到昨日，避免盘中请求当日 K 线失败。
pub fn incremental_end_date(market: &str) -> NaiveDate {
    let today = today_utc();
    if is_cn_market(market) {
        today - Duration::days(1)
    } else {
        today
    }
}

pub fn is_cn_market(market: &str) -> bool {
    matches!(
        market.trim().to_ascii_uppercase().as_str(),
        "SH" | "SZ" | "CN" | "SSE" | "SZSE" | "CN-SH" | "CN-SZ" | "FUND"
    )
}

pub fn yyyymmdd(date: NaiveDate) -> String {
    date.format("%Y%m%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cn_market_ends_yesterday() {
        let end = incremental_end_date("SH");
        assert_eq!(end, today_utc() - Duration::days(1));
    }

    #[test]
    fn global_market_ends_today() {
        let end = incremental_end_date("GLOBAL");
        assert_eq!(end, today_utc());
    }
}
