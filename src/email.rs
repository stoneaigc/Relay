//! 邮件发送(SMTP)。用于注册验证码。

use lettre::message::header::ContentType;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

use crate::config::EmailConfig;

/// 发送注册验证码邮件(HTML,验证码加粗突出,中文符号)。
pub async fn send_code(cfg: &EmailConfig, to: &str, code: &str) -> anyhow::Result<()> {
    let from = if cfg.from.is_empty() { &cfg.username } else { &cfg.from };
    let html = format!(
        r#"<div style="font-family:-apple-system,Segoe UI,Microsoft YaHei,sans-serif;font-size:15px;line-height:1.8;color:#1f2329;">
<p style="margin:0 0 12px;">尊敬的用户：</p>
<p style="margin:0 0 12px;">您的注册验证码为 <strong style="font-size:20px;letter-spacing:2px;color:#2563eb;">{code}</strong>，5 分钟内有效。</p>
<p style="margin:0 0 12px;">如非本人操作，请忽略此邮件。</p>
<p style="margin:12px 0 0;color:#8a8f99;font-size:13px;">—— RunAPI</p>
</div>"#
    );
    let email = Message::builder()
        .from(from.parse()?)
        .to(to.parse()?)
        .subject("RunAPI 注册验证码")
        .header(ContentType::TEXT_HTML)
        .body(html)?;

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
