//! ActionCable-compatible websocket at /cable. In-process pub/sub (no Redis).
//! ponytail: single-node only. Add Redis/PG LISTEN fanout when running >1 instance.
use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sqlx::PgPool;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::common::AppState;

type Tx = UnboundedSender<String>;

#[derive(Clone, Default)]
pub struct Hub {
    /// stream name -> list of (conn id, sender, identifier json)
    streams: Arc<Mutex<HashMap<String, Vec<(u64, Tx, String)>>>>,
}

/// (account_id, user_id) -> last heartbeat. Contacts: (account_id, -contact_id).
static PRESENCE: LazyLock<Mutex<HashMap<(i64, i64), Instant>>> = LazyLock::new(Default::default);
const PRESENCE_TTL: Duration = Duration::from_secs(20);
const CONTACT_PRESENCE_TTL: Duration = Duration::from_secs(90);

/// account_id -> model -> cache key (epoch ms). Bumped by `Hub::invalidate_cache`.
static CACHE_KEYS: LazyLock<Mutex<HashMap<i64, HashMap<String, i64>>>> = LazyLock::new(Default::default);
static BOOT_MS: LazyLock<i64> = LazyLock::new(|| chrono::Utc::now().timestamp_millis());

pub fn cache_keys(account_id: i64) -> Value {
    let m = CACHE_KEYS.lock().unwrap();
    let get = |k: &str| m.get(&account_id).and_then(|h| h.get(k)).copied().unwrap_or(*BOOT_MS);
    json!({ "label": get("label"), "inbox": get("inbox"), "team": get("team") })
}

pub struct PresenceSnapshot(HashMap<(i64, i64), Instant>);
impl PresenceSnapshot {
    pub fn is_online(&self, account_id: i64, user_id: i64) -> bool {
        self.0.get(&(account_id, user_id)).is_some_and(|t| t.elapsed() < PRESENCE_TTL)
    }
    /// Contacts with a live widget session (OnlineStatusTracker.get_available_contact_ids). Contacts are keyed by negative id.
    pub fn online_contact_ids(&self, account_id: i64) -> Vec<i64> {
        self.0.iter().filter(|((a, id), t)| *a == account_id && *id < 0 && t.elapsed() < CONTACT_PRESENCE_TTL).map(|((_, id), _)| -id).collect()
    }
}
pub fn presence_snapshot() -> PresenceSnapshot {
    PresenceSnapshot(PRESENCE.lock().unwrap().clone())
}

impl Hub {
    /// Send `{event, data}` to every subscriber of each stream (pubsub tokens or `account_{id}`).
    pub fn broadcast(&self, streams: &[String], event: &str, data: Value) {
        let map = self.streams.lock().unwrap();
        let mut seen = std::collections::HashSet::new();
        for s in streams {
            for (cid, tx, ident) in map.get(s).into_iter().flatten() {
                if seen.insert((*cid, ident.clone())) {
                    let msg = json!({ "identifier": ident, "message": { "event": event, "data": data } });
                    let _ = tx.send(msg.to_string());
                }
            }
        }
    }

    /// Broadcast to all agents of an account (stream `account_{id}`).
    pub fn to_account(&self, account_id: i64, event: &str, data: Value) {
        // contact.*, inbox.*, notification.* etc. also feed the async listeners (webhooks, automation).
        if !event.starts_with("account.") && !event.starts_with("presence.") {
            crate::events::publish(event, account_id, data.clone(), None);
        }
        self.broadcast(&[format!("account_{account_id}")], event, data);
    }

    /// Bump a cacheable model key (label|inbox|team) and notify dashboards.
    pub fn invalidate_cache(&self, account_id: i64, model: &str) {
        CACHE_KEYS
            .lock()
            .unwrap()
            .entry(account_id)
            .or_default()
            .insert(model.to_string(), chrono::Utc::now().timestamp_millis());
        self.to_account(account_id, "account.cache_invalidated", json!({ "cache_keys": cache_keys(account_id) }));
    }

