//! In-app notifications. Port of Chatwoot's NotificationListener, NotificationBuilder, ParticipationListener,
//! Messages::MentionService and Messages::NewMessageNotificationService. Email and push delivery are not ported
//! yet: this writes `notifications` rows and sends `notification.created` to the user's pubsub token.
//!
//! events::start runs every listener for every event, so `handle` dispatches on `ev.name`.
//! ponytail: no self-assignment skip (the assignee.changed event carries no actor), and the access check is
//! admin or inbox member only (no custom-role or team rules). Add when the event carries performed_by.
//! ponytail: a re-assignment is skipped while an unread conversation_assignment for the same agent exists,
//! because the event carries no previous assignee to detect a real change.

use chrono::NaiveDateTime;
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::common::AppState;
use crate::events::Event;

/// Chatwoot Notification::NOTIFICATION_TYPES in order; stored value = index + 1.
const TYPE_NAMES: [&str; 5] = [
    "conversation_creation",
    "conversation_assignment",
    "assigned_conversation_new_message",
    "conversation_mention",
    "participating_conversation_new_message",
];
const CONVERSATION_CREATION: i64 = 1;
const CONVERSATION_ASSIGNMENT: i64 = 2;
const ASSIGNED_NEW_MESSAGE: i64 = 3;
const CONVERSATION_MENTION: i64 = 4;
const PARTICIPATING_NEW_MESSAGE: i64 = 5;
/// conversations.status index for "pending" (STATUSES in conversations.rs).
const PENDING: i32 = 2;

pub async fn handle(st: AppState, ev: Event) {
    let res = match ev.name.as_str() {
        "conversation.created" => conversation_created(&st, &ev).await,
        "assignee.changed" => assignee_changed(&st, &ev).await,
        "message.created" => message_created(&st, &ev).await,
        _ => return,
    };
    if let Err(e) = res {
        tracing::error!("notifications for {}: {e}", ev.name);
    }
}

#[derive(sqlx::FromRow)]
struct ConvRef {
    id: i64,
    inbox_id: i64,
    status: i32,
    assignee_id: Option<i64>,
    contact_id: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct NotifRow {
    notification_type: i32,
    primary_actor_id: i64,
    created_at: NaiveDateTime,
    last_activity_at: Option<NaiveDateTime>,
    name: Option<String>,
    pubsub_token: Option<String>,
}

async fn conv_ref(db: &PgPool, account_id: i64, display_id: i64) -> sqlx::Result<Option<ConvRef>> {
    sqlx::query_as(
        "SELECT id::bigint AS id, inbox_id::bigint AS inbox_id, status, assignee_id::bigint AS assignee_id,
                contact_id::bigint AS contact_id
         FROM conversations WHERE account_id = $1::bigint AND display_id = $2::bigint",
    )
    .bind(account_id)
    .bind(display_id)
    .fetch_optional(db)
    .await
}

async fn conversation_created(st: &AppState, ev: &Event) -> sqlx::Result<()> {
    let Some(did) = ev.data["id"].as_i64() else { return Ok(()) };
    let Some(conv) = conv_ref(&st.db, ev.account_id, did).await? else { return Ok(()) };
    if conv.status == PENDING {
        return Ok(());
    }
    let members: Vec<i64> = sqlx::query_scalar("SELECT user_id::bigint FROM inbox_members WHERE inbox_id = $1::bigint")
        .bind(conv.inbox_id)
        .fetch_all(&st.db)
        .await?;
    for uid in members {
        build(st, ev.account_id, &conv, uid, CONVERSATION_CREATION, None).await?;
    }
    Ok(())
}

async fn assignee_changed(st: &AppState, ev: &Event) -> sqlx::Result<()> {
    let Some(did) = ev.data["id"].as_i64() else { return Ok(()) };
    let Some(conv) = conv_ref(&st.db, ev.account_id, did).await? else { return Ok(()) };
    let Some(assignee) = conv.assignee_id else { return Ok(()) };
    add_participant(&st.db, ev.account_id, assignee, conv.id).await?;
    if conv.status == PENDING {
        return Ok(());
    }
    let unread_dup: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM notifications WHERE user_id = $1::bigint AND primary_actor_type = 'Conversation'
           AND primary_actor_id = $2::bigint AND notification_type = $3::bigint AND read_at IS NULL)",
    )
    .bind(assignee)
    .bind(conv.id)
    .bind(CONVERSATION_ASSIGNMENT)
    .fetch_one(&st.db)
    .await?;
    if unread_dup {
        return Ok(());
    }
    build(st, ev.account_id, &conv, assignee, CONVERSATION_ASSIGNMENT, None).await
}

