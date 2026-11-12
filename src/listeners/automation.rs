//! Automation rule listener, port of Chatwoot's AutomationRuleListener and
//! related services (ConditionsFilterService, ActionService).
//!
//! Subscribes to events from `events::publish`, evaluates active automation rules
//! for the account and event name, and executes rule actions. Includes a loop
//! guard to prevent recursive automation execution.

use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use serde_json::{json, Value};
use sqlx::PgPool;

use crate::{
    common::AppState,
    conversations::{conv_json, emit, insert_message, reload, Conv},
    events::Event,
};

#[derive(sqlx::FromRow, Clone, Debug)]
#[allow(dead_code)]
struct RuleRow {
    id: i64,
    account_id: i64,
    name: String,
    event_name: String,
    conditions: Value,
    actions: Value,
}

#[derive(sqlx::FromRow, Clone, Debug)]
#[allow(dead_code)]
struct ContactRow {
    id: i64,
    name: Option<String>,
    email: Option<String>,
    phone_number: Option<String>,
    identifier: Option<String>,
    blocked: Option<bool>,
    additional_attributes: Option<Value>,
    custom_attributes: Option<Value>,
}

#[derive(sqlx::FromRow, Clone, Debug)]
#[allow(dead_code)]
struct MsgRow {
    id: i64,
    content: Option<String>,
    account_id: i64,
    inbox_id: i64,
    conversation_id: i64,
    message_type: i32,
    private: bool,
    processed_message_content: Option<String>,
    content_attributes: Option<Value>,
    additional_attributes: Option<Value>,
}

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

/// Loop guard: returns true if the event was triggered by an automation rule
/// or should otherwise be ignored (activity messages, auto replies).
fn performed_by_automation(ev: &Event) -> bool {
    if let Some(pb) = ev.data.get("performed_by") {
        if !pb.is_null() {
            return true;
        }
    }
    if ev
        .data
        .get("performed_by_automation")
        .and_then(Value::as_bool)
        == Some(true)
    {
        return true;
    }
    if ev
        .data
        .pointer("/content_attributes/automation_rule_id")
        .is_some()
    {
        return true;
    }
    if ev.data.get("automation_rule_id").is_some() {
        return true;
    }

    let event = ev.name.replace('.', "_");
    if event == "message_created" {
        // Chatwoot: message.activity? || message.auto_reply_email?
        if ev.data.get("message_type").and_then(Value::as_i64) == Some(2) {
            return true;
        }
        if ev
            .data
            .pointer("/content_attributes/email/auto_reply")
            .and_then(Value::as_bool)
            == Some(true)
        {
            return true;
        }
    }

    if matches!(
        event.as_str(),
        "conversation_created" | "conversation_opened"
    ) {
        // Chatwoot: auto_reply_skip_events.include?(event_name) && ignore_auto_reply_event?(event)
        if ev
            .data
            .pointer("/additional_attributes/auto_reply")
            .is_some()
        {
            return true;
        }
    }

    false
}

pub async fn handle(st: AppState, ev: Event) {
    if let Err(e) = process(&st, &ev).await {
        tracing::warn!(error = %e, event = %ev.name, "automation listener failed");
    }
}

async fn get_conv_id(db: &PgPool, account_id: i64, data: &Value) -> Option<i64> {
    if let Some(conv_id) = data.pointer("/conversation/id").and_then(Value::as_i64) {
        // Could be display_id or id
        if let Ok(Some(id)) = sqlx::query_scalar::<_, i64>(
            "SELECT id::bigint FROM conversations WHERE account_id = $1 AND (display_id = $2 OR id = $2) LIMIT 1",
        )
        .bind(account_id)
        .bind(conv_id)
        .fetch_optional(db)
        .await
        {
            return Some(id);
        }
    }

    if let Some(conv_id) = data.get("id").and_then(Value::as_i64) {
        if let Ok(Some(id)) = sqlx::query_scalar::<_, i64>(
            "SELECT id::bigint FROM conversations WHERE account_id = $1 AND (display_id = $2 OR id = $2) LIMIT 1",
        )
        .bind(account_id)
        .bind(conv_id)
        .fetch_optional(db)
        .await
        {
            return Some(id);
        }
    }

    None
}

async fn load_contact(db: &PgPool, contact_id: Option<i64>) -> Option<ContactRow> {
    let cid = contact_id?;
    sqlx::query_as::<_, ContactRow>(
        "SELECT id::bigint AS id, name, email, phone_number, identifier, blocked, additional_attributes, custom_attributes FROM contacts WHERE id = $1",
    )
    .bind(cid)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}

