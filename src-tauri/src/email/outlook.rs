//! Send email via Microsoft Graph (`POST /me/sendMail`) using the OAuth
//! access token stored for the user's Microsoft connection. Mirrors the
//! audit-row pattern used by the SMTP and Gmail paths.

use anyhow::{anyhow, Context, Result};
use sqlx::SqlitePool;

use crate::calendar::microsoft;

use super::EmailSent;

const SEND_URL: &str = "https://graph.microsoft.com/v1.0/me/sendMail";

pub async fn send(
    pool: &SqlitePool,
    http: &reqwest::Client,
    to: &str,
    subject: &str,
    body: &str,
    kind: Option<&str>,
    related_kind: Option<&str>,
    related_id: Option<i64>,
) -> Result<EmailSent> {
    let recipient = to.trim();
    if recipient.is_empty() {
        return Err(anyhow!("recipient is required"));
    }
    if subject.trim().is_empty() {
        return Err(anyhow!("subject is required"));
    }

    let body_preview = super::preview(body);

    let pending_id = sqlx::query(
        "INSERT INTO email_sent (recipient, subject, body_preview, kind, related_kind, related_id, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending')",
    )
    .bind(recipient)
    .bind(subject)
    .bind(&body_preview)
    .bind(kind)
    .bind(related_kind)
    .bind(related_id)
    .execute(pool)
    .await
    .context("insert pending email_sent (outlook)")?
    .last_insert_rowid();

    let send_result = do_send(pool, http, recipient, subject, body).await;

    match send_result {
        Ok(()) => {
            sqlx::query(
                "UPDATE email_sent
                 SET status = 'sent', sent_at = CURRENT_TIMESTAMP, error_message = NULL
                 WHERE id = ?1",
            )
            .bind(pending_id)
            .execute(pool)
            .await
            .context("mark outlook email sent")?;
        }
        Err(e) => {
            let msg = format!("{e:#}");
            sqlx::query(
                "UPDATE email_sent SET status = 'failed', error_message = ?1 WHERE id = ?2",
            )
            .bind(&msg)
            .bind(pending_id)
            .execute(pool)
            .await
            .context("mark outlook email failed")?;
            let row = super::fetch_one(pool, pending_id).await?;
            return Err(anyhow!("outlook send failed: {msg}").context(format!(
                "email row id={} recipient={}",
                row.id, row.recipient
            )));
        }
    }

    super::fetch_one(pool, pending_id).await
}

async fn do_send(
    pool: &SqlitePool,
    http: &reqwest::Client,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<()> {
    let token = microsoft::access_token(pool, http)
        .await
        .map_err(|e| anyhow!("get microsoft access token: {e}"))?;

    let payload = serde_json::json!({
        "message": {
            "subject": subject,
            "body": {
                "contentType": "Text",
                "content": body,
            },
            "toRecipients": [
                { "emailAddress": { "address": to } }
            ],
        },
        "saveToSentItems": true,
    });

    let resp = http
        .post(SEND_URL)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| anyhow!("graph post: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("graph api {status}: {body}");
    }
    Ok(())
}
