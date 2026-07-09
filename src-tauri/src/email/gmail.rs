//! Send email via the Gmail REST API using the OAuth2 access token already
//! stored for the user's Google connection. Mirrors the SMTP audit-row
//! pattern in `super`: insert pending → call API → mark sent/failed.

use anyhow::{anyhow, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sqlx::SqlitePool;

use crate::calendar::google;

use super::EmailSent;

const SEND_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages/send";
const LIST_URL: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";
const GET_BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me/messages";

/// v0.21.4 Tier 1 — inbox read API for the proactive observer. One
/// row per message; populated from Gmail's REST surface.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxMessage {
    pub id: String,
    pub thread_id: String,
    pub from_email: String,
    pub from_name: Option<String>,
    pub subject: String,
    pub snippet: String,
    pub received_at: String,
    pub is_unread: bool,
    /// First 4kB of the message body, plain text. Truncated; for full
    /// fetch use [`get_message_body`].
    pub body_preview: String,
    /// Gmail labels currently on this message (INBOX, IMPORTANT, etc.).
    pub labels: Vec<String>,
}

/// List recent messages from the user's inbox. `since` is an
/// ISO-8601 timestamp; messages before it are excluded. `max` caps
/// the result count. The two-step pattern (list → get each) is
/// Gmail's documented surface — there is no batch fetch in v1.
pub async fn list_recent(
    pool: &SqlitePool,
    http: &reqwest::Client,
    since: chrono::DateTime<chrono::Utc>,
    max: usize,
) -> Result<Vec<InboxMessage>> {
    let token = google::access_token(pool, http)
        .await
        .map_err(|e| anyhow!("get google access token: {e}"))?;

    // Gmail's `q` param accepts a Unix epoch via after:NNNN.
    let after_epoch = since.timestamp();
    let q = format!("in:inbox after:{after_epoch}");
    let list_resp = http
        .get(LIST_URL)
        .bearer_auth(&token)
        .query(&[
            ("q", q.as_str()),
            ("maxResults", &max.min(50).to_string()),
        ])
        .send()
        .await
        .map_err(|e| anyhow!("gmail list: {e}"))?;
    let list_status = list_resp.status();
    if !list_status.is_success() {
        let body = list_resp.text().await.unwrap_or_default();
        anyhow::bail!("gmail list api {list_status}: {body}");
    }
    #[derive(serde::Deserialize)]
    struct ListRow {
        id: String,
        #[serde(rename = "threadId")]
        thread_id: String,
    }
    #[derive(serde::Deserialize)]
    struct ListResp {
        #[serde(default)]
        messages: Vec<ListRow>,
    }
    let listed: ListResp = list_resp.json().await.map_err(|e| anyhow!("gmail list parse: {e}"))?;

    // Fetch each message. Cap parallelism implicitly via a small loop —
    // 50 sequential gets at ~50ms each is 2.5s which is acceptable for
    // a background observer. If we hit rate limits we'll batch.
    let mut out = Vec::with_capacity(listed.messages.len());
    for row in listed.messages.iter().take(max) {
        match fetch_one_metadata(http, &token, &row.id).await {
            Ok(msg) => out.push(msg),
            Err(e) => tracing::warn!("gmail get {}: {e}", row.id),
        }
    }
    Ok(out)
}

async fn fetch_one_metadata(
    http: &reqwest::Client,
    token: &str,
    message_id: &str,
) -> Result<InboxMessage> {
    let url = format!("{GET_BASE}/{message_id}");
    let resp = http
        .get(&url)
        .bearer_auth(token)
        // METADATA strips bodies but keeps headers; FULL gives the
        // body. We want both, so FULL it is — Gmail's quota cost is
        // 5 units vs 1 but worth it to avoid a second round trip.
        .query(&[("format", "full")])
        .send()
        .await
        .map_err(|e| anyhow!("gmail get: {e}"))?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("gmail get api {status}: {body}");
    }
    let v: serde_json::Value = resp.json().await.map_err(|e| anyhow!("gmail get parse: {e}"))?;

    let id = v["id"].as_str().unwrap_or_default().to_string();
    let thread_id = v["threadId"].as_str().unwrap_or_default().to_string();
    let snippet = v["snippet"].as_str().unwrap_or_default().to_string();
    let internal_ms = v["internalDate"]
        .as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);
    let received_at = chrono::DateTime::<chrono::Utc>::from_timestamp(internal_ms / 1000, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default();

    let labels: Vec<String> = v["labelIds"]
        .as_array()
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let is_unread = labels.iter().any(|l| l == "UNREAD");

    let headers = v["payload"]["headers"].as_array();
    let mut from_email = String::new();
    let mut from_name: Option<String> = None;
    let mut subject = String::new();
    if let Some(arr) = headers {
        for h in arr {
            let Some(name) = h["name"].as_str() else { continue };
            let Some(val) = h["value"].as_str() else { continue };
            match name.to_ascii_lowercase().as_str() {
                "from" => {
                    let (n, e) = parse_from(val);
                    from_name = n;
                    from_email = e;
                }
                "subject" => subject = val.to_string(),
                _ => {}
            }
        }
    }

    let body_preview = extract_body_text(&v["payload"]).chars().take(4000).collect();

    Ok(InboxMessage {
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
    })
}