    /// Pubsub tokens of agents that are members of the inbox, plus account admins.
    pub async fn inbox_tokens(&self, db: &PgPool, account_id: i64, inbox_id: i64) -> Vec<String> {
        sqlx::query_scalar(
            "SELECT DISTINCT u.pubsub_token FROM users u
             WHERE u.pubsub_token IS NOT NULL AND (
               u.id IN (SELECT user_id FROM inbox_members WHERE inbox_id = $2)
               OR u.id IN (SELECT user_id FROM account_users WHERE account_id = $1 AND role = 1))",
        )
        .bind(account_id)
        .bind(inbox_id)
        .fetch_all(db)
        .await
        .unwrap_or_default()
    }

    /// Conversation/message event to inbox agents + the contact's widget session.
    pub async fn to_inbox(&self, db: &PgPool, account_id: i64, inbox_id: i64, contact_inbox_id: Option<i64>, event: &str, data: Value) {
        let mut tokens = self.inbox_tokens(db, account_id, inbox_id).await;
        if let Some(ci) = contact_inbox_id {
            if let Ok(Some(t)) = sqlx::query_scalar::<_, Option<String>>("SELECT pubsub_token FROM contact_inboxes WHERE id=$1")
                .bind(ci)
                .fetch_one(db)
                .await
            {
                tokens.push(t);
            }
        }
        self.broadcast(&tokens, event, data);
    }

    pub async fn presence_payload(db: &PgPool, account_id: i64) -> Value {
        let snap = presence_snapshot();
        let rows: Vec<(i64, i32)> = sqlx::query_as(
            "SELECT user_id, availability FROM account_users WHERE account_id = $1",
        )
        .bind(account_id)
        .fetch_all(db)
        .await
        .unwrap_or_default();
        let users: serde_json::Map<String, Value> = rows
            .into_iter()
            .filter(|(uid, _)| snap.is_online(account_id, *uid))
            .map(|(uid, a)| (uid.to_string(), json!(crate::auth::availability_name(a))))
            .collect();
        let contacts: serde_json::Map<String, Value> = snap
            .0
            .iter()
            .filter(|((a, id), t)| *a == account_id && *id < 0 && t.elapsed() < CONTACT_PRESENCE_TTL)
            .map(|((_, id), _)| ((-id).to_string(), json!("online")))
            .collect();
        json!({ "account_id": account_id, "users": users, "contacts": contacts })
    }

    pub async fn broadcast_presence(&self, db: &PgPool, account_id: i64) {
        let data = Self::presence_payload(db, account_id).await;
        self.to_account(account_id, "presence.update", data);
    }

    fn subscribe(&self, stream: String, cid: u64, tx: Tx, ident: String) {
        self.streams.lock().unwrap().entry(stream).or_default().push((cid, tx, ident));
    }