async fn message_created(st: &AppState, ev: &Event) -> sqlx::Result<()> {
    let db = &st.db;
    let d = &ev.data;
    let Some(did) = d["conversation_id"].as_i64() else { return Ok(()) };
    let Some(msg_id) = d["id"].as_i64() else { return Ok(()) };
    let Some(conv) = conv_ref(db, ev.account_id, did).await? else { return Ok(()) };
    let private = d["private"].as_bool().unwrap_or(false);
    let content = d["content"].as_str().unwrap_or_default();
    let sender = if d["sender_type"].as_str() == Some("User") { d["sender_id"].as_i64() } else { None };
    let secondary = Some(("Message", msg_id));

    // Mentions run first, like Chatwoot's listener: private notes only, mentioned users must be admins or inbox members.
    if private && !content.trim().is_empty() {
        let mut ids = mention_ids(content, "user");
        let teams = mention_ids(content, "team");
        if !teams.is_empty() {
            let members: Vec<i64> = sqlx::query_scalar(
                "SELECT tm.user_id::bigint FROM team_members tm JOIN teams t ON t.id = tm.team_id
                 WHERE t.account_id = $1::bigint AND tm.team_id = ANY($2::bigint[])",
            )
            .bind(ev.account_id)
            .bind(&teams)
            .fetch_all(db)
            .await?;
            ids.extend(members);
        }
        let valid: Vec<i64> = sqlx::query_scalar(
            "SELECT user_id::bigint FROM account_users WHERE account_id = $1::bigint AND role = 1
             UNION SELECT user_id::bigint FROM inbox_members WHERE inbox_id = $2::bigint",
        )
        .bind(ev.account_id)
        .bind(conv.inbox_id)
        .fetch_all(db)
        .await?;
        ids.retain(|i| valid.contains(i));
        ids.sort_unstable();
        ids.dedup();
        for uid in ids {
            add_participant(db, ev.account_id, uid, conv.id).await?;
            if Some(uid) != sender {
                build(st, ev.account_id, &conv, uid, CONVERSATION_MENTION, secondary).await?;
            }
        }
    }

    // Chatwoot's message.notifiable?: incoming or outgoing, not private.
    let notifiable = matches!(d["message_type"].as_i64(), Some(0) | Some(1)) && !private;
    if !notifiable {
        return Ok(());
    }
    if let Some(a) = conv.assignee_id {
        if Some(a) != sender && !already_notified(db, conv.id, a, msg_id).await? {
            build(st, ev.account_id, &conv, a, ASSIGNED_NEW_MESSAGE, secondary).await?;
        }
    }
    let participants: Vec<i64> =
        sqlx::query_scalar("SELECT user_id::bigint FROM conversation_participants WHERE conversation_id = $1::bigint")
            .bind(conv.id)
            .fetch_all(db)
            .await?;
    for uid in participants {
        if Some(uid) == sender || already_notified(db, conv.id, uid, msg_id).await? {
            continue;
        }
        build(st, ev.account_id, &conv, uid, PARTICIPATING_NEW_MESSAGE, secondary).await?;
    }
    Ok(())
}

/// Chatwoot Messages::MentionService#mentioned_ids: `(mention://user/ID/name)` or `(mention://team/ID/name)`.
fn mention_ids(text: &str, kind: &str) -> Vec<i64> {
    let marker = format!("(mention://{kind}/");
    text.split(marker.as_str()).skip(1).filter_map(|rest| rest.split('/').next()?.parse::<i64>().ok()).collect()
}

async fn already_notified(db: &PgPool, conv_id: i64, user_id: i64, msg_id: i64) -> sqlx::Result<bool> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM notifications WHERE user_id = $1::bigint AND primary_actor_type = 'Conversation'
           AND primary_actor_id = $2::bigint AND secondary_actor_type = 'Message' AND secondary_actor_id = $3::bigint)",
    )
    .bind(user_id)
    .bind(conv_id)
    .bind(msg_id)
    .fetch_one(db)
    .await
}

async fn add_participant(db: &PgPool, account_id: i64, user_id: i64, conv_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO conversation_participants (account_id, user_id, conversation_id, created_at, updated_at)
         VALUES ($1::bigint, $2::bigint, $3::bigint, now(), now()) ON CONFLICT DO NOTHING",
    )
    .bind(account_id)
    .bind(user_id)
    .bind(conv_id)
    .execute(db)
    .await?;
    Ok(())
}

