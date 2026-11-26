use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::header::ContentType,
    transport::smtp::authentication::Credentials,
};

/// Global SMTP (SMTP_* env) is configured: ApplicationMailer#smtp_config_set_or_development?.
pub fn configured() -> bool {
    std::env::var("SMTP_ADDRESS").is_ok_and(|h| !h.trim().is_empty())
}

/// Send an HTML email through the global SMTP settings. No-op with a warning when SMTP is unset.
pub async fn send_html(to: &str, subject: &str, html: String) -> anyhow::Result<()> {
    let Some(host) = std::env::var("SMTP_ADDRESS")
        .ok()
        .filter(|h| !h.trim().is_empty())
    else {
        tracing::warn!(to, subject, "SMTP_ADDRESS not set; email not sent");
        return Ok(());
    };
    let sender =
        std::env::var("MAILER_SENDER_EMAIL").unwrap_or_else(|_| "noreply@localhost".into());
    let message = Message::builder()
        .from(sender.parse()?)
        .to(to.parse()?)
        .subject(subject)
        .header(ContentType::TEXT_HTML)
        .body(html)?;
    transport(&host)?.send(message).await?;
    Ok(())
}

fn transport(host: &str) -> anyhow::Result<AsyncSmtpTransport<Tokio1Executor>> {
    // config/initializers/mailer.rb: SMTP_SSL/SMTP_TLS = implicit TLS; SMTP_ENABLE_STARTTLS_AUTO=false = plain
    // connection (internal relays). Otherwise STARTTLS, which lettre requires rather than attempts.
    let implicit = ["SMTP_TLS", "SMTP_SSL"]
        .iter()
        .any(|k| std::env::var(k).is_ok_and(|v| v.eq_ignore_ascii_case("true")));
    let starttls =
        !std::env::var("SMTP_ENABLE_STARTTLS_AUTO").is_ok_and(|v| v.eq_ignore_ascii_case("false"));
    let mut builder = if implicit {
        AsyncSmtpTransport::<Tokio1Executor>::relay(host)?
    } else if starttls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host)
    };
    builder = builder.port(
        std::env::var("SMTP_PORT")
            .unwrap_or_else(|_| "587".into())
            .parse::<u16>()?,
    );
    if let Some(username) = std::env::var("SMTP_USERNAME")
        .ok()
        .filter(|u| !u.is_empty())
    {
        builder = builder.credentials(Credentials::new(
            username,
            std::env::var("SMTP_PASSWORD").unwrap_or_default(),
        ));
    }
    Ok(builder.build())
}

pub async fn reset(email: &str, link: &str) -> anyhow::Result<()> {
    let subject = "Reset password instructions";
    let body = format!(
        "Hello {email}!\n\nSomeone has requested a link to change your password. You can do this through the link below.\n\n{link}\n\nIf you didn't request this, please ignore this email.\nYour password won't change until you access the link above and create a new one."
    );
    send_plain(email, subject, body, "password reset").await
}

pub async fn invite(email: &str, link: &str) -> anyhow::Result<()> {
    let subject = "Invitation instructions";
    let body = format!(
        "Hello {email}!\n\nYou have been invited to Pulsar. You can set your password and activate your access through the link below.\n\n{link}"
    );
    send_plain(email, subject, body, "agent invite").await
}

async fn send_plain(email: &str, subject: &str, body: String, kind: &str) -> anyhow::Result<()> {
    let Some(host) = std::env::var("SMTP_ADDRESS")
        .ok()
        .filter(|h| !h.trim().is_empty())
    else {
        // Body can carry a live reset token: only log it when explicitly asked (local dev/tests).
        if std::env::var("PULSAR_LOG_MAIL").as_deref() == Ok("1") {
            tracing::info!(to = email, subject, body, "{kind} email");
        } else {
            tracing::warn!(to = email, "SMTP_ADDRESS not set; {kind} email not sent");
        }
        return Ok(());
    };
    let sender =
        std::env::var("MAILER_SENDER_EMAIL").unwrap_or_else(|_| "noreply@localhost".into());
    let message = Message::builder()
        .from(sender.parse()?)
        .to(email.parse()?)
        .subject(subject)
        .body(body)?;
    transport(&host)?.send(message).await?;
    Ok(())
}
