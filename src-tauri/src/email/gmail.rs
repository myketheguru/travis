//! Send email via the Gmail REST API using the OAuth2 access token already
//! stored for the user's Google connection. Mirrors the SMTP audit-row
//! pattern in `super`: insert pending → call API → mark sent/failed.

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sqlx::SqlitePool;

use crate::calendar::google;

use super::EmailSent;

const SEND_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";

/// Send a plain-text email via Gmail. The body is encoded as RFC 2822 plain
/// text (UTF-8) and uploaded base64url-encoded under `raw`.
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
    .context("insert pending email_sent (gmail)")?
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
            .context("mark gmail email sent")?;
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
            .context("mark gmail email failed")?;
            let row = super::fetch_one(pool, pending_id).await?;
            return Err(anyhow!("gmail send failed: {msg}").context(format!(
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
    let token = google::access_token(pool, http)
        .await
        .map_err(|e| anyhow!("get google access token: {e}"))?;

    let raw = build_rfc822(to, subject, body);
    let encoded = URL_SAFE_NO_PAD.encode(raw.as_bytes());
    let payload = serde_json::json!({ "raw": encoded });

    let resp = http
        .post(SEND_URL)
        .bearer_auth(&token)
        .json(&payload)
        .send()
        .await
        .map_err(|e| anyhow!("gmail post: {e}"))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gmail api {status}: {body}");
    }
    Ok(())
}

/// Build a minimal RFC 2822 message. Subject is RFC 2047-encoded if it
/// contains non-ASCII characters; body rides as 8-bit UTF-8.
fn build_rfc822(to: &str, subject: &str, body: &str) -> String {
    let subject_header = if subject.is_ascii() {
        subject.to_string()
    } else {
        // =?UTF-8?B?<base64>?= per RFC 2047.
        format!(
            "=?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode(subject.as_bytes())
        )
    };

    // Normalize bare LFs to CRLF — required by RFC 2822.
    let body_crlf = body.replace("\r\n", "\n").replace('\n', "\r\n");

    format!(
        "To: {to}\r\n\
         Subject: {subject_header}\r\n\
         MIME-Version: 1.0\r\n\
         Content-Type: text/plain; charset=\"UTF-8\"\r\n\
         Content-Transfer-Encoding: 8bit\r\n\
         \r\n\
         {body_crlf}"
    )
}
