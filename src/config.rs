use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub database: DatabaseConfig,
    #[serde(default)]
    pub report: ReportConfig,
    #[serde(default)]
    pub providers: ProvidersConfig,
    #[serde(default)]
    pub update: UpdateConfig,
    pub instruments: Vec<InstrumentConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReportConfig {
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    #[serde(default = "default_report_format")]
    pub format: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default)]
    pub json_output: Option<String>,
    #[serde(default)]
    pub markdown_output: Option<String>,
    #[serde(default)]
    pub email: EmailConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    #[serde(default = "default_true")]
    pub use_tls: bool,
    #[serde(default)]
    pub username: String,
    #[serde(default = "default_smtp_username_env")]
    pub username_env: String,
    #[serde(default)]
    pub password: String,
    #[serde(default = "default_smtp_password_env")]
    pub password_env: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default = "default_email_subject")]
    pub subject: String,
    /// run-daily 生成报告后自动发送
    #[serde(default = "default_true")]
    pub send_on_daily: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProvidersConfig {
    #[serde(default = "default_true")]
    pub stooq_enabled: bool,
    #[serde(default = "default_true")]
    pub alpha_vantage_enabled: bool,
    #[serde(default = "default_true")]
    pub eastmoney_enabled: bool,
    #[serde(default)]
    pub stooq: StooqConfig,
    #[serde(default)]
    pub alpha_vantage: AlphaVantageConfig,
    #[serde(default)]
    pub eastmoney: EastmoneyConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StooqConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_stooq_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_stooq_base_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlphaVantageConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_alpha_api_key_env")]
    pub api_key_env: String,
    #[serde(default = "default_alpha_base_url")]
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EastmoneyConfig {
    #[serde(default = "default_eastmoney_base_url")]
    pub base_url: String,
    #[serde(default = "default_eastmoney_fund_base_url")]
    pub fund_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_lookback_days")]
    pub default_lookback_days: i64,
    #[serde(default = "default_max_bootstrap_days")]
    pub max_bootstrap_days: i64,
    #[serde(default = "default_retry_count")]
    pub retry_count: usize,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InstrumentConfig {
    pub id: String,
    pub name: String,
    pub kind: InstrumentKind,
    pub market: String,
    #[serde(default = "default_currency")]
    pub currency: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
    pub provider: String,
    #[serde(default, alias = "alpha_symbol", alias = "alpha_vantage_symbol")]
    pub alpha_vantage_symbol: Option<String>,
    #[serde(default)]
    pub stooq_symbol: Option<String>,
    #[serde(default)]
    pub eastmoney_secid: Option<String>,
    #[serde(default)]
    pub eastmoney_fund_code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstrumentKind {
    Index,
    Stock,
    Commodity,
    Etf,
}

impl Config {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file {}", path.display()))?;
        let config: Self = toml::from_str(&raw)
            .with_context(|| format!("failed to parse config file {}", path.display()))?;
        config.validate()?;
        tracing::info!(path = %path.display(), instruments = config.instruments.len(), "config loaded");
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.database.path.trim().is_empty() {
            bail!("database.path is required");
        }
        if self.instruments.is_empty() {
            bail!("at least one [[instruments]] entry is required");
        }
        if self.report.email.enabled {
            self.report.email.validate()?;
        }
        for instrument in &self.instruments {
            if instrument.id.trim().is_empty() {
                bail!("instrument id is required");
            }
            if instrument.name.trim().is_empty() {
                bail!("instrument {} name is required", instrument.id);
            }
            match instrument.provider.as_str() {
                "stooq" if instrument.stooq_symbol.is_none() => {
                    bail!(
                        "instrument {} uses stooq but stooq_symbol is missing",
                        instrument.id
                    )
                }
                "alpha_vantage" if instrument.alpha_vantage_symbol.is_none() => bail!(
                    "instrument {} uses alpha_vantage but alpha_symbol is missing",
                    instrument.id
                ),
                "eastmoney" => {}
                "stooq" | "alpha_vantage" => {}
                other => bail!(
                    "instrument {} has unsupported provider {other}",
                    instrument.id
                ),
            }
        }
        Ok(())
    }

    pub fn instrument(&self, id: &str) -> Result<&InstrumentConfig> {
        self.instruments
            .iter()
            .find(|instrument| instrument.id == id)
            .ok_or_else(|| anyhow!("instrument {id} not found in config"))
    }

    pub fn report_markdown_path(&self, date: chrono::NaiveDate) -> String {
        self.report
            .markdown_output
            .clone()
            .unwrap_or_else(|| format!("{}/{}.md", self.report.output_dir, date))
    }

    pub fn report_json_path(&self, date: chrono::NaiveDate) -> String {
        self.report
            .json_output
            .clone()
            .unwrap_or_else(|| format!("{}/{}.json", self.report.output_dir, date))
    }

    pub fn alpha_api_key(&self) -> Option<String> {
        non_empty(&self.providers.alpha_vantage.api_key)
            .or_else(|| std::env::var(&self.providers.alpha_vantage.api_key_env).ok())
    }

    pub fn stooq_api_key(&self) -> Option<String> {
        non_empty(&self.providers.stooq.api_key)
            .or_else(|| std::env::var(&self.providers.stooq.api_key_env).ok())
    }
}

impl EmailConfig {
    pub fn validate(&self) -> Result<()> {
        if self.smtp_host.trim().is_empty() {
            bail!("report.email.smtp_host is required when email is enabled");
        }
        if self.from.trim().is_empty() {
            bail!("report.email.from is required when email is enabled");
        }
        if self.to.iter().all(|addr| addr.trim().is_empty()) {
            bail!("report.email.to must contain at least one address when email is enabled");
        }
        if self.smtp_username().is_none() {
            bail!(
                "report.email.username or env {} is required when email is enabled",
                self.username_env
            );
        }
        if self.smtp_password().is_none() {
            bail!(
                "report.email.password or env {} is required when email is enabled",
                self.password_env
            );
        }
        Ok(())
    }

    pub fn smtp_username(&self) -> Option<String> {
        non_empty(&self.username).or_else(|| std::env::var(&self.username_env).ok())
    }

    pub fn smtp_password(&self) -> Option<String> {
        non_empty(&self.password).or_else(|| std::env::var(&self.password_env).ok())
    }

    pub fn subject_for(&self, date: chrono::NaiveDate) -> String {
        self.subject.replace("{date}", &date.to_string())
    }

    pub fn recipients(&self) -> Vec<String> {
        self.to
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }
}

impl InstrumentConfig {
    pub fn daily_provider(&self) -> &str {
        &self.provider
    }

    pub fn history_provider(&self) -> &str {
        &self.provider
    }
}

impl Default for ReportConfig {
    fn default() -> Self {
        Self {
            output_dir: default_output_dir(),
            format: default_report_format(),
            timezone: default_timezone(),
            json_output: None,
            markdown_output: None,
            email: EmailConfig::default(),
        }
    }
}

impl Default for EmailConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            smtp_host: String::new(),
            smtp_port: default_smtp_port(),
            use_tls: true,
            username: String::new(),
            username_env: default_smtp_username_env(),
            password: String::new(),
            password_env: default_smtp_password_env(),
            from: String::new(),
            to: Vec::new(),
            subject: default_email_subject(),
            send_on_daily: true,
        }
    }
}

