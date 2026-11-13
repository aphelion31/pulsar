//! Inbound email for `Channel::Email` inboxes via IMAP polling.
//! Mirrors chatwoot/app/jobs/inboxes/fetch_imap_email_inboxes_job.rb,
//! fetch_imap_emails_job.rb, app/services/imap/base_fetch_email_service.rb,
//! app/services/imap/fetch_email_service.rb and app/mailboxes/imap/imap_mailbox.rb.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context};
use chrono::{Duration as ChronoDuration, Utc};
use futures::StreamExt;
use mail_parser::{MessageParser, TextList};
use serde_json::json;
use sqlx::PgPool;
use tokio::net::TcpStream;
use tokio_rustls::{rustls, TlsConnector};

use crate::common::AppState;
use crate::conversations::{self, Conv};

/// Chatwoot Message enums: message_type incoming = 0, content_type incoming_email = 8.
const MESSAGE_TYPE_INCOMING: i32 = 0;
const CONTENT_TYPE_INCOMING_EMAIL: i32 = 8;
const DEFAULT_INTERVAL_SECS: u64 = 60;
/// Chatwoot FetchEmailService::MAX_MESSAGES_PER_SYNC.
const MAX_MESSAGES_PER_SYNC: usize = 500;

/// Spawn the polling loop. Per-inbox errors are logged and never stop the loop.
pub fn spawn(st: AppState) {
    let interval = std::env::var("PULSAR_IMAP_INTERVAL_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_INTERVAL_SECS);
    tokio::spawn(async move {
        loop {
            if let Err(e) = fetch_all(&st).await {
                tracing::error!("[IMAP] inbox scan failed: {e:#}");
            }
            tokio::time::sleep(Duration::from_secs(interval)).await;
        }
    });
}

/// Credentials are kept in this struct only. Never format it into a log line.
#[derive(sqlx::FromRow)]
struct EmailInbox {
    inbox_id: i32,
    account_id: i32,
    imap_address: String,
    imap_port: i32,
    imap_login: String,
    imap_password: String,
    imap_enable_ssl: bool,
}

async fn fetch_all(st: &AppState) -> anyhow::Result<()> {
    // Mirrors FetchImapEmailInboxesJob#should_fetch_emails?: skip suspended accounts and imap-disabled channels.
    let rows: Vec<EmailInbox> = sqlx::query_as(
        "SELECT i.id AS inbox_id, i.account_id, ce.imap_address, ce.imap_port, ce.imap_login,
                ce.imap_password, ce.imap_enable_ssl
         FROM inboxes i
         JOIN channel_email ce ON ce.id = i.channel_id
         JOIN accounts a ON a.id = i.account_id
         WHERE i.channel_type = 'Channel::Email'
           AND ce.imap_enabled = true
           AND COALESCE(a.status, 0) <> 1",
    )
    .fetch_all(&st.db)
    .await?;
    for ch in rows {
        let inbox_id = ch.inbox_id;
        if let Err(e) = fetch_inbox(st, ch).await {
            tracing::error!("[IMAP] inbox {inbox_id} fetch failed: {e:#}");
        }
    }
    Ok(())
}

async fn fetch_inbox(st: &AppState, ch: EmailInbox) -> anyhow::Result<()> {
    let raws = fetch_raw_messages(&ch).await?;
    for raw in raws {
        if let Err(e) = ingest_raw(st, ch.account_id as i64, ch.inbox_id as i64, &raw).await {
            tracing::error!("[IMAP] inbox {} ingest failed: {e:#}", ch.inbox_id);
        }
    }
    Ok(())
}

/// Connect (TLS when imap_enable_ssl), log in, SEARCH SINCE yesterday, return raw bodies not yet stored.
async fn fetch_raw_messages(ch: &EmailInbox) -> anyhow::Result<Vec<Vec<u8>>> {
    let tcp = TcpStream::connect((ch.imap_address.as_str(), ch.imap_port as u16))
        .await
        .with_context(|| format!("connect {}:{}", ch.imap_address, ch.imap_port))?;
    if ch.imap_enable_ssl {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = rustls::ClientConfig::builder().with_root_certificates(roots).with_no_client_auth();
        let server = rustls::pki_types::ServerName::try_from(ch.imap_address.clone()).map_err(|e| anyhow!("bad imap host: {e}"))?;
        let tls = TlsConnector::from(Arc::new(cfg)).connect(server, tcp).await.context("tls handshake")?;
        session_fetch(async_imap::Client::new(tls), ch).await
    } else {
        session_fetch(async_imap::Client::new(tcp), ch).await
    }
}

async fn session_fetch<T>(client: async_imap::Client<T>, ch: &EmailInbox) -> anyhow::Result<Vec<Vec<u8>>>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    // Plain LOGIN only. Chatwoot also allows cram-md5; add when an inbox needs it.
    // ponytail: cram-md5 not wired, add when a provider requires it.
    let mut session = client
        .login(&ch.imap_login, &ch.imap_password)
        .await
        .map_err(|(e, _)| anyhow!("imap login failed: {e}"))?;
    session.select("INBOX").await.context("select INBOX")?;
    let since = (Utc::now() - ChronoDuration::days(1)).format("%d-%b-%Y").to_string();
    let seqs = session.search(format!("SINCE {since}")).await.context("imap search")?;
    let mut seq_list: Vec<u32> = seqs.into_iter().collect();
    seq_list.sort_unstable();
    seq_list.truncate(MAX_MESSAGES_PER_SYNC);

    let mut out = Vec::new();
    for seq in seq_list {
        // BODY.PEEK[] keeps the message unread, as Chatwoot does.
        let mut stream = session.fetch(seq.to_string(), "BODY.PEEK[]").await.context("imap fetch")?;
        while let Some(item) = stream.next().await {
            let fetch = item.context("imap fetch item")?;
            if let Some(body) = fetch.body() {
                out.push(body.to_vec());
            }
        }
    }
    let _ = session.logout().await;
    Ok(out)
}