async fn process(st: &AppState, ev: &Event) -> anyhow::Result<()> {
    if performed_by_automation(ev) {
        return Ok(());
    }

    let event = ev.name.replace('.', "_");
    if event == "message_created" {
        let msg_id = ev
            .data
            .get("id")
            .and_then(Value::as_i64)
            .ok_or_else(|| anyhow::anyhow!("missing message id"))?;

        let msg: Option<MsgRow> = sqlx::query_as(
            "SELECT id::bigint AS id, content, account_id::bigint AS account_id, inbox_id::bigint AS inbox_id,
                    conversation_id::bigint AS conversation_id, message_type, private,
                    processed_message_content, content_attributes::jsonb AS content_attributes, additional_attributes
             FROM messages WHERE id = $1 AND account_id = $2",
        )
        .bind(msg_id)
        .bind(ev.account_id)
        .fetch_optional(&st.db)
        .await?;

        let Some(msg) = msg else { return Ok(()) };

        // Activity message or auto-reply guard
        if msg.message_type == 2 {
            return Ok(());
        }
        if msg
            .content_attributes
            .as_ref()
            .and_then(|c| c.pointer("/email/auto_reply"))
            .and_then(Value::as_bool)
            == Some(true)
        {
            return Ok(());
        }

        let mut conv = match reload(&st.db, msg.conversation_id).await {
            Ok(c) => c,
            Err(_) => return Ok(()),
        };

        let contact = load_contact(&st.db, conv.contact_id).await;

        let rules: Vec<RuleRow> = sqlx::query_as(
            "SELECT id, account_id, name, event_name, conditions, actions
             FROM automation_rules
             WHERE account_id = $1 AND event_name = 'message_created' AND active = true AND execution_delay IS NULL
             ORDER BY id ASC",
        )
        .bind(ev.account_id)
        .fetch_all(&st.db)
        .await?;

        for rule in rules {
            let conds = rule.conditions.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            if matches_conditions(conds, &conv, contact.as_ref(), Some(&msg), ev.changed.as_ref()) {
                execute_rule(st, &rule, &mut conv, Some(&msg)).await?;
            }
        }

        return Ok(());
    }

    let is_conv_event = matches!(
        event.as_str(),
        "conversation_created"
            | "conversation_updated"
            | "conversation_opened"
            | "conversation_resolved"
            | "conversation_status_changed"
    );

    if !is_conv_event {
        return Ok(());
    }

    let Some(conv_id) = get_conv_id(&st.db, ev.account_id, &ev.data).await else {
        return Ok(());
    };

    let mut conv = match reload(&st.db, conv_id).await {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    let contact = load_contact(&st.db, conv.contact_id).await;

    let target_events: Vec<&str> = match event.as_str() {
        "conversation_created" => vec!["conversation_created"],
        "conversation_updated" => vec!["conversation_updated"],
        "conversation_opened" => vec!["conversation_opened"],
        "conversation_resolved" => vec!["conversation_resolved"],
        "conversation_status_changed" => {
            let mut list = vec!["conversation_updated"];
            if conv.status == 0 {
                list.push("conversation_opened");
            } else if conv.status == 1 {
                list.push("conversation_resolved");
            }
            list
        }
        _ => vec![],
    };

    for target_event in target_events {
        let rules: Vec<RuleRow> = sqlx::query_as(
            "SELECT id, account_id, name, event_name, conditions, actions
             FROM automation_rules
             WHERE account_id = $1 AND event_name = $2 AND active = true AND execution_delay IS NULL
             ORDER BY id ASC",
        )
        .bind(ev.account_id)
        .bind(target_event)
        .fetch_all(&st.db)
        .await?;

        for rule in rules {
            let conds = rule.conditions.as_array().map(|a| a.as_slice()).unwrap_or(&[]);
            if matches_conditions(conds, &conv, contact.as_ref(), None, ev.changed.as_ref()) {
                execute_rule(st, &rule, &mut conv, None).await?;
            }
        }
    }

    Ok(())
}

// ---------- Conditions filtering ----------

fn values_to_strings(values: &Value) -> Vec<String> {
    if let Some(arr) = values.as_array() {
        arr.iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None,
            })
            .collect()
    } else if let Some(s) = values.as_str() {
        vec![s.to_string()]
    } else if let Some(n) = values.as_i64() {
        vec![n.to_string()]
    } else if let Some(b) = values.as_bool() {
        vec![b.to_string()]
    } else {
        vec![]
    }
}

fn status_name(status: i32) -> &'static str {
    match status {
        0 => "open",
        1 => "resolved",
        2 => "pending",
        3 => "snoozed",
        _ => "open",
    }
}

fn priority_name(priority: Option<i32>) -> &'static str {
    match priority {
        Some(0) => "low",
        Some(1) => "medium",
        Some(2) => "high",
        Some(3) => "urgent",
        _ => "nil",
    }
}

fn message_type_name(message_type: i32) -> &'static str {
    match message_type {
        0 => "incoming",
        1 => "outgoing",
        2 => "activity",
        3 => "template",
        _ => "incoming",
    }
}

fn parse_date(s: &str) -> Option<NaiveDateTime> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.naive_utc());
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return d.and_hms_opt(0, 0, 0);
    }
    if let Ok(epoch) = s.parse::<i64>() {
        return DateTime::from_timestamp(epoch, 0).map(|d| d.naive_utc());
    }
    None
}

