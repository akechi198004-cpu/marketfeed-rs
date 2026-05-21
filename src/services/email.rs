use crate::config::Config;
use anyhow::{bail, Context, Result};
use chrono::NaiveDate;
use lettre::message::header::ContentType;
use lettre::message::{Attachment, Body, Message, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};
use std::path::Path;
use tracing::info;

pub async fn send_daily_report_if_enabled(
    config: &Config,
    report_path: &str,
    date: NaiveDate,
) -> Result<()> {
    let email = &config.report.email;
    if !email.enabled || !email.send_on_daily {
        return Ok(());
    }
    send_report_email(config, report_path, date).await
}

pub async fn send_report_email(config: &Config, report_path: &str, date: NaiveDate) -> Result<()> {
    let email_cfg = &config.report.email;
    email_cfg.validate()?;

    let markdown = std::fs::read_to_string(report_path)
        .with_context(|| format!("failed to read report file {report_path}"))?;
    let filename = Path::new(report_path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("report.md");

    let subject = email_cfg.subject_for(date);
    let intro = format!(
        "市场行情日报已生成。\n报告日期：{date}\n附件为 Markdown 报告，也可用纯文本正文查看。\n"
    );

    let mut builder = Message::builder()
        .from(
            email_cfg
                .from
                .parse()
                .with_context(|| format!("invalid report.email.from: {}", email_cfg.from))?,
        )
        .subject(subject.clone());

    for recipient in email_cfg.recipients() {
        builder = builder.to(
            recipient
                .parse()
                .with_context(|| format!("invalid report.email.to address: {recipient}"))?,
        );
    }

    let message = if email_cfg.attach_markdown {
        let attachment =
            Attachment::new(filename.to_string()).body(markdown.clone(), ContentType::TEXT_PLAIN);
        builder
            .multipart(
                MultiPart::mixed()
                    .singlepart(
                        SinglePart::builder()
                            .header(ContentType::TEXT_PLAIN)
                            .body(Body::new(intro)),
                    )
                    .singlepart(attachment),
            )
            .context("failed to build multipart email")?
    } else {
        builder
            .singlepart(
                SinglePart::builder()
                    .header(ContentType::TEXT_PLAIN)
                    .body(Body::new(format!("{intro}\n\n{markdown}"))),
            )
            .context("failed to build plain email")?
    };

    let username = email_cfg
        .smtp_username()
        .context("missing SMTP username")?;
    let password = email_cfg
        .smtp_password()
        .context("missing SMTP password")?;

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
        report_path,
        "report email sent"
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
