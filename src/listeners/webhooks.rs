use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use hmac::{Hmac, Mac};
use serde_json::json;
use sha2_compat::Sha256;
use sqlx::Row;

use crate::{common::AppState, events::Event};

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.is_broadcast()
                || ip.is_multicast()
                || o[0] == 0
                || o[0] >= 224
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 192 && o[1] == 0 && o[2] == 0)
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19)))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(v4.into());
            }
            (ip.segments()[0] & 0xe000) == 0x2000
                && !(ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_multicast()
                    || (ip.segments()[0] & 0xfe00) == 0xfc00
                    || (ip.segments()[0] & 0xffc0) == 0xfe80)
        }
    }
}

async fn deliver(url: &str, secret: Option<&str>, body: &str) -> anyhow::Result<()> {
    let url = url::Url::parse(url)?;
    anyhow::ensure!(
        matches!(url.scheme(), "http" | "https")
            && url.username().is_empty()
            && url.password().is_none(),
        "invalid webhook URL"
    );
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("missing host"))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| anyhow::anyhow!("missing port"))?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, port)).await?.collect();
    anyhow::ensure!(!addresses.is_empty(), "host has no addresses");
    if std::env::var("PULSAR_WEBHOOK_ALLOW_PRIVATE").as_deref() != Ok("1") {
        anyhow::ensure!(
            addresses.iter().all(|a| public_ip(a.ip())),
            "private webhook address refused"
        );
    }
    let timeout = std::env::var("WEBHOOK_TIMEOUT")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(5);
    // Pin validated DNS results, disable redirects and proxies to prevent DNS rebinding and redirect SSRF.
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .timeout(Duration::from_secs(timeout))
        .build()?;
    let mut request = client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("X-Chatwoot-Delivery", uuid::Uuid::new_v4().to_string());
    if let Some(secret) = secret.filter(|s| !s.is_empty()) {
        let ts = chrono::Utc::now().timestamp().to_string();
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
        mac.update(format!("{ts}.{body}").as_bytes());
        request = request.header("X-Chatwoot-Timestamp", ts).header(
            "X-Chatwoot-Signature",
            format!("sha256={}", hex::encode(mac.finalize().into_bytes())),
        );
    }
    request
        .body(body.to_owned())
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn handle(st: AppState, ev: Event) {
    if let Err(error) = process(&st, &ev).await {
        tracing::warn!(%error, event = %ev.name, "webhook listener failed");
    }
}

