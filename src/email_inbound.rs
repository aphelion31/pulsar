//! Inbound email for `Channel::Email` inboxes via IMAP polling.
//! Mirrors chatwoot/app/jobs/inboxes/fetch_imap_email_inboxes_job.rb,
//! fetch_imap_emails_job.rb, app/services/imap/base_fetch_email_service.rb,
//! app/services/imap/fetch_email_service.rb and app/mailboxes/imap/imap_mailbox.rb.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, anyhow};
use axum::{
    Json, Router,
    body::Bytes,
    extract::{Multipart, State},
    http::StatusCode,
    routing::post,
};
use base64::Engine as _;
use chrono::{Duration as ChronoDuration, Utc};
use futures::StreamExt;
use mail_parser::{MessageParser, TextList};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, rustls};

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
    imap_authentication: Option<String>,
}

async fn fetch_all(st: &AppState) -> anyhow::Result<()> {
    // Mirrors FetchImapEmailInboxesJob#should_fetch_emails?: skip suspended accounts and imap-disabled channels.
    let rows: Vec<EmailInbox> = sqlx::query_as(
        "SELECT i.id AS inbox_id, i.account_id, ce.imap_address, ce.imap_port, ce.imap_login,
                ce.imap_password, ce.imap_enable_ssl, ce.imap_authentication
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
        let cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let server = rustls::pki_types::ServerName::try_from(ch.imap_address.clone())
            .map_err(|e| anyhow!("bad imap host: {e}"))?;
        let tls = TlsConnector::from(Arc::new(cfg))
            .connect(server, tcp)
            .await
            .context("tls handshake")?;
        session_fetch(async_imap::Client::new(tls), ch).await
    } else {
        session_fetch(async_imap::Client::new(tcp), ch).await
    }
}

struct CramMd5Auth {
    user: String,
    pass: String,
}

impl async_imap::Authenticator for CramMd5Auth {
    type Response = Vec<u8>;
    fn process(&mut self, challenge: &[u8]) -> Self::Response {
        let digest = hmac_md5(self.pass.as_bytes(), challenge);
        format!("{} {}", self.user, hex::encode(digest)).into_bytes()
    }
}

/// Pure-Rust RFC 1321 MD5 + RFC 2104 HMAC-MD5 solely for IMAP CRAM-MD5 auth (zero extra Cargo deps).
fn md5_digest(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut msg = input.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_le_bytes());

    let (mut a0, mut b0, mut c0, mut d0) =
        (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    for chunk in msg.chunks_exact(64) {
        let mut m = [0u32; 16];
        for (i, w) in m.iter_mut().enumerate() {
            *w = u32::from_le_bytes(chunk[i * 4..i * 4 + 4].try_into().unwrap());
        }
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | ((!b) & d), i),
                16..=31 => ((d & b) | ((!d) & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | (!d)), (7 * i) % 16),
            };
            let next_d = d;
            d = c;
            c = b;
            let sum = a.wrapping_add(f).wrapping_add(K[i]).wrapping_add(m[g]);
            b = b.wrapping_add(sum.rotate_left(S[i]));
            a = next_d;
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

