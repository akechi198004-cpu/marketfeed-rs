use crate::config::InstrumentConfig;
use crate::models::{DailyBar, ProviderErrorRecord, Signal, SignalAction};
use crate::providers::ProviderErrorKind;
use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database directory {}", parent.display())
            })?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open sqlite database {}", path.display()))?;
        Ok(Self { conn })
    }

    pub fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(SCHEMA)?;
        self.apply_light_migrations()?;
        Ok(())
    }

    fn apply_light_migrations(&self) -> Result<()> {
        self.try_add_column("signals", "ma20", "ma20 REAL")?;
        self.try_add_column("signals", "ma60", "ma60 REAL")?;
        self.try_add_column("signals", "ma120", "ma120 REAL")?;
        self.try_add_column("signals", "deviation_ma60_pct", "deviation_ma60_pct REAL")?;
        self.try_add_column("signals", "deviation_ma120_pct", "deviation_ma120_pct REAL")?;
        self.try_add_column("signals", "change_60d_pct", "change_60d_pct REAL")?;
        self.try_add_column("signals", "drawdown_120d_pct", "drawdown_120d_pct REAL")?;
        self.try_add_column("provider_errors", "error_start", "error_start TEXT")?;
        self.try_add_column("provider_errors", "error_end", "error_end TEXT")?;
        self.try_add_column(
            "provider_errors",
            "kind",
            "kind TEXT NOT NULL DEFAULT 'provider_message'",
        )?;
        self.try_add_column(
            "provider_errors",
            "raw_message",
            "raw_message TEXT NOT NULL DEFAULT ''",
        )?;
        self.conn.execute(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_bars_unique_instrument_date ON daily_bars(instrument_id, trade_date)",
            [],
        )?;
        Ok(())
    }

    fn try_add_column(&self, table: &str, column: &str, ddl: &str) -> Result<()> {
        let mut stmt = self.conn.prepare(&format!("PRAGMA table_info({table})"))?;
        let columns = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if !columns.iter().any(|existing| existing == column) {
            self.conn
                .execute(&format!("ALTER TABLE {table} ADD COLUMN {ddl}"), [])?;
        }
        Ok(())
    }

    pub fn upsert_instruments(&self, instruments: &[InstrumentConfig]) -> Result<usize> {
        let mut written = 0;
        for instrument in instruments {
            self.conn.execute(
                r#"
                INSERT INTO instruments (
                    id, name, kind, market, currency, timezone,
                    stooq_symbol, alpha_vantage_symbol, eastmoney_secid,
                    daily_provider, history_provider, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    kind = excluded.kind,
                    market = excluded.market,
                    currency = excluded.currency,
                    timezone = excluded.timezone,
                    stooq_symbol = excluded.stooq_symbol,
                    alpha_vantage_symbol = excluded.alpha_vantage_symbol,
                    eastmoney_secid = excluded.eastmoney_secid,
                    daily_provider = excluded.daily_provider,
                    history_provider = excluded.history_provider,
                    updated_at = excluded.updated_at
                "#,
                params![
                    instrument.id,
                    instrument.name,
                    format!("{:?}", instrument.kind).to_lowercase(),
                    instrument.market,
                    instrument.currency,
                    instrument.timezone,
                    instrument.stooq_symbol,
                    instrument.alpha_vantage_symbol,
                    instrument.eastmoney_secid,
                    instrument.provider,
                    instrument.provider,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            written += 1;
        }
        Ok(written)
    }

    pub fn count_daily_bars(&self, instrument_id: &str) -> Result<i64> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(1) FROM daily_bars WHERE instrument_id = ?1",
            params![instrument_id],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    pub fn upsert_daily_bars(&self, bars: &[DailyBar]) -> Result<usize> {
        let mut written = 0;
        for bar in bars {
            self.conn.execute(
                r#"
                INSERT INTO daily_bars (
                    instrument_id, trade_date, open, high, low, close,
                    volume, amount, source, fetched_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                ON CONFLICT(instrument_id, trade_date) DO UPDATE SET
                    open = excluded.open,
                    high = excluded.high,
                    low = excluded.low,
                    close = excluded.close,
                    volume = excluded.volume,
                    amount = excluded.amount,
                    source = excluded.source,
                    fetched_at = excluded.fetched_at
                "#,
                params![
                    bar.instrument_id,
                    bar.trade_date.to_string(),
                    bar.open,
                    bar.high,
                    bar.low,
                    bar.close,
                    bar.volume,
                    bar.amount,
                    bar.source,
                    Utc::now().to_rfc3339(),
                ],
            )?;
            written += 1;
        }
        Ok(written)
    }

    pub fn insert_provider_error(
        &self,
        provider: &str,
        instrument_id: &str,
        range_start: Option<NaiveDate>,
        range_end: Option<NaiveDate>,
        kind: ProviderErrorKind,
        raw_message: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO provider_errors (
                provider, instrument_id, error_start, error_end, kind, raw_message, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                provider,
                instrument_id,
                range_start.map(|date| date.to_string()),
                range_end.map(|date| date.to_string()),
                kind.as_str(),
                raw_message,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn daily_bars_in_range(
        &self,
        instrument_id: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyBar>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT instrument_id, trade_date, open, high, low, close, volume, amount, source
            FROM daily_bars
            WHERE instrument_id = ?1 AND trade_date >= ?2 AND trade_date <= ?3
            ORDER BY trade_date ASC
            "#,
        )?;
        let rows = stmt.query_map(
            params![instrument_id, start.to_string(), end.to_string()],
            daily_bar_from_row,
        )?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn earliest_bar_date(&self, instrument_id: &str) -> Result<Option<NaiveDate>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT MIN(trade_date) FROM daily_bars WHERE instrument_id = ?1",
                params![instrument_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        value
            .map(|date| parse_date_for_sqlite(&date, 0).map_err(Into::into))
            .transpose()
    }

    pub fn latest_bar_date(&self, instrument_id: &str) -> Result<Option<NaiveDate>> {
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT MAX(trade_date) FROM daily_bars WHERE instrument_id = ?1",
                params![instrument_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        value
            .map(|date| parse_date_for_sqlite(&date, 0).map_err(Into::into))
            .transpose()
    }

    pub fn provider_errors_since(&self, date: NaiveDate) -> Result<Vec<ProviderErrorRecord>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT provider, instrument_id, kind, raw_message, created_at
            FROM provider_errors
            WHERE created_at >= ?1
            ORDER BY created_at DESC
            LIMIT 100
            "#,
        )?;
        let rows = stmt.query_map(params![format!("{date}T00:00:00Z")], |row| {
            Ok(ProviderErrorRecord {
                provider: row.get(0)?,
                instrument_id: row.get(1)?,
                kind: row.get(2)?,
                raw_message: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn latest_bars_for_signal(
        &self,
        instrument_id: &str,
        limit: usize,
    ) -> Result<Vec<DailyBar>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT instrument_id, trade_date, open, high, low, close, volume, amount, source
            FROM daily_bars
            WHERE instrument_id = ?1
            ORDER BY trade_date DESC
            LIMIT ?2
            "#,
        )?;
        let rows = stmt.query_map(params![instrument_id, limit as i64], daily_bar_from_row)?;
        let mut bars = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        bars.reverse();
        Ok(bars)
    }

    pub fn upsert_signal(&self, signal: &Signal) -> Result<()> {
        let reasons = serde_json::to_string(&signal.reasons)?;
        self.conn.execute(
            r#"
            INSERT INTO signals (
                instrument_id, trade_date, action, score, reason, source, close,
                ma20, ma60, ma120, deviation_ma60_pct, deviation_ma120_pct,
                change_60d_pct, drawdown_120d_pct, generated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
            ON CONFLICT(instrument_id, trade_date) DO UPDATE SET
                action = excluded.action,
                score = excluded.score,
                reason = excluded.reason,
                source = excluded.source,
                close = excluded.close,
                ma20 = excluded.ma20,
                ma60 = excluded.ma60,
                ma120 = excluded.ma120,
                deviation_ma60_pct = excluded.deviation_ma60_pct,
                deviation_ma120_pct = excluded.deviation_ma120_pct,
                change_60d_pct = excluded.change_60d_pct,
                drawdown_120d_pct = excluded.drawdown_120d_pct,
                generated_at = excluded.generated_at
            "#,
            params![
                signal.instrument_id,
                signal.trade_date.to_string(),
                signal.action.as_str(),
                signal.score,
                reasons,
                signal.source,
                signal.close,
                signal.ma20,
                signal.ma60,
                signal.ma120,
                signal.deviation_ma60_pct,
                signal.deviation_ma120_pct,
                signal.change_60d_pct,
                signal.drawdown_120d_pct,
                signal.generated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn latest_signal(&self, instrument_id: &str) -> Result<Option<Signal>> {
        self.conn
            .query_row(
                r#"
                SELECT instrument_id, trade_date, action, score, reason, source, close,
                       ma20, ma60, ma120, deviation_ma60_pct, deviation_ma120_pct,
                       change_60d_pct, drawdown_120d_pct, generated_at
                FROM signals
                WHERE instrument_id = ?1
                ORDER BY generated_at DESC, trade_date DESC
                LIMIT 1
                "#,
                params![instrument_id],
                signal_from_row,
            )
            .optional()
            .map_err(Into::into)
    }
}

fn daily_bar_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DailyBar> {
    let trade_date: String = row.get(1)?;
    Ok(DailyBar {
        instrument_id: row.get(0)?,
        trade_date: parse_date_for_sqlite(&trade_date, 1)?,
        open: row.get(2)?,
        high: row.get(3)?,
        low: row.get(4)?,
        close: row.get(5)?,
        volume: row.get(6)?,
        amount: row.get(7)?,
        source: row.get(8)?,
    })
}

fn signal_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Signal> {
    let trade_date: String = row.get(1)?;
    let action: String = row.get(2)?;
    let reason: String = row.get(4)?;
    let generated_at: String = row.get(14)?;
    Ok(Signal {
        instrument_id: row.get(0)?,
        trade_date: parse_date_for_sqlite(&trade_date, 1)?,
        action: parse_action(&action),
        score: row.get(3)?,
        reasons: parse_reasons(&reason),
        source: row.get(5)?,
        close: row.get(6)?,
        ma20: row.get(7)?,
        ma60: row.get(8)?,
        ma120: row.get(9)?,
        deviation_ma60_pct: row.get(10)?,
        deviation_ma120_pct: row.get(11)?,
        change_60d_pct: row.get(12)?,
        drawdown_120d_pct: row.get(13)?,
        generated_at: DateTime::parse_from_rfc3339(&generated_at)
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
    })
}

fn parse_date_for_sqlite(value: &str, column: usize) -> rusqlite::Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(err),
        )
    })
}

fn parse_action(value: &str) -> SignalAction {
    match value {
        "BUY" | "buy" => SignalAction::Buy,
        "SELL" | "sell" => SignalAction::Sell,
        _ => SignalAction::Hold,
    }
}

fn parse_reasons(value: &str) -> Vec<String> {
    serde_json::from_str(value).unwrap_or_else(|_| vec![value.to_string()])
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS instruments (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    market TEXT NOT NULL,
    currency TEXT NOT NULL,
    timezone TEXT NOT NULL,
    stooq_symbol TEXT,
    alpha_vantage_symbol TEXT,
    eastmoney_secid TEXT,
    daily_provider TEXT NOT NULL,
    history_provider TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS daily_bars (
    instrument_id TEXT NOT NULL,
    trade_date TEXT NOT NULL,
    open REAL NOT NULL,
    high REAL NOT NULL,
    low REAL NOT NULL,
    close REAL NOT NULL,
    volume REAL,
    amount REAL,
    source TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    PRIMARY KEY (instrument_id, trade_date),
    FOREIGN KEY (instrument_id) REFERENCES instruments(id)
);

CREATE TABLE IF NOT EXISTS signals (
    instrument_id TEXT NOT NULL,
    trade_date TEXT NOT NULL,
    action TEXT NOT NULL,
    score INTEGER NOT NULL,
    reason TEXT NOT NULL,
    source TEXT NOT NULL,
    close REAL NOT NULL,
    ma20 REAL,
    ma60 REAL,
    ma120 REAL,
    deviation_ma60_pct REAL,
    deviation_ma120_pct REAL,
    change_60d_pct REAL,
    drawdown_120d_pct REAL,
    generated_at TEXT NOT NULL,
    PRIMARY KEY (instrument_id, trade_date),
    FOREIGN KEY (instrument_id) REFERENCES instruments(id)
);

CREATE TABLE IF NOT EXISTS provider_errors (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    provider TEXT NOT NULL,
    instrument_id TEXT NOT NULL,
    error_start TEXT,
    error_end TEXT,
    kind TEXT NOT NULL,
    raw_message TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_daily_bars_instrument_date
    ON daily_bars(instrument_id, trade_date);

CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_bars_unique_instrument_date
    ON daily_bars(instrument_id, trade_date);

CREATE INDEX IF NOT EXISTS idx_provider_errors_instrument
    ON provider_errors(instrument_id, created_at);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daily_bars_are_idempotent_by_instrument_and_date() {
        let path = std::env::temp_dir().join(format!(
            "marketfeed-db-test-{}.sqlite",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db.conn
            .execute(
                "INSERT INTO instruments (id, name, kind, market, currency, timezone, daily_provider, history_provider, updated_at) VALUES ('foo', 'Foo', 'stock', 'US', 'USD', 'UTC', 'stooq', 'stooq', 'now')",
                [],
            )
            .unwrap();
        let bar = DailyBar {
            instrument_id: "foo".to_string(),
            trade_date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 1.0,
            high: 2.0,
            low: 0.5,
            close: 1.5,
            volume: Some(10.0),
            amount: None,
            source: "stooq".to_string(),
        };
        db.upsert_daily_bars(std::slice::from_ref(&bar)).unwrap();
        let mut replacement = bar.clone();
        replacement.close = 1.8;
        replacement.source = "eastmoney".to_string();
        db.upsert_daily_bars(&[replacement]).unwrap();
        assert_eq!(db.count_daily_bars("foo").unwrap(), 1);
        let latest = db.latest_bars_for_signal("foo", 10).unwrap();
        assert_eq!(latest[0].close, 1.8);
        assert_eq!(latest[0].source, "eastmoney");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn repeated_init_keeps_existing_daily_bars() {
        let config = crate::config::Config::from_file("config.example.toml").unwrap();
        let path = std::env::temp_dir().join(format!(
            "marketfeed-init-repeat-{}.sqlite",
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let db = Database::open(&path).unwrap();
        db.init_schema().unwrap();
        db.upsert_instruments(&config.instruments).unwrap();
        db.upsert_daily_bars(&[DailyBar {
            instrument_id: "fund501203".to_string(),
            trade_date: NaiveDate::from_ymd_opt(2024, 1, 2).unwrap(),
            open: 1.0,
            high: 1.0,
            low: 1.0,
            close: 1.0,
            volume: None,
            amount: None,
            source: "test".to_string(),
        }])
        .unwrap();
        db.init_schema().unwrap();
        db.upsert_instruments(&config.instruments).unwrap();
        assert_eq!(db.count_daily_bars("fund501203").unwrap(), 1);
        let _ = std::fs::remove_file(path);
    }
}