/// Parsed view of an inbound mail. Pure data, no IO.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct ParsedMail {
    pub message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from_email: Option<String>,
    pub from_name: Option<String>,
    pub subject: Option<String>,
    pub to: String,
    pub cc: String,
    pub date: Option<String>,
    pub text_full: String,
    pub text_reply: String,
    pub html_full: Option<String>,
}

/// Trim whitespace and angle brackets from one Message-ID token.
pub(crate) fn clean_message_id(s: &str) -> String {
    s.trim().trim_start_matches('<').trim_end_matches('>').trim().to_string()
}

/// Drop the quoted reply: lines from the first `>` quote or `On ... wrote:` intro, and a `-- ` signature delimiter.
pub(crate) fn strip_quoted_reply(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let lower = t.to_ascii_lowercase();
        if t.starts_with('>') || (lower.starts_with("on ") && lower.ends_with("wrote:")) || t == "--" || t == "-- " {
            break;
        }
        out.push(line);
    }
    out.join("\n").trim_end().to_string()
}

fn text_list(l: Option<TextList<'_>>) -> Vec<String> {
    l.map(|l| l.iter().map(clean_message_id).collect()).unwrap_or_default()
}

pub(crate) fn parse_mail(raw: &[u8]) -> anyhow::Result<ParsedMail> {
    let msg = MessageParser::default().parse(raw).ok_or_else(|| anyhow!("unparseable email"))?;
    let message_id = msg.message_id().map(clean_message_id).filter(|s| !s.is_empty());
    let in_reply_to = text_list(msg.in_reply_to()).into_iter().next();
    let references = text_list(msg.references());
    let from = msg.from().and_then(|a| a.first());
    let from_email = from.as_ref().and_then(|m| m.address()).map(str::to_lowercase);
    let from_name = from.as_ref().and_then(|m| m.name()).map(str::to_string);
    let addrs = |list: Option<mail_parser::AddressList<'_>>| -> String {
        list.map(|a| a.mailboxes().filter_map(|x| x.address().map(str::to_string)).collect::<Vec<_>>().join(", ")).unwrap_or_default()
    };
    let text_full = msg.body_text(0).map(|b| b.into_owned()).unwrap_or_default();
    let text_reply = strip_quoted_reply(&text_full);
    Ok(ParsedMail {
        message_id,
        in_reply_to,
        references,
        from_email,
        from_name,
        subject: msg.subject().map(str::to_string),
        to: addrs(msg.to()),
        cc: addrs(msg.cc()),
        date: msg.date().map(|d| d.to_rfc3339()),
        text_full,
        text_reply,
        html_full: msg.body_html(0).map(|b| b.into_owned()),
    })
}

