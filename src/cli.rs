use crate::config::Config;
use crate::db::Database;
use crate::services::{bootstrap, email, provider_errors, report, signal, updater};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Parser)]
#[command(name = "marketfeed-rs")]
#[command(about = "Market data collector and rule signal reporter for OpenClaw")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Init {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
    },
    Bootstrap {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
        #[arg(long)]
        instrument: Option<String>,
        #[arg(long, value_parser = parse_date)]
        from: Option<NaiveDate>,
        #[arg(long)]
        dry_run: bool,
    },
    Update {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
        #[arg(long)]
        instrument: Option<String>,
        #[arg(long, value_parser = parse_date)]
        from: Option<NaiveDate>,
        #[arg(long, value_parser = parse_date)]
        to: Option<NaiveDate>,
        #[arg(long)]
        dry_run: bool,
    },
    Signal {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
        /// 用已有日线回补历史 signals（连续 N 日统计需要）
        #[arg(long)]
        backfill: bool,
        /// 仅回补最近 N 个交易日；默认 120
        #[arg(long)]
        backfill_days: Option<usize>,
        #[arg(long)]
        instrument: Option<String>,
    },
    Report {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
        #[arg(long, value_enum)]
        format: Option<ReportFormat>,
        #[arg(long, value_parser = parse_date)]
        date: Option<NaiveDate>,
    },
    RunDaily {
        #[arg(long, default_value = "config.toml")]
        config: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ReportFormat {
    Json,
    Markdown,
}

pub async fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Init { config } => {
            let (config, db) = load(&config)?;
            init_db(&config, &db)?;
        }
        Commands::Bootstrap {
            config,
            instrument,
            from,
            dry_run,
        } => {
            let (config, db) = load(&config)?;
            if !dry_run {
                init_db(&config, &db)?;
            }
            let summary = bootstrap::bootstrap_with_options(
                &config,
                &db,
                bootstrap::BootstrapOptions {
                    instrument_id: instrument,
                    from,
                    dry_run,
                },
            )
            .await?;
            print_update_summary("Bootstrap", &summary);
            if !dry_run {
                let backfill = signal::backfill_signals(
                    &config,
                    &db,
                    signal::BackfillOptions::default(),
                )?;
                let total: usize = backfill
                    .instruments
                    .iter()
                    .map(|i| i.signals_written)
                    .sum();
                println!("Bootstrap 后已回补历史信号，共写入 {total} 条。");
            }
        }
        Commands::Update {
            config,
            instrument,
            from,
            to,
            dry_run,
        } => {
            let (config, db) = load(&config)?;
            if !dry_run {
                init_db(&config, &db)?;
            }
            let summary = updater::update(
                &config,
                &db,
                updater::UpdateOptions {
                    instrument_id: instrument,
                    from,
                    to,
                    dry_run,
                },
            )
            .await?;
            print_update_summary("Updated", &summary);
        }
        Commands::Signal {
            config,
            backfill,
            backfill_days,
            instrument,
        } => {
            let (config, db) = load(&config)?;
            db.init_schema()?;
            if backfill {
                let summary = signal::backfill_signals(
                    &config,
                    &db,
                    signal::BackfillOptions {
                        instrument_id: instrument,
                        trading_days: backfill_days,
                    },
                )?;
                for item in &summary.instruments {
                    if item.signals_written == 0 {
                        println!(
                            "{}: 跳过（日线 {} 条，需至少 {} 条）",
                            item.instrument_id,
                            item.bars_total,
                            signal::MIN_HISTORY
                        );
                    } else {
                        println!(
                            "{}: 回补 {} 条信号（{} ~ {}，日线共 {} 条）",
                            item.instrument_id,
                            item.signals_written,
                            item.first_trade_date.unwrap(),
                            item.last_trade_date.unwrap(),
                            item.bars_total
                        );
                    }
                }
            }
            signal::calculate_and_store(&config, &db)?;
            println!(
                "已写入最新交易日信号，共 {} 个标的。",
                config.instruments.len()
            );
        }
        Commands::Report {
            config,
            format,
            date,
        } => {
            let (config, db) = load(&config)?;
            db.init_schema()?;
            let date = date.unwrap_or_else(|| chrono::Utc::now().date_naive());
            let selected_format = format.unwrap_or(match config.report.format.as_str() {
                "json" => ReportFormat::Json,
                _ => ReportFormat::Markdown,
            });
            let path = match selected_format {
                ReportFormat::Json => report::write_json_report_for_date(&config, &db, date)?,
                ReportFormat::Markdown => {
                    report::write_markdown_report_for_date(&config, &db, date)?
                }
            };
            println!("Report written: {path}");
        }
        Commands::RunDaily { config, dry_run } => {
            let (config, db) = load(&config)?;
            if dry_run {
                let summary = updater::update(
                    &config,
                    &db,
                    updater::UpdateOptions {
                        instrument_id: None,
                        from: None,
                        to: None,
                        dry_run: true,
                    },
                )
                .await?;
                print_update_summary("Run daily dry-run", &summary);
                println!(
                    "Dry-run: no daily_bars, signals, provider_errors, or reports were written."
                );
                return Ok(());
            }
            init_db(&config, &db)?;
            let summary = updater::update_recent(&config, &db).await?;
            print_update_summary("Updated", &summary);
            signal::calculate_and_store(&config, &db)?;
            // 若此前未跑过 backfill，用已有 K 线补齐近期 signals，避免「连续」只有 1～2 日
            let _ = signal::backfill_signals(&config, &db, signal::BackfillOptions::default())?;
            let date = chrono::Utc::now().date_naive();
            let path = report::write_markdown_report_for_date(&config, &db, date)?;
            if config.report.email.enabled && config.report.email.send_on_daily {
                match email::send_daily_report_if_enabled(&config, &db, date).await {
                    Ok(()) => {
                        println!(
                            "邮件已发送: {}",
                            config.report.email.recipients().join(", ")
                        );
                    }
                    Err(err) => {
                        eprintln!("报告已生成，但邮件发送失败: {err:#}");
                    }
                }
            } else {
                println!("邮件: 未发送（config.toml 中 [report.email] enabled = false）");
            }
            if summary.provider_error_count() > 0 {
                println!("Run daily completed with provider errors. Exit code remains 0 by design; see report: {path}");
            } else {
                println!("Run daily completed successfully. Report: {path}");
            }
        }
    }

    Ok(())
}