fn hmac_md5(key: &[u8], data: &[u8]) -> [u8; 16] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..16].copy_from_slice(&md5_digest(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + data.len());
    inner.extend(k.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(data);
    let inner_hash = md5_digest(&inner);
    let mut outer = Vec::with_capacity(64 + 16);
    outer.extend(k.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&inner_hash);
    md5_digest(&outer)
}

async fn session_fetch<T>(
    client: async_imap::Client<T>,
    ch: &EmailInbox,
) -> anyhow::Result<Vec<Vec<u8>>>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + std::fmt::Debug + Send,
{
    let pass = crate::ar_encryption::decrypt(&ch.imap_password);
    let use_cram = ch
        .imap_authentication
        .as_deref()
        .is_some_and(|m| m.eq_ignore_ascii_case("cram-md5") || m.eq_ignore_ascii_case("cram_md5"));
    let mut session = if use_cram {
        client
            .authenticate(
                "CRAM-MD5",
                CramMd5Auth {
                    user: ch.imap_login.clone(),
                    pass,
                },
            )
            .await
            .map_err(|(e, _)| anyhow!("imap cram-md5 login failed: {e}"))?
    } else {
        client
            .login(&ch.imap_login, &pass)
            .await
            .map_err(|(e, _)| anyhow!("imap login failed: {e}"))?
    };
    session.select("INBOX").await.context("select INBOX")?;
    let since = (Utc::now() - ChronoDuration::days(1))
        .format("%d-%b-%Y")
        .to_string();
    let seqs = session
        .search(format!("SINCE {since}"))
        .await
        .context("imap search")?;
    let mut seq_list: Vec<u32> = seqs.into_iter().collect();
    seq_list.sort_unstable();
    seq_list.truncate(MAX_MESSAGES_PER_SYNC);

    let mut out = Vec::new();
    for seq in seq_list {
        // BODY.PEEK[] keeps the message unread, as Chatwoot does.
        let mut stream = session
            .fetch(seq.to_string(), "BODY.PEEK[]")
            .await
            .context("imap fetch")?;
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
    s.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .to_string()
}

/// Drop the quoted reply: lines from the first `>` quote or `On ... wrote:` intro, and a `-- ` signature delimiter.
pub(crate) fn strip_quoted_reply(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim_start();
        let lower = t.to_ascii_lowercase();
        if t.starts_with('>')
            || (lower.starts_with("on ") && lower.ends_with("wrote:"))
            || t == "--"
            || t == "-- "
        {
            break;
        }
        out.push(line);
    }
    out.join("\n").trim_end().to_string()
}

fn text_list(l: Option<TextList<'_>>) -> Vec<String> {
    l.map(|l| l.iter().map(clean_message_id).collect())
        .unwrap_or_default()
}

pub(crate) fn parse_mail(raw: &[u8]) -> anyhow::Result<ParsedMail> {
    let msg = MessageParser::default()
        .parse(raw)
        .ok_or_else(|| anyhow!("unparseable email"))?;
    let message_id = msg
        .message_id()
        .map(clean_message_id)
        .filter(|s| !s.is_empty());
    let in_reply_to = text_list(msg.in_reply_to()).into_iter().next();
    let references = text_list(msg.references());
    let from = msg.from().and_then(|a| a.first());
    let from_email = from
        .as_ref()
        .and_then(|m| m.address())
        .map(str::to_lowercase);
    let from_name = from.as_ref().and_then(|m| m.name()).map(str::to_string);
    let addrs = |list: Option<mail_parser::AddressList<'_>>| -> String {
        list.map(|a| {
            a.mailboxes()
                .filter_map(|x| x.address().map(str::to_string))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
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
pub(crate) async fn ingest_raw(
    st: &AppState,
    account_id: i64,
    inbox_id: i64,
    raw: &[u8],
) -> anyhow::Result<()> {
    let mail = parse_mail(raw)?;
    let Some(message_id) = mail.message_id.clone() else {
        return Ok(());
    };
    let exists: Option<i32> =
        sqlx::query_scalar("SELECT 1 FROM messages WHERE inbox_id = $1 AND source_id = $2 LIMIT 1")
            .bind(inbox_id as i32)
            .bind(&message_id)
            .fetch_optional(&st.db)
            .await?;
    if exists.is_some() {
        return Ok(());
    }
    let sender = mail
        .from_email
        .clone()
        .ok_or_else(|| anyhow!("mail {message_id} has no From address"))?;

    let (contact_id, contact_inbox_id) = find_or_create_contact(
        &st.db,
        account_id,
        inbox_id,
        &sender,
        mail.from_name.as_deref(),
    )
    .await?;
    let conv = find_or_create_conversation(
        &st.db,
        account_id,
        inbox_id,
        contact_id,
        contact_inbox_id,
        &mail,
    )
    .await?;

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
async fn find_or_create_contact(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    sender: &str,
    name: Option<&str>,
) -> anyhow::Result<(i64, i64)> {
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
    let (ci,): (i64,) = sqlx::query_as(
        "SELECT id::bigint FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2",
    )
    .bind(inbox_id as i32)
    .bind(sender)
    .fetch_one(db)
    .await?;
    Ok((contact_id, ci))
}

/// Reply lookup: In-Reply-To / References matching an existing message source_id in this inbox reuses its conversation.
async fn find_or_create_conversation(
    db: &PgPool,
    account_id: i64,
    inbox_id: i64,
    contact_id: i64,
    contact_inbox_id: i64,
    mail: &ParsedMail,
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
            return conversations::reload(db, cid)
                .await
                .map_err(|e| anyhow!("reload conversation: {e:?}"));
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
    conversations::reload(db, cid)
        .await
        .map_err(|e| anyhow!("reload conversation: {e:?}"))
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
        assert_eq!(
            p.references,
            vec!["m0@x.com".to_string(), "m1@x.com".to_string()]
        );
        assert_eq!(p.from_email.as_deref(), Some("ann@example.com"));
    }

    #[test]
    fn strips_quoted_reply() {
        assert_eq!(
            strip_quoted_reply("Thanks\n\nOn Mon, a wrote:\n> old"),
            "Thanks"
        );
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
        let url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://localhost/pulsar".into());
        let inbox_id: i64 = std::env::var("PULSAR_TEST_INBOX_ID")?.parse()?;
        let db = sqlx::postgres::PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await?;
        let st = AppState {
            db: db.clone(),
            hub: crate::cable::Hub::default(),
            public_dir: std::path::PathBuf::new(),
            frontend_url: String::new(),
        };
        let (account_id,): (i64,) =
            sqlx::query_as("SELECT account_id::bigint FROM inboxes WHERE id = $1")
                .bind(inbox_id as i32)
                .fetch_one(&db)
                .await?;
        let stamp = Utc::now().timestamp_millis();
        // Stored source_id is the cleaned Message-ID (no angle brackets).
        let m1 = format!("ingest-live-{stamp}-1@test.invalid");
        let m2 = format!("ingest-live-{stamp}-2@test.invalid");
        let raw1 = format!(
            "Message-ID: <{m1}>\r\nFrom: Live Test <live-{stamp}@test.invalid>\r\nSubject: First\r\n\r\nHello one\r\n"
        );
        let raw2 = format!(
            "Message-ID: <{m2}>\r\nIn-Reply-To: <{m1}>\r\nFrom: Live Test <live-{stamp}@test.invalid>\r\nSubject: Re: First\r\n\r\nHello two\r\n"
        );

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
        sqlx::query("DELETE FROM messages WHERE inbox_id = $1 AND source_id IN ($2, $3)")
            .bind(inbox_id as i32)
            .bind(&m1)
            .bind(&m2)
            .execute(&db)
            .await?;
        sqlx::query("DELETE FROM conversations WHERE id = ANY($1) AND NOT EXISTS (SELECT 1 FROM messages m WHERE m.conversation_id = conversations.id)").bind(&conv_ids).execute(&db).await?;
        sqlx::query("DELETE FROM contact_inboxes WHERE inbox_id = $1 AND source_id = $2")
            .bind(inbox_id as i32)
            .bind(format!("live-{stamp}@test.invalid"))
            .execute(&db)
            .await?;
        sqlx::query("DELETE FROM contacts WHERE account_id = $1 AND lower(email) = $2 AND NOT EXISTS (SELECT 1 FROM conversations c WHERE c.contact_id = contacts.id)")
            .bind(account_id as i32)
            .bind(format!("live-{stamp}@test.invalid"))
            .execute(&db)
            .await?;
        result
    }
}

/// ActionMailbox-compatible HTTP inbound email webhooks for Mailgun, SendGrid, Postmark, Mandrill,
/// generic SMTP/Postfix relay, and Amazon SES/SNS.
pub(crate) fn webhook_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/rails/action_mailbox/mailgun/inbound_emails/mime",
            post(mailgun_inbound),
        )
        .route(
            "/rails/action_mailbox/sendgrid/inbound_emails",
            post(sendgrid_inbound),
        )
        .route(
            "/rails/action_mailbox/postmark/inbound_emails",
            post(postmark_inbound),
        )
        .route(
            "/rails/action_mailbox/mandrill/inbound_emails",
            post(mandrill_inbound),
        )
        .route(
            "/rails/action_mailbox/relay/inbound_emails",
            post(relay_inbound),
        )
        .route(
            "/rails/action_mailbox/amazon/inbound_emails",
            post(amazon_inbound),
        )
}

async fn find_email_channel(db: &PgPool, candidates: &[String]) -> Option<(i64, i64)> {
    let lower: Vec<String> = candidates
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s.contains('@'))
        .collect();
    if lower.is_empty() {
        return None;
    }
    sqlx::query_as(
        "SELECT i.account_id::bigint, i.id::bigint
         FROM inboxes i
         JOIN channel_email ce ON ce.id = i.channel_id
         JOIN accounts a ON a.id = i.account_id
         WHERE i.channel_type = 'Channel::Email'
           AND COALESCE(a.status, 0) <> 1
           AND (lower(ce.email) = ANY($1) OR lower(ce.forward_to_email) = ANY($1))
         ORDER BY i.id LIMIT 1",
    )
    .bind(&lower)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}

fn synth_rfc822(from: &str, to: &str, subject: &str, msg_id: &str, text: &str) -> Vec<u8> {
    let mid = if msg_id.starts_with('<') {
        msg_id.to_string()
    } else {
        format!("<{msg_id}>")
    };
    format!(
        "From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nMessage-ID: {mid}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n"
    )
    .into_bytes()
}

async fn route_raw_email(st: &AppState, raw: &[u8], hint_recipients: &[String]) -> StatusCode {
    let mut candidates = hint_recipients.to_vec();
    if let Ok(parsed) = parse_mail(raw) {
        candidates.extend(
            parsed
                .to
                .split(',')
                .chain(parsed.cc.split(','))
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        );
    }
    let Some((account_id, inbox_id)) = find_email_channel(&st.db, &candidates).await else {
        tracing::warn!(recipients = ?candidates, "inbound email webhook matched no Channel::Email inbox");
        return StatusCode::NO_CONTENT;
    };
    match ingest_raw(st, account_id, inbox_id, raw).await {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(e) => {
            tracing::error!(inbox_id, error = %e, "inbound email webhook ingest failed");
            StatusCode::UNPROCESSABLE_ENTITY
        }
    }
}

async fn mailgun_inbound(State(st): State<AppState>, mut mp: Multipart) -> StatusCode {
    let mut raw_mime: Option<Vec<u8>> = None;
    let mut recipient = String::new();
    let mut sender = String::new();
    let mut subject = String::new();
    let mut text = String::new();
    let mut msg_id = String::new();

    while let Ok(Some(field)) = mp.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "body-mime" => {
                if let Ok(b) = field.bytes().await {
                    raw_mime = Some(b.to_vec());
                }
            }
            "recipient" | "To" => recipient = field.text().await.unwrap_or_default(),
            "sender" | "from" | "From" => sender = field.text().await.unwrap_or_default(),
            "subject" | "Subject" => subject = field.text().await.unwrap_or_default(),
            "body-plain" | "stripped-text" => text = field.text().await.unwrap_or_default(),
            "Message-Id" | "message-id" => msg_id = field.text().await.unwrap_or_default(),
            _ => {}
        }
    }
    let hints: Vec<String> = recipient
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(raw) = raw_mime {
        return route_raw_email(&st, &raw, &hints).await;
    }
    if sender.is_empty() || recipient.is_empty() {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    let mid = if msg_id.is_empty() {
        format!("mailgun-{}@pulsar.local", uuid::Uuid::new_v4().simple())
    } else {
        msg_id
    };
    let raw = synth_rfc822(&sender, &recipient, &subject, &mid, &text);
    route_raw_email(&st, &raw, &hints).await
}