/// Parse one raw mail and store it in the inbox. Duplicate Message-IDs are skipped (Chatwoot email_already_present?).
pub(crate) async fn ingest_raw(st: &AppState, account_id: i64, inbox_id: i64, raw: &[u8]) -> anyhow::Result<()> {
    let mail = parse_mail(raw)?;
    let Some(message_id) = mail.message_id.clone() else {
        return Ok(());
    };
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2 LIMIT 1")
        .bind(inbox_id as i32)
        .bind(&message_id)
        .fetch_optional(&st.db)
        .await?;
    if exists.is_some() {
        return Ok(());
    }
    let sender = mail.from_email.clone().ok_or_else(|| anyhow!("mail {message_id} has no From address"))?;

    let (contact_id, contact_inbox_id) = find_or_create_contact(&st.db, account_id, inbox_id, &sender, mail.from_name.as_deref()).await?;
    let conv = find_or_create_conversation(&st.db, account_id, inbox_id, contact_id, contact_inbox_id, &mail).await?;

    let content_attributes = json!({
        "email": {
            "subject": mail.subject,
            "from": [sender],
            "to": mail.to,
            "cc": mail.cc,
            "message_id": message_id,
            "in_reply_to": mail.in_reply_to,
            "date": mail.date,
            "text_content": { "full": mail.text_full, "reply": mail.text_reply },
            "html_content": { "full": mail.html_full },
        },
        // Internal carrier: insert_message moves this into messages.source_id.
        "__pulsar_source_id": message_id,
    });
    conversations::insert_message(
        st,
        &conv,
        Some(("Contact", contact_id)),
        &mail.text_reply,
        MESSAGE_TYPE_INCOMING,
        false,
        CONTENT_TYPE_INCOMING_EMAIL,
        content_attributes,
        None,
    )
    .await
    .map_err(|e| anyhow!("insert_message: {e:?}"))?;
    Ok(())
}

/// Contact lookup by lower(email) in the account, then contact_inbox keyed by sender email (Chatwoot from_email + ContactInboxWithContactBuilder).
async fn find_or_create_contact(db: &PgPool, account_id: i64, inbox_id: i64, sender: &str, name: Option<&str>) -> anyhow::Result<(i64, i64)> {
    let existing: Option<(i64,)> = sqlx::query_as("SELECT id::bigint FROM contacts WHERE account_id = $1 AND lower(email) = $2 ORDER BY id LIMIT 1")
        .bind(account_id as i32)
        .bind(sender)
        .fetch_optional(db)
        .await?;
    let contact_id = match existing {
        Some((id,)) => id,
        None => {
            let display = name
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| sender.split('@').next().unwrap_or(sender));
            let (id,): (i64,) = sqlx::query_as(
                "INSERT INTO contacts (name, email, account_id, created_at, updated_at, additional_attributes, last_activity_at)
                 VALUES ($1, $2, $3, now(), now(), '{}', now()) RETURNING id::bigint",
            )
            .bind(display)
            .bind(sender)
            .bind(account_id as i32)
            .fetch_one(db)
            .await?;
            id
        }
    };
    sqlx::query(
        "INSERT INTO contact_inboxes (contact_id, inbox_id, source_id, created_at, updated_at, pubsub_token)
         VALUES ($1, $2, $3, now(), now(), $4)
         ON CONFLICT (inbox_id, source_id) DO NOTHING",
    )
    .bind(contact_id)
    .bind(inbox_id as i32)
    .bind(sender)
    .bind(uuid::Uuid::new_v4().to_string())
    .execute(db)
    .await?;
    let (ci,): (i64,) = sqlx::query_as("SELECT id::bigint FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2")
        .bind(inbox_id as i32)
        .bind(sender)
        .fetch_one(db)
        .await?;
    Ok((contact_id, ci))
}

