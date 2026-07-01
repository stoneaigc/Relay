//! 邮件发送(SMTP)。用于注册验证码。

use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::EmailConfig;

/// 发送注册验证码邮件。
pub async fn send_code(cfg: &EmailConfig, to: &str, code: &str) -> anyhow::Result<()> {
    let from = if cfg.from.is_empty() { &cfg.username } else { &cfg.from };
    let email = Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject("RunAPI 注册验证码")
        .body(format!("您的验证码是 {code},5 分钟内有效。如非本人操作请忽略。"))?;

    // 按端口选择 TLS 方式:465 为隐式 TLS(SSL),587/25 为 STARTTLS。
    let builder = if cfg.smtp_port == 587 || cfg.smtp_port == 25 {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.smtp_host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&cfg.smtp_host)?
    };
    let mailer = builder
        .port(cfg.smtp_port)
        .credentials(Credentials::new(cfg.username.clone(), cfg.password.clone()))
        .build();

    mailer.send(email).await?;
    Ok(())
}