    fn drop_conn(&self, cid: u64) {
        let mut m = self.streams.lock().unwrap();
        m.values_mut().for_each(|v| v.retain(|(c, _, _)| *c != cid));
        m.retain(|_, v| !v.is_empty());
    }
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub async fn ws_handler(State(st): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.protocols(["actioncable-v1-json", "actioncable-unsupported"])
        .on_upgrade(move |socket| conn(st, socket))
}

/// Resolved subscriber: an agent (user) or a widget contact.
enum Who {
    User { id: i64, account: Option<i64> },
    Contact { id: i64, account: i64 },
}

async fn conn(st: AppState, socket: WebSocket) {
    let cid = NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = unbounded_channel::<String>();
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if sink.send(Message::Text(m.into())).await.is_err() {
                break;
            }
        }
    });
    let _ = tx.send(json!({ "type": "welcome" }).to_string());
    let ping_tx = tx.clone();
    let pinger = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            if ping_tx.send(json!({ "type": "ping", "message": chrono::Utc::now().timestamp() }).to_string()).is_err() {
                break;
            }
        }
    });

    let mut subs: HashMap<String, Who> = HashMap::new();
    while let Some(Ok(msg)) = stream.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        let ident = v["identifier"].as_str().unwrap_or_default().to_string();
        let params: Value = serde_json::from_str(&ident).unwrap_or_default();
        match v["command"].as_str() {
            Some("subscribe") => match resolve(&st.db, &params).await {
                Some(who) => {
                    let token = params["pubsub_token"].as_str().unwrap_or_default().to_string();
                    st.hub.subscribe(token, cid, tx.clone(), ident.clone());
                    if let Who::User { account: Some(a), .. } = who {
                        st.hub.subscribe(format!("account_{a}"), cid, tx.clone(), ident.clone());
                    }
                    let _ = tx.send(json!({ "identifier": ident, "type": "confirm_subscription" }).to_string());
                    heartbeat(&st, &who, &params).await;
                    subs.insert(ident, who);
                }
                None => {
                    let _ = tx.send(json!({ "identifier": ident, "type": "reject_subscription" }).to_string());
                }
            },
            Some("message") => {
                if let Some(who) = subs.get(&ident) {
                    let data: Value = serde_json::from_str(v["data"].as_str().unwrap_or("{}")).unwrap_or_default();
                    if data["action"] == "update_presence" {
                        heartbeat(&st, who, &params).await;
                    }
                }
            }
            Some("unsubscribe") => {
                subs.remove(&ident);
            }
            _ => {}
        }
    }
    st.hub.drop_conn(cid);
    pinger.abort();
    writer.abort();
}

async fn resolve(db: &PgPool, p: &Value) -> Option<Who> {
    let token = p["pubsub_token"].as_str()?;
    let as_i64 = |v: &Value| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()));
    if let Some(uid) = as_i64(&p["user_id"]) {
        let ok: Option<i64> = sqlx::query_scalar("SELECT id::bigint FROM users WHERE pubsub_token=$1 AND id=$2")
            .bind(token)
            .bind(uid)
            .fetch_optional(db)
            .await
            .ok()??;
        ok?;
        let account = match as_i64(&p["account_id"]) {
            Some(a) => sqlx::query_scalar::<_, i64>("SELECT account_id FROM account_users WHERE user_id=$1 AND account_id=$2")
                .bind(uid)
                .bind(a)
                .fetch_optional(db)
                .await
                .ok()?,
            None => None,
        };
        Some(Who::User { id: uid, account })
    } else {
        let (cid, account): (i64, i64) = sqlx::query_as(
            "SELECT c.id::bigint, c.account_id::bigint FROM contact_inboxes ci JOIN contacts c ON c.id = ci.contact_id WHERE ci.pubsub_token=$1",
        )
        .bind(token)
        .fetch_optional(db)
        .await
        .ok()??;
        Some(Who::Contact { id: cid, account })
    }
}

async fn heartbeat(st: &AppState, who: &Who, params: &Value) {
    let (key, account) = match who {
        Who::User { id, account: Some(a) } => ((*a, *id), *a),
        Who::Contact { id, account } => ((*account, -*id), *account),
        _ => return,
    };
    PRESENCE.lock().unwrap().insert(key, Instant::now());
    let mut data = Hub::presence_payload(&st.db, account).await;
    if let Who::User { id, .. } = who {
        let wp: Option<(bool, bool)> = sqlx::query_as(&format!(
            "SELECT {} FROM account_users au WHERE user_id=$1 AND account_id=$2",
            crate::common::work_pref_cols("au")
        ))
        .bind(id)
        .bind(account)
        .fetch_optional(&st.db)
        .await
        .ok()
        .flatten();
        if let Some((c, k)) = wp {
            data["work_preferences"] = json!({ "accepts_conversations": c, "accepts_calls": k });
        }
    } else {
        data.as_object_mut().unwrap().remove("contacts");
    }
    let token = params["pubsub_token"].as_str().unwrap_or_default().to_string();
    st.hub.broadcast(&[token], "presence.update", data);
}
