pub mod alpha_vantage;
pub mod eastmoney;
pub mod stooq;

use crate::config::InstrumentConfig;
use crate::models::DailyBar;
use anyhow::Result;
use async_trait::async_trait;
use chrono::NaiveDate;
use std::{error::Error, fmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderErrorKind {
    Http,
    RateLimited,
    Auth,
    NoData,
    MalformedResponse,
    UnsupportedInstrument,
    ProviderMessage,
}

impl ProviderErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::RateLimited => "rate_limited",
            Self::Auth => "auth",
            Self::NoData => "no_data",
            Self::MalformedResponse => "malformed_response",
            Self::UnsupportedInstrument => "unsupported_instrument",
            Self::ProviderMessage => "provider_message",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderError {
    pub provider: &'static str,
    pub kind: ProviderErrorKind,
    pub raw_message: String,
}

impl ProviderError {
    pub fn new(
        provider: &'static str,
        kind: ProviderErrorKind,
        raw_message: impl Into<String>,
    ) -> Self {
        Self {
            provider,
            kind,
            raw_message: raw_message.into(),
        }
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {}: {}",
            self.provider,
            self.kind.as_str(),
            self.raw_message
        )
    }
}

impl Error for ProviderError {}

#[async_trait]
pub trait MarketDataProvider {
    fn name(&self) -> &'static str;

    async fn fetch_daily_bars(
        &self,
        instrument: &InstrumentConfig,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<DailyBar>>;
}
