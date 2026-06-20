//! Send email via Microsoft Graph (`POST /me/sendMail`) using the OAuth
//! access token stored for the user's Microsoft connection. Mirrors the
//! audit-row pattern used by the SMTP and Gmail paths.

use anyhow::{anyhow, Context, Result};
use sqlx::SqlitePool;

use crate::calendar::microsoft;

use super::EmailSent;

const SEND_URL: &str = "https://graph.microsoft.com/v1.0/me/sendMail";
const LIST_URL: &str = "https://graph.microsoft.com/v1.0/me/mailFolders/inbox/messages";

/// v0.21.4 Tier 1 — list recent inbox messages via Microsoft Graph.
/// Schema-compatible with [`crate::email::gmail::InboxMessage`] so
/// callers handle both providers uniformly.
pub async fn list_recent(
    pool: &SqlitePool,
    http: &reqwest::Client,
    since: chrono::DateTime<chrono::Utc>,
    max: usize,
) -> Result<Vec<crate::email::gmail::InboxMessage>> {
    let token = microsoft::access_token(pool, http)
        .await
        .map_err(|e| anyhow!("get microsoft access token: {e}"))?;

    let filter = format!(
        "receivedDateTime ge {}",
        since.to_rfc3339()
    );
    let resp = http
        .get(LIST_URL)
        .bearer_auth(&token)
        .query(&[
            ("$filter", filter.as_str()),
            ("$top", &max.min(50).to_string()),
            ("$select", "id,conversationId,from,subject,bodyPreview,receivedDateTime,isRead,body,categories"),
            ("$orderby", "receivedDateTime desc"),
        ])
        .send()
        .await
        .map_err(|e| anyhow!("graph list: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("graph list api {status}: {body}");
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| anyhow!("graph list parse: {e}"))?;
    let Some(items) = v["value"].as_array() else {
        return Ok(Vec::new());
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items.iter().take(max) {
        let id = item["id"].as_str().unwrap_or_default().to_string();
        let thread_id = item["conversationId"].as_str().unwrap_or_default().to_string();
        let subject = item["subject"].as_str().unwrap_or_default().to_string();
        let snippet = item["bodyPreview"].as_str().unwrap_or_default().to_string();
        let received_at = item["receivedDateTime"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let is_unread = !item["isRead"].as_bool().unwrap_or(true);

        let from_obj = &item["from"]["emailAddress"];
        let from_email = from_obj["address"].as_str().unwrap_or_default().to_string();
        let from_name = from_obj["name"]
            .as_str()
            .map(String::from)
            .filter(|s| !s.is_empty() && s != &from_email);

        // Graph body has a content + contentType (HTML or text). Strip
        // HTML to plain so the LLM sees text. Cap at 4kB.
        let body_raw = item["body"]["content"].as_str().unwrap_or_default();
        let body_type = item["body"]["contentType"].as_str().unwrap_or("text");
        let body_preview: String = if body_type.eq_ignore_ascii_case("html") {
            strip_html_for_preview(body_raw)
        } else {
            body_raw.to_string()
        }
        .chars()
        .take(4000)
        .collect();

        let labels: Vec<String> = item["categories"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();

        out.push(crate::email::gmail::InboxMessage {
            id,
            thread_id,
            from_email,
            from_name,
            subject,
            snippet,
            received_at,
            is_unread,
            body_preview,
            labels,
        });
    }
    Ok(out)
}

fn strip_html_for_preview(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
            out.push(' ');
        } else if !in_tag {
            out.push(c);
        }
    }
    let mut collapsed = String::with_capacity(out.len());
    let mut last_ws = false;
    for c in out.chars() {
        if c.is_whitespace() {
            if !last_ws {
                collapsed.push(' ');
            }
            last_ws = true;
        } else {
            collapsed.push(c);
            last_ws = false;
        }
    }
    collapsed.trim().to_string()
}

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