impl Default for ProvidersConfig {
    fn default() -> Self {
        Self {
            stooq_enabled: true,
            alpha_vantage_enabled: true,
            eastmoney_enabled: true,
            stooq: StooqConfig::default(),
            alpha_vantage: AlphaVantageConfig::default(),
            eastmoney: EastmoneyConfig::default(),
        }
    }
}

impl Default for StooqConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_key_env: default_stooq_api_key_env(),
            base_url: default_stooq_base_url(),
        }
    }
}

impl Default for AlphaVantageConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_key_env: default_alpha_api_key_env(),
            base_url: default_alpha_base_url(),
        }
    }
}

impl Default for EastmoneyConfig {
    fn default() -> Self {
        Self {
            base_url: default_eastmoney_base_url(),
            fund_base_url: default_eastmoney_fund_base_url(),
        }
    }
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            default_lookback_days: default_lookback_days(),
            max_bootstrap_days: default_max_bootstrap_days(),
            retry_count: default_retry_count(),
            retry_delay_ms: default_retry_delay_ms(),
        }
    }
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn default_true() -> bool {
    true
}
fn default_output_dir() -> String {
    "reports".to_string()
}
fn default_report_format() -> String {
    "markdown".to_string()
}
fn default_timezone() -> String {
    "UTC".to_string()
}
fn default_currency() -> String {
    "USD".to_string()
}
fn default_stooq_api_key_env() -> String {
    "STOOQ_API_KEY".to_string()
}
fn default_alpha_api_key_env() -> String {
    "ALPHA_VANTAGE_API_KEY".to_string()
}
fn default_stooq_base_url() -> String {
    "https://stooq.com/q/d/l/".to_string()
}
fn default_alpha_base_url() -> String {
    "https://www.alphavantage.co/query".to_string()
}
fn default_eastmoney_base_url() -> String {
    "https://push2his.eastmoney.com/api/qt/stock/kline/get".to_string()
}
fn default_eastmoney_fund_base_url() -> String {
    "https://fundf10.eastmoney.com/F10DataApi.aspx".to_string()
}
fn default_lookback_days() -> i64 {
    7
}
fn default_max_bootstrap_days() -> i64 {
    5000
}
fn default_retry_count() -> usize {
    2
}
fn default_retry_delay_ms() -> u64 {
    1000
}
fn default_smtp_port() -> u16 {
    587
}
fn default_smtp_username_env() -> String {
    "MARKETFEED_SMTP_USER".to_string()
}
fn default_smtp_password_env() -> String {
    "MARKETFEED_SMTP_PASS".to_string()
}
fn default_email_subject() -> String {
    "市场行情日报 {date}".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_example_parses() {
        let raw = std::fs::read_to_string("config.example.toml").unwrap();
        let config: Config = toml::from_str(&raw).unwrap();
        config.validate().unwrap();
        assert_eq!(config.instruments.len(), 5);
        assert_eq!(config.report.output_dir, "reports");
    }
}