/// Reply lookup: In-Reply-To / References matching an existing message source_id in this inbox reuses its conversation.
async fn find_or_create_conversation(
    db: &PgPool, account_id: i64, inbox_id: i64, contact_id: i64, contact_inbox_id: i64, mail: &ParsedMail,
) -> anyhow::Result<Conv> {
    let mut refs: Vec<&str> = mail.in_reply_to.iter().map(String::as_str).collect();
    refs.extend(mail.references.iter().map(String::as_str));
    if !refs.is_empty() {
        let found: Option<(i64,)> = sqlx::query_as(
            "SELECT conversation_id::bigint FROM messages
             WHERE inbox_id = $1 AND account_id = $2 AND source_id = ANY($3)
             ORDER BY id DESC LIMIT 1",
        )
        .bind(inbox_id as i32)
        .bind(account_id as i32)
        .bind(&refs)
        .fetch_optional(db)
        .await?;
        if let Some((cid,)) = found {
            return conversations::reload(db, cid).await.map_err(|e| anyhow!("reload conversation: {e:?}"));
        }
    }
    let attrs = json!({
        "source": "email",
        "in_reply_to": mail.in_reply_to,
        "mail_subject": mail.subject,
        "initiated_at": { "timestamp": Utc::now().to_rfc3339() },
    });
    let (cid,): (i64,) = sqlx::query_as(
        "INSERT INTO conversations (account_id, inbox_id, status, contact_id, contact_inbox_id, additional_attributes,
                                    custom_attributes, last_activity_at, created_at, updated_at)
         VALUES ($1, $2, 0, $3, $4, $5, '{}', now(), now(), now()) RETURNING id::bigint",
    )
    .bind(account_id as i32)
    .bind(inbox_id as i32)
    .bind(contact_id)
    .bind(contact_inbox_id)
    .bind(attrs)
    .fetch_one(db)
    .await?;
    conversations::reload(db, cid).await.map_err(|e| anyhow!("reload conversation: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_id_strips_brackets() {
        assert_eq!(clean_message_id(" <abc@x.com> "), "abc@x.com");
    }

    #[test]
    fn parses_ids_and_reply_headers() {
        let raw = b"Message-ID: <m2@x.com>\r\nIn-Reply-To: <m1@x.com>\r\nReferences: <m0@x.com> <m1@x.com>\r\nFrom: Ann <Ann@Example.com>\r\nSubject: Hi\r\n\r\nHello\r\n\r\n> old\r\n";
        let p = parse_mail(raw).unwrap();
        assert_eq!(p.message_id.as_deref(), Some("m2@x.com"));
        assert_eq!(p.in_reply_to.as_deref(), Some("m1@x.com"));
        assert_eq!(p.references, vec!["m0@x.com".to_string(), "m1@x.com".to_string()]);
        assert_eq!(p.from_email.as_deref(), Some("ann@example.com"));
    }

    #[test]
    fn strips_quoted_reply() {
        assert_eq!(strip_quoted_reply("Thanks\n\nOn Mon, a wrote:\n> old"), "Thanks");
        assert_eq!(strip_quoted_reply("Ok\n> q\nmore"), "Ok");
        assert_eq!(strip_quoted_reply("Plain body"), "Plain body");
    }
}

