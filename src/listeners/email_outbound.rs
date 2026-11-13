use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor, message::{Mailbox, header::{InReplyTo, References}}, transport::smtp::authentication::Credentials};
use serde_json::Value;
use sqlx::Row;
use crate::{common::AppState, events::Event};

fn headers(uuid: &str, id: i64, account: i64, domain: &str, incoming: Option<&str>, refs: Option<&Value>) -> (String, String, String) {
    let mid = format!("<conversation/{uuid}/messages/{id}@{domain}>");
    let reply = incoming.filter(|s| !s.is_empty()).map(|s| format!("<{}>", s.trim_matches(|c| c == '<' || c == '>'))).unwrap_or_else(|| format!("<account/{account}/conversation/{uuid}@{domain}>"));
    let mut all = Vec::new();
    if let Some(refs) = refs {
        let values: Vec<&str> = match refs { Value::String(s) => vec![s], Value::Array(a) => a.iter().filter_map(Value::as_str).collect(), _ => vec![] };
        for s in values { for part in s.split_whitespace() { let r = format!("<{}>", part.trim_matches(|c| c == '<' || c == '>')); if !all.contains(&r) { all.push(r); } } }
    }
    if !all.contains(&reply) { all.push(reply.clone()); }
    (mid, reply, all.join(" "))
}

pub async fn handle(st: AppState, ev: Event) {
    if ev.name != "message.created" { return; }
    if let Err(e) = send(&st, &ev).await { tracing::warn!(error = %e, "outbound email failed"); }
}

async fn send(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let Some(id) = ev.data["id"].as_i64() else { return Ok(()); };
    let Some(r) = sqlx::query("SELECT m.id::bigint, m.content, c.uuid::text, c.display_id, c.additional_attributes, i.name AS inbox_name, ce.email, ce.smtp_enabled, ce.smtp_address, ce.smtp_port, ce.smtp_login, ce.smtp_password, ce.smtp_enable_ssl_tls, ce.smtp_enable_starttls_auto, ct.email AS contact_email, u.name AS sender_name, (SELECT content_attributes FROM messages WHERE conversation_id = c.id AND message_type = 0 ORDER BY id DESC LIMIT 1) AS incoming FROM messages m JOIN conversations c ON c.id = m.conversation_id AND c.account_id = m.account_id JOIN inboxes i ON i.id = c.inbox_id AND i.account_id = c.account_id AND i.channel_type = 'Channel::Email' JOIN channel_email ce ON ce.id = i.channel_id AND ce.account_id = i.account_id JOIN contacts ct ON ct.id = c.contact_id AND ct.account_id = c.account_id LEFT JOIN users u ON m.sender_type = 'User' AND u.id = m.sender_id WHERE m.id::bigint = $1 AND m.account_id::bigint = $2 AND m.message_type = 1 AND NOT COALESCE(m.private, false)")
        .bind(id).bind(ev.account_id).fetch_optional(&st.db).await? else { return Ok(()); };
    let channel: String = r.try_get("email")?;
    let recipient: Option<String> = r.try_get("contact_email")?;
    let Some(recipient) = recipient.filter(|s| !s.is_empty()) else { anyhow::bail!("contact has no email"); };
    let enabled: bool = r.try_get::<Option<bool>,_>("smtp_enabled")?.unwrap_or(false);
    let host = if enabled { r.try_get::<Option<String>,_>("smtp_address")?.unwrap_or_default() } else { std::env::var("SMTP_ADDRESS").unwrap_or_default() };
    if host.trim().is_empty() { tracing::warn!(message_id = id, "SMTP not configured; email skipped"); return Ok(()); }
    let ssl = if enabled { r.try_get::<Option<bool>,_>("smtp_enable_ssl_tls")?.unwrap_or(false) } else { ["SMTP_TLS", "SMTP_SSL"].iter().any(|k| std::env::var(k).is_ok_and(|s| s.eq_ignore_ascii_case("true"))) };
    let starttls = if enabled { r.try_get::<Option<bool>,_>("smtp_enable_starttls_auto")?.unwrap_or(true) } else { true };
    let port = if enabled { u16::try_from(r.try_get::<Option<i32>,_>("smtp_port")?.unwrap_or(0))? } else { std::env::var("SMTP_PORT").unwrap_or_else(|_| "587".into()).parse()? };
    let login = if enabled { r.try_get::<Option<String>,_>("smtp_login")?.unwrap_or_default() } else { std::env::var("SMTP_USERNAME").unwrap_or_default() };
    let password = if enabled { crate::ar_encryption::decrypt(&r.try_get::<Option<String>,_>("smtp_password")?.unwrap_or_default()) } else { std::env::var("SMTP_PASSWORD").unwrap_or_default() };
    let uuid: String = r.try_get("uuid")?;
    let domain = std::env::var("MAILER_INBOUND_EMAIL_DOMAIN").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| channel.rsplit('@').next().unwrap_or("localhost").to_owned());
    let incoming: Option<Value> = r.try_get("incoming")?;
    let (mid, reply, refs) = headers(&uuid, id, ev.account_id, &domain, incoming.as_ref().and_then(|v| v["email"]["message_id"].as_str()), incoming.as_ref().and_then(|v| v["email"].get("references")));
    let attrs: Option<Value> = r.try_get("additional_attributes")?;
    let subject = attrs.as_ref().and_then(|v| v["mail_subject"].as_str()).map(|s| format!("Re: {s}")).unwrap_or_else(|| format!("[#{}] New message", r.get::<i32,_>("display_id")));
    let inbox: String = r.try_get("inbox_name")?;
    let sender = r.try_get::<Option<String>,_>("sender_name")?.unwrap_or_else(|| "Notifications".into());
    let from = Mailbox::new(Some(format!("{sender} from {inbox}")), channel.parse()?);
    let message = Message::builder().from(from).to(recipient.parse()?).subject(subject).message_id(Some(mid.clone())).header(InReplyTo::from(reply)).header(References::from(refs)).body(r.try_get::<Option<String>,_>("content")?.unwrap_or_default())?;
    let mut builder = if ssl { AsyncSmtpTransport::<Tokio1Executor>::relay(&host)? } else if starttls { AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)? } else { AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host) }.port(port);
    if !login.is_empty() { builder = builder.credentials(Credentials::new(login, password)); }
    builder.build().send(message).await?;
    sqlx::query("UPDATE messages SET source_id = $1, updated_at = now() WHERE id::bigint = $2 AND account_id::bigint = $3").bind(mid.trim_matches(|c| c == '<' || c == '>')).bind(id).bind(ev.account_id).execute(&st.db).await?;
    Ok(())
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn threading_headers() {
        let (id, reply, refs) = headers("abc", 42, 1, "example.com", Some("<in@example.com>"), Some(&serde_json::json!(["old@example.com", "<in@example.com>"])));
        assert_eq!(id, "<conversation/abc/messages/42@example.com>");
        assert_eq!(reply, "<in@example.com>");
        assert_eq!(refs, "<old@example.com> <in@example.com>");
        assert_eq!(headers("abc", 42, 1, "example.com", None, None).1, "<account/1/conversation/abc@example.com>");
    }
}
