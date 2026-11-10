//! `chatwoot-rs seed <email> <password> [account name]`: creates confirmed admin + account + API inbox.
use sqlx::PgPool;

pub async fn run(db: &PgPool, args: &[String]) -> anyhow::Result<()> {
    let email = args.first().map(|s| s.to_lowercase()).ok_or_else(|| anyhow::anyhow!("usage: seed <email> <password> [account]"))?;
    let password = args.get(1).ok_or_else(|| anyhow::anyhow!("password required"))?;
    anyhow::ensure!(password.len() >= 6, "password must be >= 6 chars");
    let account_name = args.get(2).cloned().unwrap_or_else(|| "Acme".into());
    let hash = bcrypt::hash(password, 11)?;
    let (ff, ff1) = crate::auth::default_feature_flags();
    let mut tx = db.begin().await?;
    let uid: i64 = sqlx::query_scalar(
        "INSERT INTO users (provider, uid, encrypted_password, name, email, confirmed_at, pubsub_token, created_at, updated_at, tokens)
         VALUES ('email', $1, $2, $3, $1, now(), $4, now(), now(), '{}')
         ON CONFLICT (uid, provider) DO UPDATE SET encrypted_password = EXCLUDED.encrypted_password RETURNING id::bigint",
    )
    .bind(&email)
    .bind(&hash)
    .bind(email.split('@').next().unwrap_or("Admin"))
    .bind(uuid::Uuid::new_v4().simple().to_string())
    .fetch_one(&mut *tx)
    .await?;
    let existing: Option<i64> = sqlx::query_scalar("SELECT account_id FROM account_users WHERE user_id=$1 LIMIT 1")
        .bind(uid)
        .fetch_optional(&mut *tx)
        .await?;
    let aid = match existing {
        Some(a) => a,
        None => {
            let aid: i64 = sqlx::query_scalar(
                "INSERT INTO accounts (name, created_at, updated_at, feature_flags, feature_flags_ext_1) VALUES ($1, now(), now(), $2, $3) RETURNING id::bigint",
            )
            .bind(&account_name)
            .bind(ff)
            .bind(ff1)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("INSERT INTO account_users (account_id, user_id, role, created_at, updated_at, active_at) VALUES ($1,$2,1,now(),now(),now())")
                .bind(aid)
                .bind(uid)
                .execute(&mut *tx)
                .await?;
            let ch: i64 = sqlx::query_scalar(
                "INSERT INTO channel_api (account_id, identifier, hmac_token, secret, created_at, updated_at) VALUES ($1,$2,$3,$4,now(),now()) RETURNING id",
            )
            .bind(aid)
            .bind(uuid::Uuid::new_v4().simple().to_string())
            .bind(uuid::Uuid::new_v4().simple().to_string())
            .bind(uuid::Uuid::new_v4().simple().to_string())
            .fetch_one(&mut *tx)
            .await?;
            let inbox: i64 = sqlx::query_scalar(
                "INSERT INTO inboxes (channel_id, account_id, name, channel_type, created_at, updated_at) VALUES ($1,$2,'API Inbox','Channel::Api',now(),now()) RETURNING id::bigint",
            )
            .bind(ch as i32)
            .bind(aid as i32)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("INSERT INTO inbox_members (user_id, inbox_id, created_at, updated_at) VALUES ($1,$2,now(),now())")
                .bind(uid as i32)
                .bind(inbox as i32)
                .execute(&mut *tx)
                .await?;
            sqlx::query("INSERT INTO access_tokens (owner_type, owner_id, token, created_at, updated_at) VALUES ('User',$1,$2,now(),now()) ON CONFLICT DO NOTHING")
                .bind(uid)
                .bind(uuid::Uuid::new_v4().simple().to_string())
                .execute(&mut *tx)
                .await?;
            aid
        }
    };
    tx.commit().await?;
    println!("user {uid} <{email}> admin of account {aid}");
    Ok(())
}
