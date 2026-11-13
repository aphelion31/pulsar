//! Event dispatcher, the Rust side of Chatwoot's `Dispatcher` + `AsyncDispatcher` (app/dispatchers).
//!
//! Handlers call `events::publish(st, name, payload)` after a write commits. Realtime (ActionCable) stays
//! synchronous in the handlers as before. Everything Chatwoot runs from AsyncDispatcher listeners (webhooks,
//! automation rules, notifications, hooks, CSAT, campaigns, reporting events) subscribes here and runs on
//! a background task, so a slow webhook never delays the HTTP response, like Sidekiq in Chatwoot.
//!
//! Event names are Chatwoot's `Events::Types` strings (lib/events/types.rb), e.g. `conversation.created`.
//! Durability: every event is written to the `pulsar_events` outbox before dispatch and deleted after all
//! listeners finished, so events in flight survive a restart (at-least-once, like Sidekiq retries).
//! Rows left by a crash are replayed on startup; after MAX_ATTEMPTS they are dropped with an error log.

use serde_json::Value;
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::common::AppState;

const MAX_ATTEMPTS: i32 = 3;

#[derive(Clone, Debug)]
pub struct Event {
    pub name: String,
    pub account_id: i64,
    /// Chatwoot webhook-shaped payload (conversation / message / contact push_event_data).
    pub data: Value,
    /// Changed attributes as `{attr: {previous_value, current_value}}` when relevant.
    pub changed: Option<Value>,
}

type Handler = Arc<dyn Fn(AppState, Event) -> futures::future::BoxFuture<'static, ()> + Send + Sync>;

static TX: OnceLock<UnboundedSender<Event>> = OnceLock::new();

/// Start the background worker with the given listeners. Call once at startup.
pub fn start(st: AppState, listeners: Vec<(&'static str, Handler)>) {
    let (tx, mut rx) = unbounded_channel::<Event>();
    if TX.set(tx).is_err() {
        return;
    }
    // Dispatch queue of (outbox row id, event); row id 0 = not persisted (DB write failed), still dispatched.
    let (out_tx, mut out_rx) = unbounded_channel::<(i64, Event)>();

    // Persist in publish order on one task, then hand off to the dispatcher. Replay leftovers first.
    let db = st.db.clone();
    tokio::spawn(async move {
        replay(&db, &out_tx).await;
        while let Some(ev) = rx.recv().await {
            let id: i64 = sqlx::query_scalar("INSERT INTO pulsar_events (name, account_id, data, changed) VALUES ($1, $2, $3, $4) RETURNING id")
                .bind(&ev.name)
                .bind(ev.account_id)
                .bind(&ev.data)
                .bind(&ev.changed)
                .fetch_one(&db)
                .await
                .unwrap_or_else(|e| {
                    tracing::warn!("event outbox write failed, dispatching without persistence: {e}");
                    0
                });
            let _ = out_tx.send((id, ev));
        }
    });

    tokio::spawn(async move {
        while let Some((id, ev)) = out_rx.recv().await {
            let mut tasks = Vec::with_capacity(listeners.len());
            for (name, h) in &listeners {
                // Each listener runs in its own task: one failing or slow listener does not block the others.
                let (st, ev, h, name) = (st.clone(), ev.clone(), h.clone(), *name);
                tasks.push(tokio::spawn(async move {
                    let task = tokio::spawn(h(st, ev));
                    if let Err(e) = task.await {
                        tracing::error!("listener {name} panicked: {e}");
                    }
                }));
            }
            if id > 0 {
                let db = st.db.clone();
                tokio::spawn(async move {
                    futures::future::join_all(tasks).await;
                    let _ = sqlx::query("DELETE FROM pulsar_events WHERE id = $1").bind(id).execute(&db).await;
                });
            }
        }
    });
}

/// Re-dispatch events a previous process left behind (crash or restart mid-delivery).
async fn replay(db: &sqlx::PgPool, out: &UnboundedSender<(i64, Event)>) {
    let dropped = sqlx::query("DELETE FROM pulsar_events WHERE attempts >= $1").bind(MAX_ATTEMPTS).execute(db).await.map(|r| r.rows_affected()).unwrap_or(0);
    if dropped > 0 {
        tracing::error!(dropped, "dropped events that failed to complete {MAX_ATTEMPTS} times");
    }
    let mut rows: Vec<(i64, String, i64, Value, Option<Value>)> = sqlx::query_as(
        "UPDATE pulsar_events SET attempts = attempts + 1 RETURNING id, name, account_id, data, changed",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();
    rows.sort_by_key(|r| r.0);
    if !rows.is_empty() {
        tracing::info!(count = rows.len(), "replaying undelivered events");
    }
    for (id, name, account_id, data, changed) in rows {
        let _ = out.send((id, Event { name, account_id, data, changed }));
    }
}

/// Queue an event for the async listeners. Never blocks, never fails the caller.
pub fn publish(name: &str, account_id: i64, data: Value, changed: Option<Value>) {
    if let Some(tx) = TX.get() {
        let _ = tx.send(Event { name: name.to_string(), account_id, data, changed });
    }
}

/// Wrap an async fn as a listener.
pub fn listener<F, Fut>(f: F) -> Handler
where
    F: Fn(AppState, Event) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    Arc::new(move |st, ev| Box::pin(f(st, ev)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_without_start_is_a_noop() {
        publish("conversation.created", 1, serde_json::json!({}), None);
    }
}