/// Chatwoot NotificationBuilder#perform: subscription, blocked-contact and access checks, then insert and push.
async fn build(
    st: &AppState, account_id: i64, conv: &ConvRef, user_id: i64, kind: i64, secondary: Option<(&str, i64)>,
) -> sqlx::Result<()> {
    let db = &st.db;
    if kind == CONVERSATION_CREATION {
        let flags: Option<i32> = sqlx::query_scalar(
            "SELECT email_flags | push_flags FROM notification_settings WHERE account_id = $1::bigint AND user_id = $2::bigint",
        )
        .bind(account_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
        if (flags.unwrap_or(0) & 1) == 0 {
            return Ok(());
        }
    }
    if kind != CONVERSATION_MENTION {
        if let Some(cid) = conv.contact_id {
            let blocked: bool = sqlx::query_scalar("SELECT blocked FROM contacts WHERE id = $1::bigint")
                .bind(cid)
                .fetch_optional(db)
                .await?
                .unwrap_or(false);
            if blocked {
                return Ok(());
            }
        }
    }
    let role: Option<i32> = sqlx::query_scalar(
        "SELECT COALESCE(role, 0) FROM account_users WHERE account_id = $1::bigint AND user_id = $2::bigint",
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let Some(role) = role else { return Ok(()) };
    if role != 1 {
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM inbox_members WHERE inbox_id = $1::bigint AND user_id = $2::bigint)",
        )
        .bind(conv.inbox_id)
        .bind(user_id)
        .fetch_one(db)
        .await?;
        if !member {
            return Ok(());
        }
    }

    let (sec_type, sec_id) = match secondary {
        Some((t, i)) => (Some(t), Some(i)),
        None => (None, None),
    };
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO notifications (account_id, user_id, notification_type, primary_actor_type, primary_actor_id,
            secondary_actor_type, secondary_actor_id, created_at, updated_at, last_activity_at)
         VALUES ($1::bigint, $2::bigint, $3::bigint, 'Conversation', $4::bigint, $5::varchar, $6::bigint, now(), now(), now())
         RETURNING id::bigint",
    )
    .bind(account_id)
    .bind(user_id)
    .bind(kind)
    .bind(conv.id)
    .bind(sec_type)
    .bind(sec_id)
    .fetch_one(db)
    .await?;

    let st2 = st.clone();
    tokio::spawn(async move {
        if let Err(e) = email(&st2, id).await {
            tracing::warn!(error = %e, notification = id, "notification email failed");
        }
    });
    broadcast(st, account_id, user_id, id).await
}

