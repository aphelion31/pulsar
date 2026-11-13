use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, transport::smtp::authentication::Credentials};

pub async fn reset(email: &str, link: &str) -> anyhow::Result<()> {
    let subject = "Reset password instructions";
    let body = format!("Hello {email}!\n\nSomeone has requested a link to change your password. You can do this through the link below.\n\n{link}\n\nIf you didn't request this, please ignore this email.\nYour password won't change until you access the link above and create a new one.");
    let Some(host) = std::env::var("SMTP_ADDRESS").ok().filter(|h| !h.trim().is_empty()) else {
        tracing::info!(to = email, subject, body, "password reset email");
        return Ok(());
    };
    let sender = std::env::var("MAILER_SENDER_EMAIL").unwrap_or_else(|_| "noreply@localhost".into());
    let message = Message::builder().from(sender.parse()?).to(email.parse()?).subject(subject).body(body)?;
    let implicit = ["SMTP_TLS", "SMTP_SSL"].iter().any(|k| std::env::var(k).is_ok_and(|v| v.eq_ignore_ascii_case("true")));
    let mut builder = if implicit { AsyncSmtpTransport::<Tokio1Executor>::relay(&host)? } else { AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)? };
    let port = std::env::var("SMTP_PORT").unwrap_or_else(|_| "587".into()).parse::<u16>()?;
    builder = builder.port(port);
    if let Ok(username) = std::env::var("SMTP_USERNAME") {
        builder = builder.credentials(Credentials::new(username, std::env::var("SMTP_PASSWORD").unwrap_or_default()));
    }
    builder.build().send(message).await?;
    Ok(())
}