/// Live DB test. Run: `DATABASE_URL=postgres://localhost/pulsar cargo test -- --ignored ingest_live`.
/// Needs an existing email inbox id in PULSAR_TEST_INBOX_ID (account from that inbox). Inserts two mails
/// (new + reply via In-Reply-To), asserts one conversation with two incoming messages, then deletes the rows.
#[cfg(test)]
mod live {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn ingest_live() -> anyhow::Result<()> {
        let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/pulsar".into());
        let inbox_id: i64 = std::env::var("PULSAR_TEST_INBOX_ID")?.parse()?;
        let db = sqlx::postgres::PgPoolOptions::new().max_connections(2).connect(&url).await?;
        let st = AppState {
            db: db.clone(),
            hub: crate::cable::Hub::default(),
            public_dir: std::path::PathBuf::new(),
            frontend_url: String::new(),
        };
        let (account_id,): (i64,) = sqlx::query_as("SELECT account_id::bigint FROM inboxes WHERE id = $1")
            .bind(inbox_id as i32)
            .fetch_one(&db)
            .await?;
        let stamp = Utc::now().timestamp_millis();
        // Stored source_id is the cleaned Message-ID (no angle brackets).
        let m1 = format!("ingest-live-{stamp}-1@test.invalid");
        let m2 = format!("ingest-live-{stamp}-2@test.invalid");
        let raw1 = format!("Message-ID: <{m1}>\r\nFrom: Live Test <live-{stamp}@test.invalid>\r\nSubject: First\r\n\r\nHello one\r\n");
        let raw2 = format!("Message-ID: <{m2}>\r\nIn-Reply-To: <{m1}>\r\nFrom: Live Test <live-{stamp}@test.invalid>\r\nSubject: Re: First\r\n\r\nHello two\r\n");

        let result: anyhow::Result<()> = async {
            ingest_raw(&st, account_id, inbox_id, raw1.as_bytes()).await?;
            ingest_raw(&st, account_id, inbox_id, raw2.as_bytes()).await?;
            let convs: Vec<(i64,)> = sqlx::query_as("SELECT DISTINCT conversation_id::bigint FROM messages WHERE inbox_id = $1 AND source_id IN ($2, $3)")
                .bind(inbox_id as i32)
                .bind(&m1)
                .bind(&m2)
                .fetch_all(&db)
                .await?;
            anyhow::ensure!(convs.len() == 1, "expected 1 conversation, got {}", convs.len());
            let (incoming,): (i64,) = sqlx::query_as("SELECT count(*) FROM messages WHERE conversation_id = $1 AND message_type = 0")
                .bind(convs[0].0 as i32)
                .fetch_one(&db)
                .await?;
            anyhow::ensure!(incoming == 2, "expected 2 incoming messages, got {incoming}");
            Ok(())
        }
        .await;

        // Cleanup runs even when assertions fail. Only rows created by this test are removed.
        let created: Vec<(i64,)> = sqlx::query_as("SELECT DISTINCT conversation_id::bigint FROM messages WHERE inbox_id = $1 AND source_id IN ($2, $3)")
            .bind(inbox_id as i32)
            .bind(&m1)
            .bind(&m2)
            .fetch_all(&db)
            .await?;
        let conv_ids: Vec<i64> = created.into_iter().map(|c| c.0).collect();
        sqlx::query("DELETE FROM messages WHERE inbox_id = $1 AND source_id IN ($2, $3)").bind(inbox_id as i32).bind(&m1).bind(&m2).execute(&db).await?;
        sqlx::query("DELETE FROM conversations WHERE id = ANY($1) AND NOT EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = conversations.id)").bind(&conv_ids).execute(&db).await?;
        sqlx::query("DELETE FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2").bind(inbox_id as i32).bind(format!("live-{stamp}@test.invalid")).execute(&db).await?;
        sqlx::query("DELETE FROM contacts WHERE account_id = $1 AND lower(email) = $2 AND NOT EXISTS (SELECT 1 FROM conversations c WHERE c.contact_id = contacts.id)")
            .bind(account_id as i32)
            .bind(format!("live-{stamp}@test.invalid"))
            .execute(&db)
            .await?;
        result
    }
}
