//! Event dispatcher, the Rust side of Chatwoot's `Dispatcher` + `AsyncDispatcher` (app/dispatchers).
//!
//! Handlers call `events::publish(st, name, payload)` after a write commits. Realtime (ActionCable) stays
//! synchronous in the handlers as before. Everything Chatwoot runs from AsyncDispatcher listeners (webhooks,
//! automation rules, notifications, hooks, CSAT, campaigns, reporting events) subscribes here and runs on
//! a background task, so a slow webhook never delays the HTTP response, like Sidekiq in Chatwoot.
//!
//! Event names are Chatwoot's `Events::Types` strings (lib/events/types.rb), e.g. `conversation.created`.
//! ponytail: in-memory queue, events in flight are lost on restart. Chatwoot uses Sidekiq/Redis; add a
//! `pulsar_events` outbox table if at-least-once delivery is required.

use serde_json::Value;
use std::sync::{Arc, OnceLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::common::AppState;

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
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            for (name, h) in &listeners {
                // Each listener runs in its own task: one failing or slow listener does not block the others.
                let (st, ev, h, name) = (st.clone(), ev.clone(), h.clone(), *name);
                tokio::spawn(async move {
                    let task = tokio::spawn(h(st, ev));
                    if let Err(e) = task.await {
                        tracing::error!("listener {name} panicked: {e}");
                    }
                });
            }
        }
    });
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