/// "Sarah Jones <sarah@acme.com>" -> (Some("Sarah Jones"), "sarah@acme.com")
/// "sarah@acme.com" -> (None, "sarah@acme.com")
fn parse_from(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim();
    if let (Some(open), Some(close)) = (trimmed.find('<'), trimmed.rfind('>')) {
        if open < close {
            let name = trimmed[..open].trim().trim_matches('"').to_string();
            let email = trimmed[open + 1..close].trim().to_string();
            let name_opt = if name.is_empty() { None } else { Some(name) };
            return (name_opt, email);
        }
    }
    (None, trimmed.to_string())
}

/// Walk the MIME payload tree and return the first text/plain body
/// (or text/html stripped to text). Best-effort; Gmail's payloads
/// can be deeply nested.
fn extract_body_text(payload: &serde_json::Value) -> String {
    fn walk(node: &serde_json::Value) -> Option<String> {
        let mime = node["mimeType"].as_str().unwrap_or("");
        if mime.starts_with("text/plain") {
            if let Some(data) = node["body"]["data"].as_str() {
                if let Ok(bytes) = URL_SAFE_NO_PAD.decode(data) {
                    return String::from_utf8(bytes).ok();
                }
            }
        }
        if let Some(parts) = node["parts"].as_array() {
            for p in parts {
                if let Some(text) = walk(p) {
                    return Some(text);
                }
            }
        }
        // Fall back to text/html if no text/plain found.
        if mime.starts_with("text/html") {
            if let Some(data) = node["body"]["data"].as_str() {
                if let Ok(bytes) = URL_SAFE_NO_PAD.decode(data) {
                    if let Ok(html) = String::from_utf8(bytes) {
                        return Some(strip_html(&html));
                    }
                }
            }
        }
        None
    }
    walk(payload).unwrap_or_default()
}

/// Strip HTML tags + collapse whitespace. Good enough for triage
/// preview; we never render this — it goes to the LLM for
/// classification.
fn strip_html(html: &str) -> String {
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
    // Collapse runs of whitespace.
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

/// v0.28.22 — search the user's Sent folder for outbound messages.
/// Used by the follow-ups pack to auto-detect when the user has
/// already sent the promised email so an open follow-up can be
/// closed without them saying so.
///
/// `q_extra` is appended to the Gmail search query verbatim; caller
/// passes something like `to:sarah@acme.com newer_than:14d` to narrow.
/// Returns the top N matches with just subject + received_at so the
/// LLM can decide whether the message actually corresponds to the
/// promised follow-up.
#[derive(Debug, Clone, serde::Serialize)]
pub struct SentMatch {
    pub id: String,
    pub subject: String,
    pub to: String,
    pub sent_at: String,
    pub snippet: String,
}

pub async fn search_sent(
    pool: &SqlitePool,
    http: &reqwest::Client,
    q_extra: &str,
    max: usize,
) -> Result<Vec<SentMatch>> {
    let token = google::access_token(pool, http)
        .await
        .map_err(|e| anyhow!("get google access token: {e}"))?;
    let cleaned = q_extra.trim();
    let q = if cleaned.is_empty() {
        "in:sent".to_string()
    } else {
        format!("in:sent {cleaned}")
    };
    let list_resp = http
        .get(LIST_URL)
        .bearer_auth(&token)
        .query(&[
            ("q", q.as_str()),
            ("maxResults", &max.min(20).to_string()),
        ])
        .send()
        .await
        .map_err(|e| anyhow!("gmail sent-list: {e}"))?;
    let status = list_resp.status();
    if !status.is_success() {
        let body = list_resp.text().await.unwrap_or_default();
        anyhow::bail!("gmail sent-list {status}: {body}");
    }
    #[derive(serde::Deserialize)]
    struct ListRow { id: String }
    #[derive(serde::Deserialize)]
    struct ListResp { #[serde(default)] messages: Vec<ListRow> }
    let listed: ListResp = list_resp.json().await.context("gmail sent-list parse")?;

    let mut out = Vec::with_capacity(listed.messages.len());
    for row in listed.messages.iter().take(max) {
        let url = format!("{GET_BASE}/{}", row.id);
        let get_resp = match http
            .get(&url)
            .bearer_auth(&token)
            .query(&[("format", "metadata"), ("metadataHeaders", "To"), ("metadataHeaders", "Subject")])
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => { tracing::warn!("gmail sent-get: {e}"); continue; }
        };
        if !get_resp.status().is_success() { continue; }
        let v: serde_json::Value = match get_resp.json().await { Ok(x) => x, Err(_) => continue };
        let internal_ms = v["internalDate"].as_str().and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let sent_at = chrono::DateTime::<chrono::Utc>::from_timestamp(internal_ms / 1000, 0)
            .map(|dt| dt.to_rfc3339()).unwrap_or_default();
        let snippet = v["snippet"].as_str().unwrap_or_default().to_string();
        let mut subject = String::new();
        let mut to = String::new();
        if let Some(hs) = v["payload"]["headers"].as_array() {
            for h in hs {
                let name = h["name"].as_str().unwrap_or("").to_ascii_lowercase();
                let val = h["value"].as_str().unwrap_or("");
                if name == "subject" { subject = val.to_string(); }
                if name == "to" { to = val.to_string(); }
            }
        }
        out.push(SentMatch { id: row.id.clone(), subject, to, sent_at, snippet });
    }
    Ok(out)
}

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