async fn sendgrid_inbound(State(st): State<AppState>, mut mp: Multipart) -> StatusCode {
    let mut raw_mime: Option<Vec<u8>> = None;
    let mut to = String::new();
    let mut from = String::new();
    let mut subject = String::new();
    let mut text = String::new();
    while let Ok(Some(field)) = mp.next_field().await {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "email" => {
                if let Ok(b) = field.bytes().await {
                    raw_mime = Some(b.to_vec());
                }
            }
            "to" => to = field.text().await.unwrap_or_default(),
            "from" => from = field.text().await.unwrap_or_default(),
            "subject" => subject = field.text().await.unwrap_or_default(),
            "text" => text = field.text().await.unwrap_or_default(),
            _ => {}
        }
    }
    let hints: Vec<String> = to
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(raw) = raw_mime {
        return route_raw_email(&st, &raw, &hints).await;
    }
    if from.is_empty() || to.is_empty() {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    let mid = format!("sendgrid-{}@pulsar.local", uuid::Uuid::new_v4().simple());
    let raw = synth_rfc822(&from, &to, &subject, &mid, &text);
    route_raw_email(&st, &raw, &hints).await
}

async fn postmark_inbound(State(st): State<AppState>, Json(v): Json<Value>) -> StatusCode {
    let mut hints = Vec::new();
    if let Some(s) = v["OriginalRecipient"].as_str().filter(|s| !s.is_empty()) {
        hints.push(s.to_string());
    }
    if let Some(arr) = v["ToFull"].as_array() {
        for item in arr {
            if let Some(e) = item["Email"].as_str() {
                hints.push(e.to_string());
            }
        }
    }
    if let Some(raw) = v["RawEmail"].as_str().filter(|s| !s.is_empty()) {
        return route_raw_email(&st, raw.as_bytes(), &hints).await;
    }
    let from = v["FromFull"]["Email"]
        .as_str()
        .or_else(|| v["From"].as_str())
        .unwrap_or_default();
    let to = v["To"]
        .as_str()
        .or_else(|| hints.first().map(String::as_str))
        .unwrap_or_default();
    if from.is_empty() || to.is_empty() {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    let subject = v["Subject"].as_str().unwrap_or_default();
    let text = v["TextBody"]
        .as_str()
        .or_else(|| v["StrippedTextReply"].as_str())
        .unwrap_or_default();
    let mid = v["MessageID"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("postmark-{}@pulsar.local", uuid::Uuid::new_v4().simple()));
    let raw = synth_rfc822(from, to, subject, &mid, text);
    route_raw_email(&st, &raw, &hints).await
}

async fn mandrill_inbound(State(st): State<AppState>, body: Bytes) -> StatusCode {
    let events: Value = if let Ok(v) = serde_json::from_slice::<Value>(&body) {
        v
    } else if let Some((_, payload)) =
        url::form_urlencoded::parse(&body).find(|(k, _)| k == "mandrill_events")
    {
        serde_json::from_str(&payload).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let Some(arr) = events.as_array() else {
        return StatusCode::NO_CONTENT;
    };
    for ev in arr {
        let msg = &ev["msg"];
        let mut hints = Vec::new();
        if let Some(e) = msg["email"].as_str() {
            hints.push(e.to_string());
        }
        if let Some(raw) = msg["raw_msg"].as_str().filter(|s| !s.is_empty()) {
            let _ = route_raw_email(&st, raw.as_bytes(), &hints).await;
            continue;
        }
        let from = msg["from_email"].as_str().unwrap_or_default();
        let to = hints.first().map(String::as_str).unwrap_or_default();
        if !from.is_empty() && !to.is_empty() {
            let subject = msg["subject"].as_str().unwrap_or_default();
            let text = msg["text"].as_str().unwrap_or_default();
            let mid = msg["headers"]["Message-Id"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("mandrill-{}@pulsar.local", uuid::Uuid::new_v4().simple()));
            let raw = synth_rfc822(from, to, subject, &mid, text);
            let _ = route_raw_email(&st, &raw, &hints).await;
        }
    }
    StatusCode::NO_CONTENT
}

async fn relay_inbound(State(st): State<AppState>, body: Bytes) -> StatusCode {
    if body.is_empty() {
        return StatusCode::UNPROCESSABLE_ENTITY;
    }
    route_raw_email(&st, &body, &[]).await
}

async fn amazon_inbound(State(st): State<AppState>, Json(env): Json<Value>) -> StatusCode {
    if env["Type"].as_str() == Some("SubscriptionConfirmation")
        && let Some(sub_url) = env["SubscribeURL"].as_str()
    {
        let _ = crate::listeners::webhooks::safe_fetch_bytes(sub_url, 64 * 1024).await;
        return StatusCode::NO_CONTENT;
    }
    let msg: Value = match &env["Message"] {
        Value::String(s) => serde_json::from_str(s).unwrap_or(Value::Null),
        other => other.clone(),
    };
    let mut hints = Vec::new();
    if let Some(arr) = msg["mail"]["destination"].as_array() {
        for d in arr {
            if let Some(s) = d.as_str() {
                hints.push(s.to_string());
            }
        }
    }
    if let Some(arr) = msg["receipt"]["recipients"].as_array() {
        for r in arr {
            if let Some(s) = r.as_str() {
                hints.push(s.to_string());
            }
        }
    }
    if let Some(content) = msg["content"].as_str().filter(|s| !s.is_empty()) {
        if let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(content) {
            return route_raw_email(&st, &decoded, &hints).await;
        }
        return route_raw_email(&st, content.as_bytes(), &hints).await;
    }
    StatusCode::NO_CONTENT
}