async fn process(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    let event = ev.name.replace('.', "_");
    if !matches!(
        event.as_str(),
        "conversation_status_changed"
            | "conversation_updated"
            | "conversation_created"
            | "message_created"
            | "message_updated"
            | "webwidget_triggered"
            | "contact_created"
            | "contact_updated"
            | "inbox_created"
            | "inbox_updated"
            | "conversation_typing_on"
            | "conversation_typing_off"
    ) {
        return Ok(());
    }
    if matches!(event.as_str(), "contact_updated" | "inbox_updated")
        && ev
            .changed
            .as_ref()
            .is_none_or(|v| v.is_null() || v.as_object().is_some_and(|o| o.is_empty()))
    {
        return Ok(());
    }
    let is_message = event.starts_with("message_");
    let mut payload = ev.data.clone();
    let mut inbox_id = payload["inbox_id"].as_i64();
    if is_message {
        let id = payload["id"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("missing message id"))?;
        let row = sqlx::query("SELECT m.message_type, m.private, m.conversation_id::bigint AS conversation_id, m.inbox_id::bigint AS inbox_id, i.name AS inbox_name, a.name AS account_name FROM messages m JOIN inboxes i ON i.id=m.inbox_id AND i.account_id=m.account_id JOIN accounts a ON a.id=m.account_id WHERE m.id=$1 AND m.account_id=$2")
            .bind(id as i32).bind(ev.account_id as i32).fetch_optional(&st.db).await?;
        let Some(row) = row else {
            return Ok(());
        };
        let kind: i32 = row.try_get("message_type")?;
        let private: bool = row.try_get("private")?;
        if !matches!(kind, 0 | 1 | 3) || private {
            return Ok(());
        }
        let conv = crate::conversations::reload(&st.db, row.try_get("conversation_id")?)
            .await
            .map_err(|e| anyhow::anyhow!("conversation reload: {e:?}"))?;
        let mut conversation = crate::conversations::conv_json(&st.db, &conv).await;
        let account =
            json!({"id": ev.account_id, "name": row.try_get::<String,_>("account_name")?});
        conversation["account"] = account.clone();
        inbox_id = Some(row.try_get("inbox_id")?);
        let original = payload;
        payload = json!({"account": account, "additional_attributes": original["additional_attributes"], "content_attributes": original["content_attributes"], "content_type": original["content_type"], "content": original["content"], "conversation": conversation, "created_at": original["created_at"], "id": id, "inbox": {"id": inbox_id, "name": row.try_get::<String,_>("inbox_name")?}, "message_type": match kind {0 => "incoming", 1 => "outgoing", _ => "template"}, "private": private, "sender": original["sender"], "source_id": original["source_id"]});
        if let Some(attachments) = original.get("attachments") {
            payload["attachments"] = attachments.clone();
        }
    }
    let Some(object) = payload.as_object_mut() else {
        return Ok(());
    };
    object.insert("event".into(), json!(event));
    if matches!(
        event.as_str(),
        "conversation_status_changed"
            | "conversation_updated"
            | "contact_updated"
            | "inbox_updated"
    ) {
        object.insert(
            "changed_attributes".into(),
            ev.changed.clone().unwrap_or(json!({})),
        );
    }
    let body = serde_json::to_string(&payload)?;
    let flags: Option<(i64, i64)> =
        sqlx::query_as("SELECT feature_flags, feature_flags_ext_1 FROM accounts WHERE id=$1")
            .bind(ev.account_id as i32)
            .fetch_optional(&st.db)
            .await?;
    let enabled = flags
        .is_some_and(|(a, b)| crate::auth::enabled_features(a, b).contains_key("api_and_webhooks"));
    let rows = sqlx::query("SELECT url, secret FROM webhooks WHERE account_id=$1 AND webhook_type=0 AND subscriptions ? $2 AND url IS NOT NULL AND $3")
        .bind(ev.account_id as i32).bind(&event).bind(enabled).fetch_all(&st.db).await?;
    for row in rows {
        let url: String = row.try_get("url")?;
        let secret: Option<String> = row.try_get("secret")?;
        if let Err(error) = deliver(&url, secret.as_deref(), &body).await {
            tracing::warn!(%error, "account webhook delivery failed");
        }
    }
    if !event.starts_with("contact_") && !event.starts_with("inbox_") {
        if let Some(inbox) = inbox_id {
            let row = sqlx::query("SELECT c.webhook_url, c.secret FROM inboxes i JOIN channel_api c ON c.id=i.channel_id AND c.account_id=i.account_id WHERE i.id=$1 AND i.account_id=$2 AND i.channel_type='Channel::Api' AND c.webhook_url IS NOT NULL AND c.webhook_url<>''")
                .bind(inbox as i32).bind(ev.account_id as i32).fetch_optional(&st.db).await?;
            if let Some(row) = row {
                let url: String = row.try_get("webhook_url")?;
                let secret: Option<String> = row.try_get("secret")?;
                if let Err(error) = deliver(&url, secret.as_deref(), &body).await {
                    tracing::warn!(%error, "API inbox webhook delivery failed");
                    if is_message {
                        sqlx::query("UPDATE messages SET status=3, updated_at=now() WHERE id=$1 AND account_id=$2").bind(payload["id"].as_i64().map(|n| n as i32)).bind(ev.account_id as i32).execute(&st.db).await?;
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blocks_private_addresses() {
        for ip in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "::1",
            "::ffff:127.0.0.1",
            "fc00::1",
        ] {
            assert!(!public_ip(ip.parse().unwrap()), "{ip}");
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
    }
}