fn evaluate_condition(
    cond: &Value,
    conv: &Conv,
    contact: Option<&ContactRow>,
    msg: Option<&MsgRow>,
    changed_attributes: Option<&Value>,
) -> bool {
    let key = match cond.get("attribute_key").and_then(Value::as_str) {
        Some(k) => k,
        None => return false,
    };
    let op = match cond.get("filter_operator").and_then(Value::as_str) {
        Some(o) => o,
        None => return false,
    };
    let raw_values = cond.get("values").unwrap_or(&Value::Null);

    // Special case: attribute_changed
    if op == "attribute_changed" {
        let changed = match changed_attributes.and_then(|c| c.get(key)) {
            Some(v) => v,
            None => return false,
        };
        let from_list = values_to_strings(raw_values.get("from").unwrap_or(&Value::Null));
        let to_list = values_to_strings(raw_values.get("to").unwrap_or(&Value::Null));
        let (ch_from, ch_to) = match changed {
            Value::Array(a) if a.len() >= 2 => {
                let f = a[0].as_str().map(|s| s.to_string()).or_else(|| a[0].as_i64().map(|n| n.to_string())).unwrap_or_default();
                let t = a[1].as_str().map(|s| s.to_string()).or_else(|| a[1].as_i64().map(|n| n.to_string())).unwrap_or_default();
                (f, t)
            }
            _ => return false,
        };
        let from_ok = from_list.is_empty() || from_list.iter().any(|s| s.eq_ignore_ascii_case(&ch_from));
        let to_ok = to_list.is_empty() || to_list.iter().any(|s| s.eq_ignore_ascii_case(&ch_to));
        return from_ok && to_ok;
    }

    let values = values_to_strings(raw_values);

    // 1. Message attributes
    if let Some(m) = msg {
        if key == "content" {
            let body = m
                .processed_message_content
                .as_deref()
                .or(m.content.as_deref())
                .unwrap_or("")
                .to_lowercase();
            return match op {
                "equal_to" => values.iter().any(|v| body == v.to_lowercase()),
                "not_equal_to" => !values.iter().any(|v| body == v.to_lowercase()),
                "contains" => values.iter().any(|v| body.contains(&v.to_lowercase())),
                "does_not_contain" => !values.iter().any(|v| body.contains(&v.to_lowercase())),
                _ => false,
            };
        }
        if key == "message_type" {
            let mname = message_type_name(m.message_type);
            let mtype_str = m.message_type.to_string();
            let matches = values.iter().any(|v| v.eq_ignore_ascii_case(mname) || *v == mtype_str);
            return match op {
                "equal_to" => matches,
                "not_equal_to" => !matches,
                _ => false,
            };
        }
        if key == "private_note" {
            let is_true = values.iter().any(|v| v.eq_ignore_ascii_case("true") || v == "1");
            let is_false = values.iter().any(|v| v.eq_ignore_ascii_case("false") || v == "0");
            let matches = (m.private && is_true) || (!m.private && is_false);
            return match op {
                "equal_to" => matches,
                "not_equal_to" => !matches,
                _ => false,
            };
        }
    }

    // 2. Conversation labels
    if key == "labels" {
        let labels = conv.labels();
        return match op {
            "equal_to" => values
                .iter()
                .any(|v| labels.iter().any(|l| l.eq_ignore_ascii_case(v))),
            "not_equal_to" => !values
                .iter()
                .any(|v| labels.iter().any(|l| l.eq_ignore_ascii_case(v))),
            "is_present" => !labels.is_empty(),
            "is_not_present" => labels.is_empty(),
            _ => false,
        };
    }

    // 3. Conversation standard attributes
    if key == "status" {
        let sname = status_name(conv.status);
        let sval = conv.status.to_string();
        let matches = values.iter().any(|v| v.eq_ignore_ascii_case(sname) || *v == sval || v.eq_ignore_ascii_case("all"));
        return match op {
            "equal_to" => matches,
            "not_equal_to" => !matches,
            _ => false,
        };
    }
    if key == "priority" {
        let pname = priority_name(conv.priority);
        let matches = values.iter().any(|v| v.eq_ignore_ascii_case(pname));
        return match op {
            "equal_to" => matches,
            "not_equal_to" => !matches,
            _ => false,
        };
    }
    if key == "assignee_id" {
        return match op {
            "is_present" => conv.assignee_id.is_some(),
            "is_not_present" => conv.assignee_id.is_none(),
            "equal_to" => match conv.assignee_id {
                Some(aid) => values.iter().any(|v| v == &aid.to_string()),
                None => values.iter().any(|v| v == "nil" || v == "0" || v.is_empty()),
            },
            "not_equal_to" => match conv.assignee_id {
                Some(aid) => !values.iter().any(|v| v == &aid.to_string()),
                None => !values.iter().any(|v| v == "nil" || v == "0" || v.is_empty()),
            },
            _ => false,
        };
    }
    if key == "team_id" {
        return match op {
            "is_present" => conv.team_id.is_some(),
            "is_not_present" => conv.team_id.is_none(),
            "equal_to" => match conv.team_id {
                Some(tid) => values.iter().any(|v| v == &tid.to_string()),
                None => values.iter().any(|v| v == "nil" || v == "0" || v.is_empty()),
            },
            "not_equal_to" => match conv.team_id {
                Some(tid) => !values.iter().any(|v| v == &tid.to_string()),
                None => !values.iter().any(|v| v == "nil" || v == "0" || v.is_empty()),
            },
            _ => false,
        };
    }
    if key == "inbox_id" {
        let matches = values.iter().any(|v| v == &conv.inbox_id.to_string());
        return match op {
            "equal_to" => matches,
            "not_equal_to" => !matches,
            "is_present" => true,
            "is_not_present" => false,
            _ => false,
        };
    }
    if key == "contact_id" {
        let matches = conv
            .contact_id
            .map(|cid| values.iter().any(|v| v == &cid.to_string()))
            .unwrap_or(false);
        return match op {
            "equal_to" => matches,
            "not_equal_to" => !matches,
            _ => false,
        };
    }
    if key == "display_id" {
        let did = conv.display_id.to_string();
        return match op {
            "equal_to" => values.iter().any(|v| v == &did),
            "not_equal_to" => !values.iter().any(|v| v == &did),
            "contains" => values.iter().any(|v| did.contains(v)),
            "does_not_contain" => !values.iter().any(|v| did.contains(v)),
            _ => false,
        };
    }
    if key == "created_at" || key == "last_activity_at" {
        let dt = if key == "created_at" {
            conv.created_at
        } else {
            conv.last_activity_at
        };
        let first_val = values.first().cloned().unwrap_or_default();
        if op == "days_before" {
            if let Ok(days) = first_val.parse::<i64>() {
                let target = Utc::now().naive_utc() - chrono::Duration::days(days);
                return dt < target;
            }
        }
        if let Some(target) = parse_date(&first_val) {
            return match op {
                "is_greater_than" => dt > target,
                "is_less_than" => dt < target,
                _ => false,
            };
        }
        return false;
    }

    // 4. Conversation additional attributes
    if matches!(
        key,
        "browser_language" | "conversation_language" | "referer" | "mail_subject"
    ) {
        let field_val = conv
            .additional_attributes
            .as_ref()
            .and_then(|a| a.get(key))
            .and_then(Value::as_str)
            .unwrap_or("");
        let lower = field_val.to_lowercase();
        return match op {
            "equal_to" => values.iter().any(|v| lower == v.to_lowercase()),
            "not_equal_to" => !values.iter().any(|v| lower == v.to_lowercase()),
            "contains" => values.iter().any(|v| lower.contains(&v.to_lowercase())),
            "does_not_contain" => !values.iter().any(|v| lower.contains(&v.to_lowercase())),
            _ => false,
        };
    }

    // 5. Contact attributes
    if let Some(ct) = contact {
        if matches!(key, "name" | "email" | "identifier") {
            let str_val = match key {
                "name" => ct.name.as_deref().unwrap_or(""),
                "email" => ct.email.as_deref().unwrap_or(""),
                "identifier" => ct.identifier.as_deref().unwrap_or(""),
                _ => "",
            }
            .to_lowercase();
            return match op {
                "equal_to" => values.iter().any(|v| str_val == v.to_lowercase()),
                "not_equal_to" => !values.iter().any(|v| str_val == v.to_lowercase()),
                "contains" => values.iter().any(|v| str_val.contains(&v.to_lowercase())),
                "does_not_contain" => !values.iter().any(|v| str_val.contains(&v.to_lowercase())),
                _ => false,
            };
        }
        if key == "phone_number" {
            let phone = ct.phone_number.as_deref().unwrap_or("");
            return match op {
                "equal_to" => values.iter().any(|v| phone == v),
                "not_equal_to" => !values.iter().any(|v| phone == v),
                "contains" => values.iter().any(|v| phone.contains(v)),
                "does_not_contain" => !values.iter().any(|v| phone.contains(v)),
                "starts_with" => values.iter().any(|v| phone.starts_with(v)),
                _ => false,
            };
        }
        if key == "blocked" {
            let is_blocked = ct.blocked.unwrap_or(false);
            let target = values.iter().any(|v| v.eq_ignore_ascii_case("true") || v == "1");
            return match op {
                "equal_to" => is_blocked == target,
                "not_equal_to" => is_blocked != target,
                _ => false,
            };
        }
        if matches!(key, "country_code" | "city" | "company_name") {
            let str_val = ct
                .additional_attributes
                .as_ref()
                .and_then(|a| a.get(key))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_lowercase();
            return match op {
                "equal_to" => values.iter().any(|v| str_val == v.to_lowercase()),
                "not_equal_to" => !values.iter().any(|v| str_val == v.to_lowercase()),
                "contains" => values.iter().any(|v| str_val.contains(&v.to_lowercase())),
                "does_not_contain" => !values.iter().any(|v| str_val.contains(&v.to_lowercase())),
                _ => false,
            };
        }
    }

    // 6. Custom attributes (conversation or contact)
    let is_contact_custom = cond
        .get("custom_attribute_type")
        .and_then(Value::as_str)
        == Some("contact_attribute");
    let custom_attrs = if is_contact_custom {
        contact.and_then(|c| c.custom_attributes.as_ref())
    } else {
        conv.custom_attributes.as_ref()
    };

    if let Some(attrs) = custom_attrs {
        if let Some(val) = attrs.get(key) {
            match op {
                "is_present" => return !val.is_null(),
                "is_not_present" => return val.is_null(),
                _ => {}
            }
            if let Some(val_str) = val.as_str() {
                let lower = val_str.to_lowercase();
                return match op {
                    "equal_to" => values.iter().any(|v| lower == v.to_lowercase()),
                    "not_equal_to" => !values.iter().any(|v| lower == v.to_lowercase()),
                    "contains" => values.iter().any(|v| lower.contains(&v.to_lowercase())),
                    "does_not_contain" => !values.iter().any(|v| lower.contains(&v.to_lowercase())),
                    _ => false,
                };
            }
            if let Some(val_num) = val.as_f64() {
                if let Some(first) = values.first().and_then(|v| v.parse::<f64>().ok()) {
                    return match op {
                        "equal_to" => (val_num - first).abs() < f64::EPSILON,
                        "not_equal_to" => (val_num - first).abs() >= f64::EPSILON,
                        "is_greater_than" => val_num > first,
                        "is_less_than" => val_num < first,
                        _ => false,
                    };
                }
            }
            if let Some(val_bool) = val.as_bool() {
                let target = values.iter().any(|v| v.eq_ignore_ascii_case("true") || v == "1");
                return match op {
                    "equal_to" => val_bool == target,
                    "not_equal_to" => val_bool != target,
                    _ => false,
                };
            }
        } else {
            return match op {
                "is_not_present" => true,
                "not_equal_to" => true,
                _ => false,
            };
        }
    } else {
        return match op {
            "is_not_present" => true,
            "not_equal_to" => true,
            _ => false,
        };
    }

    false
}