/// Notification::EmailNotificationService + AgentNotifications::ConversationNotificationsMailer.
/// ponytail: bodies are the templates' fixed text plus the action link; the "recent messages" list and
/// liquid customisation are not rendered. Account email rate limit is not enforced.
async fn email(st: &AppState, id: i64) -> anyhow::Result<()> {
    if !crate::mailer::configured() {
        return Ok(());
    }
    type Row = (i64, i64, i64, i64, String, String, Option<NaiveDateTime>, Option<NaiveDateTime>, i64, Option<String>, Option<i64>);
    let r: Option<Row> = sqlx::query_as(
        "SELECT n.account_id::bigint, n.user_id::bigint, n.notification_type::bigint, c.display_id::bigint,
                u.email, COALESCE(NULLIF(u.display_name, ''), u.name), u.confirmed_at, n.read_at,
                COALESCE(ns.email_flags, 0)::bigint, i.name, n.secondary_actor_id::bigint
         FROM notifications n JOIN users u ON u.id = n.user_id JOIN conversations c ON c.id = n.primary_actor_id
         LEFT JOIN inboxes i ON i.id = c.inbox_id
         LEFT JOIN notification_settings ns ON ns.user_id = n.user_id AND ns.account_id = n.account_id
         WHERE n.id = $1 AND n.primary_actor_type = 'Conversation'",
    )
    .bind(id)
    .fetch_optional(&st.db)
    .await?;
    let Some((account_id, user_id, kind, display_id, to, name, confirmed, read, flags, inbox, message_id)) = r else { return Ok(()) };
    // has_flags bit_operator: notification type N maps to bit (N - 1)
    if read.is_some() || confirmed.is_none() || flags & (1 << (kind - 1)) == 0 {
        return Ok(());
    }
    // assigned/participating new message: skip when the agent is online
    if (kind == 3 || kind == 5) && crate::cable::presence_snapshot().is_online(account_id, user_id) {
        return Ok(());
    }
    let url = format!("{}/app/accounts/{account_id}/conversations/{display_id}", st.frontend_url.trim_end_matches('/'));
    let link = format!("<a href=\"{url}\">");
    let subject_name = name.clone();
    let name = html_escape(&name); // body only; subjects are plain text
    let (subject, body) = match kind {
        1 => (
            format!("{subject_name}, A new conversation [ID - {display_id}] has been created in {}.", inbox.clone().unwrap_or_default()),
            format!("<p>Hi {name}</p><p>A new conversation ({link}#{display_id}</a>) has been created in {}.</p>", html_escape(&inbox.unwrap_or_default())),
        ),
        2 => (
            format!("{subject_name}, A new conversation [ID - {display_id}] has been assigned to you."),
            format!("<p>Hi {name},</p><p>Time to save the world. A new conversation has been assigned to you</p><p>Click {link}here</a> to get cracking.</p>"),
        ),
        3 => (
            format!("{subject_name}, New message in your assigned conversation [ID - {display_id}]."),
            format!("<p>Hi {name},</p><p>You have received a new message in your assigned conversation.</p><p>Click {link}here</a> to get cracking.</p>"),
        ),
        4 => {
            let text: Option<String> = match message_id {
                Some(m) => sqlx::query_scalar("SELECT content FROM messages WHERE id = $1").bind(m).fetch_optional(&st.db).await?.flatten(),
                None => None,
            };
            (
                format!("{subject_name}, You have been mentioned in conversation [ID - {display_id}]"),
                format!(
                    "<p>Hi {name}, </p><p>You've been mentioned in a conversation.</p><blockquote style=\"padding:8px 16px;border-left:5px solid #eeeeee\">{}</blockquote><p>{link}View Message</a></p>",
                    html_escape(&text.unwrap_or_default())
                ),
            )
        }
        5 => (
            format!("{subject_name}, New message in your participating conversation [ID - {display_id}]."),
            format!("<p>Hi {name},</p><p>You have received a new message in a conversation you are participating.</p><p>Click {link}here</a> to view.</p>"),
        ),
        _ => return Ok(()),
    };
    crate::mailer::send_html(&to, &subject, body).await
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// ActionCableListener#notification_created: `notification.created` to the user's pubsub token.
async fn broadcast(st: &AppState, account_id: i64, user_id: i64, id: i64) -> sqlx::Result<()> {
    let db = &st.db;
    let row: NotifRow = sqlx::query_as(
        "SELECT n.notification_type, n.primary_actor_id, n.created_at, n.last_activity_at, u.name, u.pubsub_token
         FROM notifications n JOIN users u ON u.id = n.user_id WHERE n.id = $1::bigint",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    let (unread, count): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*) FILTER (WHERE read_at IS NULL), COUNT(*) FROM notifications
         WHERE account_id = $1::bigint AND user_id = $2::bigint",
    )
    .bind(account_id)
    .bind(user_id)
    .fetch_one(db)
    .await?;
    let Some(token) = row.pubsub_token else { return Ok(()) };
    let type_name = TYPE_NAMES.get((row.notification_type - 1).max(0) as usize).copied().unwrap_or(TYPE_NAMES[0]);
    let notification: Value = json!({
        "id": id, "notification_type": type_name, "push_message_title": "", "push_message_body": "",
        "primary_actor_type": "Conversation", "primary_actor_id": row.primary_actor_id,
        "primary_actor": { "id": row.primary_actor_id }, "read_at": null, "secondary_actor": null,
        "user": { "id": user_id, "name": row.name, "type": "user" },
        "created_at": row.created_at.and_utc().timestamp(),
        "last_activity_at": row.last_activity_at.map(|t| t.and_utc().timestamp()).unwrap_or(0),
        "snoozed_until": null, "meta": {},
    });
    st.hub.broadcast(&[token], "notification.created", json!({ "notification": notification, "unread_count": unread, "count": count }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_user_and_team_mentions() {
        assert_eq!(mention_ids("hi (mention://user/12/Ann) and (mention://user/7/Bo)", "user"), vec![12, 7]);
        assert_eq!(mention_ids("(mention://team/3/Support)", "team"), vec![3]);
        assert!(mention_ids("(mention://user/9)", "user").is_empty());
    }
}
