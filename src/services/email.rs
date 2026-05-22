use crate::config::Config;
use crate::db::Database;
use crate::services::report::{build_report, render_html, render_plain_email};
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use lettre::message::header::ContentType;
use lettre::message::{Body, Message, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use tracing::info;

pub async fn send_daily_report_if_enabled(
    config: &Config,
    db: &Database,
    date: NaiveDate,
) -> Result<()> {
    let email = &config.report.email;
    if !email.enabled || !email.send_on_daily {
        return Ok(());
    }
    send_report_email(config, db, date).await
}

pub async fn send_report_email(config: &Config, db: &Database, date: NaiveDate) -> Result<()> {
    let email_cfg = &config.report.email;
    email_cfg.validate()?;

    let report = build_report(config, db, date)?;
    let html = render_html(&report);
    let plain = render_plain_email(&report);

    let subject = email_cfg.subject_for(date);

    let mut builder = Message::builder()
        .from(
            email_cfg
                .from
                .parse()
                .with_context(|| format!("invalid report.email.from: {}", email_cfg.from))?,
        )
        .subject(subject.clone());

    for recipient in email_cfg.recipients() {
        builder = builder.to(recipient
            .parse()
            .with_context(|| format!("invalid report.email.to address: {recipient}"))?);
    }

    let message = builder
        .multipart(
            MultiPart::alternative()
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_PLAIN)
                        .body(Body::new(plain)),
                )
                .singlepart(
                    SinglePart::builder()
                        .header(ContentType::TEXT_HTML)
                        .body(Body::new(html)),
                ),
        )
        .context("failed to build HTML email")?;

    let username = email_cfg.smtp_username().context("missing SMTP username")?;
    let password = email_cfg.smtp_password().context("missing SMTP password")?;

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::relay(&email_cfg.smtp_host)
        .with_context(|| format!("invalid SMTP relay host {}", email_cfg.smtp_host))?
        .port(email_cfg.smtp_port)
        .credentials(Credentials::new(username, password))
        .build();

    if !email_cfg.use_tls {
        bail!("report.email.use_tls=false is not supported; please use STARTTLS on port 587");
    }

    mailer
        .send(message)
        .await
        .context("failed to send report email")?;

    info!(
        subject = %subject,
        recipients = email_cfg.recipients().join(","),
        report_date = %date,
        "report email sent (HTML)"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    #[test]
    fn email_config_in_example_parses() {
        let raw = std::fs::read_to_string("config.example.toml").unwrap();
        let config: Config = toml::from_str(&raw).unwrap();
        assert!(!config.report.email.enabled);
        assert_eq!(config.report.email.smtp_port, 587);
    }
}