fn matches_conditions(
    conditions: &[Value],
    conv: &Conv,
    contact: Option<&ContactRow>,
    msg: Option<&MsgRow>,
    changed_attributes: Option<&Value>,
) -> bool {
    if conditions.is_empty() {
        return true;
    }

    let mut result = evaluate_condition(&conditions[0], conv, contact, msg, changed_attributes);

    for i in 0..conditions.len() - 1 {
        let op = conditions[i]
            .get("query_operator")
            .and_then(Value::as_str)
            .unwrap_or("AND")
            .to_uppercase();
        let next_result =
            evaluate_condition(&conditions[i + 1], conv, contact, msg, changed_attributes);
        if op == "OR" {
            result = result || next_result;
        } else {
            result = result && next_result;
        }
    }

    result
}

// ---------- Actions execution ----------

pub(crate) async fn send_webhook(url_str: &str, payload: &Value) -> anyhow::Result<()> {
    let url = url::Url::parse(url_str)?;
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

    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(host, &addresses)
        .timeout(Duration::from_secs(5))
        .build()?;

    let body = serde_json::to_string(payload)?;
    client
        .post(url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .body(body)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn extract_text_param(params: &Value) -> Option<String> {
    if let Some(arr) = params.as_array() {
        if let Some(first) = arr.first() {
            if let Some(s) = first.as_str() {
                return Some(s.to_string());
            }
            if let Some(obj) = first.as_object() {
                if let Some(m) = obj.get("message").and_then(Value::as_str) {
                    return Some(m.to_string());
                }
            }
        }
    } else if let Some(s) = params.as_str() {
        return Some(s.to_string());
    } else if let Some(obj) = params.as_object() {
        if let Some(m) = obj.get("message").and_then(Value::as_str) {
            return Some(m.to_string());
        }
    }
    None
}

async fn execute_rule(
    st: &AppState,
    rule: &RuleRow,
    conv: &mut Conv,
    _msg: Option<&MsgRow>,
) -> anyhow::Result<()> {
    let empty_actions = vec![];
    let actions = rule.actions.as_array().unwrap_or(&empty_actions);

    for action in actions {
        let name = action.get("action_name").and_then(Value::as_str).unwrap_or("");
        let empty_params = json!([]);
        let params = action.get("action_params").unwrap_or(&empty_params);
        let first_param = params.as_array().and_then(|a| a.first()).unwrap_or(&Value::Null);

        match name {
            "add_label" | "remove_label" => {
                let labels = values_to_strings(params);
                for label in labels {
                    let label = label.trim();
                    if label.is_empty() {
                        continue;
                    }
                    if name == "add_label" {
                        sqlx::query(
                            "INSERT INTO tags (name) VALUES ($1) ON CONFLICT (name) DO NOTHING",
                        )
                        .bind(label)
                        .execute(&st.db)
                        .await?;

                        sqlx::query(
                            "INSERT INTO taggings (tag_id, taggable_type, taggable_id, context, created_at)
                             SELECT t.id, 'Conversation', $1, 'labels', now()
                             FROM tags t
                             WHERE t.name = $2
                               AND NOT EXISTS (
                                 SELECT 1 FROM taggings x
                                 WHERE x.tag_id = t.id AND x.taggable_type = 'Conversation'
                                   AND x.taggable_id = $1 AND x.context = 'labels'
                               )",
                        )
                        .bind(conv.id)
                        .bind(label)
                        .execute(&st.db)
                        .await?;
                    } else {
                        sqlx::query(
                            "DELETE FROM taggings tg USING tags t
                             WHERE tg.taggable_id = $1 AND tg.taggable_type = 'Conversation'
                               AND tg.context = 'labels' AND tg.tag_id = t.id AND t.name = $2",
                        )
                        .bind(conv.id)
                        .bind(label)
                        .execute(&st.db)
                        .await?;
                    }
                }

                sqlx::query(
                    "UPDATE conversations c
                     SET cached_label_list = (
                       SELECT string_agg(t.name, ', ' ORDER BY t.name)
                       FROM taggings tg JOIN tags t ON t.id = tg.tag_id
                       WHERE tg.taggable_type = 'Conversation' AND tg.taggable_id = c.id AND tg.context = 'labels'
                     ), updated_at = now()
                     WHERE c.account_id = $1 AND c.id = $2",
                )
                .bind(conv.account_id)
                .bind(conv.id)
                .execute(&st.db)
                .await?;
            }

            "assign_agent" => {
                let target_agent: Option<i64> = if first_param.is_null()
                    || first_param == "nil"
                    || first_param == "0"
                    || first_param == 0
                {
                    None
                } else if first_param == "last_responding_agent" {
                    sqlx::query_scalar::<_, Option<i64>>(
                        "SELECT sender_id FROM messages
                         WHERE conversation_id = $1 AND message_type = 1 AND NOT private AND sender_type = 'User'
                         ORDER BY id DESC LIMIT 1",
                    )
                    .bind(conv.id)
                    .fetch_optional(&st.db)
                    .await?
                    .flatten()
                } else {
                    first_param
                        .as_i64()
                        .or_else(|| first_param.as_str().and_then(|s| s.parse().ok()))
                };

                if let Some(agent_id) = target_agent {
                    // Check if user belongs to account and either admin or member of inbox
                    let valid: Option<i32> = sqlx::query_scalar(
                        "SELECT 1 FROM account_users au
                         WHERE au.account_id = $1 AND au.user_id = $2
                           AND (au.role = 1 OR EXISTS (SELECT 1 FROM inbox_members im WHERE im.inbox_id = $3 AND im.user_id = $2))",
                    )
                    .bind(conv.account_id)
                    .bind(agent_id)
                    .bind(conv.inbox_id)
                    .fetch_optional(&st.db)
                    .await?;

                    if valid.is_some() {
                        sqlx::query(
                            "UPDATE conversations SET assignee_id = $1, updated_at = now() WHERE account_id = $2 AND id = $3",
                        )
                        .bind(agent_id as i32)
                        .bind(conv.account_id)
                        .bind(conv.id)
                        .execute(&st.db)
                        .await?;
                    }
                } else {
                    sqlx::query(
                        "UPDATE conversations SET assignee_id = NULL, updated_at = now() WHERE account_id = $1 AND id = $2",
                    )
                    .bind(conv.account_id)
                    .bind(conv.id)
                    .execute(&st.db)
                    .await?;
                }
            }

            "assign_team" => {
                let target_team: Option<i64> = if first_param.is_null()
                    || first_param == "nil"
                    || first_param == "0"
                    || first_param == 0
                {
                    None
                } else {
                    first_param
                        .as_i64()
                        .or_else(|| first_param.as_str().and_then(|s| s.parse().ok()))
                };

                if let Some(team_id) = target_team {
                    let valid: Option<i32> = sqlx::query_scalar(
                        "SELECT 1 FROM teams WHERE account_id = $1 AND id = $2",
                    )
                    .bind(conv.account_id)
                    .bind(team_id)
                    .fetch_optional(&st.db)
                    .await?;

                    if valid.is_some() {
                        sqlx::query(
                            "UPDATE conversations SET team_id = $1, updated_at = now() WHERE account_id = $2 AND id = $3",
                        )
                        .bind(team_id)
                        .bind(conv.account_id)
                        .bind(conv.id)
                        .execute(&st.db)
                        .await?;
                    }
                } else {
                    sqlx::query(
                        "UPDATE conversations SET team_id = NULL, updated_at = now() WHERE account_id = $1 AND id = $2",
                    )
                    .bind(conv.account_id)
                    .bind(conv.id)
                    .execute(&st.db)
                    .await?;
                }
            }

            "send_message" | "add_private_note" => {
                if let Some(text) = extract_text_param(params) {
                    if !text.trim().is_empty() {
                        let is_private = name == "add_private_note";
                        let _ = insert_message(
                            st,
                            conv,
                            None,
                            &text,
                            1, // outgoing
                            is_private,
                            0, // text
                            json!({ "automation_rule_id": rule.id }),
                            None,
                        )
                        .await;
                    }
                }
            }

            "send_email_to_team" => {
                let team_ids = if let Some(arr) = params.as_array() {
                    arr.first()
                        .and_then(|o| o.get("team_ids"))
                        .and_then(Value::as_array)
                        .map(|a| a.iter().filter_map(Value::as_i64).collect::<Vec<_>>())
                        .unwrap_or_default()
                } else {
                    vec![]
                };
                let msg_text = extract_text_param(params).unwrap_or_default();
                tracing::info!(
                    rule_id = rule.id,
                    ?team_ids,
                    message = %msg_text,
                    "send_email_to_team triggered (email disabled: logged only)"
                );
            }

            "mute_conversation" => {
                if let Some(cid) = conv.contact_id {
                    sqlx::query("UPDATE contacts SET blocked = true, updated_at = now() WHERE id = $1")
                        .bind(cid)
                        .execute(&st.db)
                        .await?;
                }
                sqlx::query(
                    "UPDATE conversations SET status = 1, status_changed_at = now(), updated_at = now(), waiting_since = NULL WHERE id = $1",
                )
                .bind(conv.id)
                .execute(&st.db)
                .await?;
            }

            "snooze_conversation" => {
                sqlx::query(
                    "UPDATE conversations SET status = 3, status_changed_at = now(), updated_at = now() WHERE id = $1",
                )
                .bind(conv.id)
                .execute(&st.db)
                .await?;
            }

            "resolve_conversation" => {
                sqlx::query(
                    "UPDATE conversations SET status = 1, status_changed_at = now(), updated_at = now(), waiting_since = NULL, first_reply_created_at = COALESCE(first_reply_created_at, now()) WHERE id = $1",
                )
                .bind(conv.id)
                .execute(&st.db)
                .await?;
            }

            "open_conversation" => {
                sqlx::query(
                    "UPDATE conversations SET status = 0, status_changed_at = now(), updated_at = now() WHERE id = $1",
                )
                .bind(conv.id)
                .execute(&st.db)
                .await?;
            }

            "change_priority" => {
                let priority_str = first_param.as_str().unwrap_or("");
                let prio_val = match priority_str {
                    "low" => Some(0),
                    "medium" => Some(1),
                    "high" => Some(2),
                    "urgent" => Some(3),
                    _ => None,
                };
                sqlx::query(
                    "UPDATE conversations SET priority = $1, updated_at = now() WHERE id = $2",
                )
                .bind(prio_val)
                .bind(conv.id)
                .execute(&st.db)
                .await?;
            }

            "change_status" => {
                let status_str = first_param.as_str().unwrap_or("");
                let s_val = match status_str {
                    "open" => 0,
                    "resolved" => 1,
                    "pending" => 2,
                    "snoozed" => 3,
                    _ => 0,
                };
                sqlx::query(
                    "UPDATE conversations SET status = $1, status_changed_at = now(), updated_at = now() WHERE id = $2",
                )
                .bind(s_val)
                .bind(conv.id)
                .execute(&st.db)
                .await?;
            }

            "send_webhook_event" => {
                let url_str = first_param
                    .as_str()
                    .or_else(|| action.get("action_params").and_then(Value::as_str))
                    .unwrap_or("");
                if !url_str.is_empty() {
                    let mut payload = conv_json(&st.db, conv).await;
                    payload["event"] = json!(format!("automation_event.{}", rule.event_name));
                    let _ = send_webhook(url_str, &payload).await;
                }
            }

            "send_attachment" => {
                let blob_ids = values_to_strings(params)
                    .into_iter()
                    .filter_map(|s| s.parse::<i64>().ok())
                    .collect::<Vec<_>>();
                if !blob_ids.is_empty() {
                    // Chatwoot: blobs belonging to this rule
                    let blobs: Vec<(i64, String, Option<String>)> = sqlx::query_as(
                        "SELECT b.id, b.filename, b.content_type
                         FROM active_storage_blobs b
                         JOIN active_storage_attachments asa ON asa.blob_id = b.id
                         WHERE asa.record_type = 'AutomationRule' AND asa.record_id = $1 AND b.id = ANY($2)",
                    )
                    .bind(rule.id)
                    .bind(&blob_ids)
                    .fetch_all(&st.db)
                    .await
                    .unwrap_or_default();

                    if !blobs.is_empty() {
                        let msg_id: Option<i64> = sqlx::query_scalar(
                            "INSERT INTO messages (account_id, inbox_id, conversation_id, message_type, private, status, content_type, content_attributes, created_at, updated_at)
                             VALUES ($1, $2, $3, 1, false, 0, 0, $4::json, now(), now()) RETURNING id::bigint",
                        )
                        .bind(conv.account_id as i32)
                        .bind(conv.inbox_id as i32)
                        .bind(conv.id as i32)
                        .bind(json!({ "automation_rule_id": rule.id }))
                        .fetch_optional(&st.db)
                        .await?;

                        if let Some(mid) = msg_id {
                            for (blob_id, fname, ct) in blobs {
                                let ft = if ct.as_deref().unwrap_or("").starts_with("image/") { 0 } else { 3 };
                                let ext = fname.rsplit('.').next().map(|s| s.to_string());
                                let att_id: Option<i64> = sqlx::query_scalar(
                                    "INSERT INTO attachments (file_type, message_id, account_id, extension, created_at, updated_at)
                                     VALUES ($1, $2, $3, $4, now(), now()) RETURNING id::bigint",
                                )
                                .bind(ft)
                                .bind(mid as i32)
                                .bind(conv.account_id as i32)
                                .bind(ext)
                                .fetch_optional(&st.db)
                                .await?;

                                if let Some(aid) = att_id {
                                    let _ = sqlx::query(
                                        "INSERT INTO active_storage_attachments (name, record_type, record_id, blob_id, created_at)
                                         VALUES ('file', 'Attachment', $1, $2, now())",
                                    )
                                    .bind(aid)
                                    .bind(blob_id)
                                    .execute(&st.db)
                                    .await;
                                }
                            }
                        }
                    }
                }
            }

            _ => {
                tracing::warn!(action_name = name, rule_id = rule.id, "unsupported automation action");
            }
        }
    }

    // Reload conversation and broadcast conversation.updated
    if let Ok(updated_conv) = reload(&st.db, conv.id).await {
        *conv = updated_conv;
        let mut data = conv_json(&st.db, conv).await;
        data["performed_by"] = json!({ "type": "AutomationRule", "id": rule.id });
        emit(st, conv, "conversation.updated", data).await;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_conv() -> Conv {
        Conv {
            id: 1,
            account_id: 1,
            inbox_id: 2,
            status: 0,
            assignee_id: Some(10),
            created_at: Utc::now().naive_utc(),
            updated_at: Utc::now().naive_utc(),
            contact_id: Some(20),
            display_id: 100,
            contact_last_seen_at: None,
            agent_last_seen_at: None,
            additional_attributes: Some(json!({ "referer": "https://example.com" })),
            contact_inbox_id: Some(30),
            uuid: "uuid-123".into(),
            last_activity_at: Utc::now().naive_utc(),
            team_id: Some(5),
            snoozed_until: None,
            custom_attributes: Some(json!({ "vip_code": "GOLD" })),
            assignee_last_seen_at: None,
            first_reply_created_at: None,
            priority: Some(1),
            waiting_since: None,
            cached_label_list: Some("support, billing".into()),
        }
    }

    #[test]
    fn test_content_conditions() {
        let conv = dummy_conv();
        let msg = MsgRow {
            id: 100,
            content: Some("I would like a refund please".into()),
            account_id: 1,
            inbox_id: 2,
            conversation_id: 1,
            message_type: 0,
            private: false,
            processed_message_content: None,
            content_attributes: None,
            additional_attributes: None,
        };

        let cond_contains = json!([
            { "attribute_key": "content", "filter_operator": "contains", "values": ["refund"] }
        ]);
        assert!(matches_conditions(cond_contains.as_array().unwrap(), &conv, None, Some(&msg), None));

        let cond_not_contains = json!([
            { "attribute_key": "content", "filter_operator": "does_not_contain", "values": ["refund"] }
        ]);
        assert!(!matches_conditions(cond_not_contains.as_array().unwrap(), &conv, None, Some(&msg), None));

        let cond_no_match = json!([
            { "attribute_key": "content", "filter_operator": "contains", "values": ["shipping"] }
        ]);
        assert!(!matches_conditions(cond_no_match.as_array().unwrap(), &conv, None, Some(&msg), None));
    }

    #[test]
    fn test_label_and_status_conditions() {
        let conv = dummy_conv();

        let cond = json!([
            { "attribute_key": "labels", "filter_operator": "equal_to", "values": ["billing"], "query_operator": "AND" },
            { "attribute_key": "status", "filter_operator": "equal_to", "values": ["open"] }
        ]);
        assert!(matches_conditions(cond.as_array().unwrap(), &conv, None, None, None));

        let cond_fail = json!([
            { "attribute_key": "labels", "filter_operator": "equal_to", "values": ["missing_tag"], "query_operator": "AND" },
            { "attribute_key": "status", "filter_operator": "equal_to", "values": ["open"] }
        ]);
        assert!(!matches_conditions(cond_fail.as_array().unwrap(), &conv, None, None, None));

        let cond_or = json!([
            { "attribute_key": "labels", "filter_operator": "equal_to", "values": ["missing_tag"], "query_operator": "OR" },
            { "attribute_key": "status", "filter_operator": "equal_to", "values": ["open"] }
        ]);
        assert!(matches_conditions(cond_or.as_array().unwrap(), &conv, None, None, None));
    }

    #[test]
    fn test_loop_guard() {
        let ev_clean = Event {
            name: "message.created".into(),
            account_id: 1,
            data: json!({ "id": 1, "message_type": 0 }),
            changed: None,
        };
        assert!(!performed_by_automation(&ev_clean));

        let ev_automated = Event {
            name: "message.created".into(),
            account_id: 1,
            data: json!({
                "id": 2,
                "message_type": 1,
                "content_attributes": { "automation_rule_id": 42 }
            }),
            changed: None,
        };
        assert!(performed_by_automation(&ev_automated));

        let ev_activity = Event {
            name: "message.created".into(),
            account_id: 1,
            data: json!({ "id": 3, "message_type": 2 }),
            changed: None,
        };
        assert!(performed_by_automation(&ev_activity));
    }

    #[test]
    fn test_custom_and_changed_conditions() {
        let conv = dummy_conv();
        let cond_custom = json!([
            { "attribute_key": "vip_code", "filter_operator": "equal_to", "values": ["gold"] }
        ]);
        assert!(matches_conditions(cond_custom.as_array().unwrap(), &conv, None, None, None));

        let changed = json!({ "status": ["open", "resolved"] });
        let cond_changed = json!([
            { "attribute_key": "status", "filter_operator": "attribute_changed", "values": { "from": ["open"], "to": ["resolved"] } }
        ]);
        assert!(matches_conditions(cond_changed.as_array().unwrap(), &conv, None, None, Some(&changed)));

        let cond_changed_miss = json!([
            { "attribute_key": "status", "filter_operator": "attribute_changed", "values": { "from": ["pending"], "to": ["resolved"] } }
        ]);
        assert!(!matches_conditions(cond_changed_miss.as_array().unwrap(), &conv, None, None, Some(&changed)));
    }
}