fn load(path: &PathBuf) -> Result<(Config, Database)> {
    let config = Config::from_file(path)?;
    let db = Database::open(&config.database.path)?;
    Ok((config, db))
}

fn init_db(config: &Config, db: &Database) -> Result<()> {
    db.init_schema()?;
    let count = db.upsert_instruments(&config.instruments)?;
    info!(database = %config.database.path, instruments = count, "db initialized");
    println!("Initialized database: {}", config.database.path);
    println!("Imported/updated {count} instruments.");
    for instrument in &config.instruments {
        let bars = db.count_daily_bars(&instrument.id)?;
        println!(
            "- {}: {} ({}, provider={}, bars={})",
            instrument.id, instrument.name, instrument.market, instrument.provider, bars
        );
    }
    Ok(())
}

fn print_update_summary(title: &str, summary: &updater::UpdateSummary) {
    println!(
        "{title} {} instruments. Range: {} to {}. Dry-run: {}",
        summary.instruments.len(),
        summary.start,
        summary.end,
        summary.dry_run
    );
    for item in &summary.instruments {
        if let Some(error) = &item.error {
            eprintln!(
                "⚠ {} [{}]: {}",
                item.instrument_id,
                provider_errors::provider_label_zh(&item.provider),
                error
            );
        } else if summary.dry_run {
            println!(
                "{}: would request provider {}",
                item.instrument_id, item.provider
            );
        } else {
            println!(
                "{}: {} bars inserted/replaced",
                item.instrument_id, item.bars_written
            );
        }
    }
}

fn parse_date(value: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .with_context(|| format!("invalid date {value}, expected YYYY-MM-DD"))
}
