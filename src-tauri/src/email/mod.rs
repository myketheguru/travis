//! SMTP email sending + audit log.
//!
//! The active SMTP config is a single-row table (`smtp_config`); the password
//! lives in the OS keychain under service "Travis", entry "smtp_password"
//! (mirroring how `secrets` stores LLM API keys).
//!
//! Every send attempt is recorded in `email_sent` first as `pending`, then
//! transitioned to `sent` / `failed` so the UI can display history even when
//! the send loop blew up mid-flight.

pub mod gmail;
pub mod outlook;

use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use lettre::message::header::ContentType;
use lettre::message::{Attachment, MultiPart, SinglePart};
use lettre::transport::smtp::authentication::Credentials;
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

use crate::secrets;

// ---------------------------------------------------------------------------
// SMTP config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct SmtpConfig {
    pub id: i64,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub from_address: String,
    pub from_name: Option<String>,
    /// SQLite stores BOOLEAN as INTEGER 0/1.
    pub use_tls: i64,
    pub updated_at: String,
}

impl SmtpConfig {
    pub fn use_tls_bool(&self) -> bool {
        self.use_tls != 0
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SmtpConfigInput {
    pub host: String,
    pub port: Option<i64>,
    pub username: String,
    pub from_address: String,
    pub from_name: Option<String>,
    pub use_tls: Option<bool>,
}

const SMTP_PASSWORD_PROVIDER: &str = "smtp_password";

pub async fn get_config(pool: &SqlitePool) -> Result<Option<SmtpConfig>> {
    let row = sqlx::query_as::<_, SmtpConfig>(
        "SELECT id, host, port, username, from_address, from_name, use_tls, updated_at
         FROM smtp_config WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .context("query smtp_config")?;
    Ok(row)
}

pub async fn set_config(
    pool: &SqlitePool,
    input: SmtpConfigInput,
    password: Option<String>,
) -> Result<SmtpConfig> {
    let host = input.host.trim().to_string();
    let username = input.username.trim().to_string();
    let from_address = input.from_address.trim().to_string();
    if host.is_empty() {
        return Err(anyhow!("host is required"));
    }
    if username.is_empty() {
        return Err(anyhow!("username is required"));
    }
    if from_address.is_empty() {
        return Err(anyhow!("fromAddress is required"));
    }
    let port = input.port.unwrap_or(587);
    if !(1..=65535).contains(&port) {
        return Err(anyhow!("port must be between 1 and 65535"));
    }
    let use_tls: i64 = if input.use_tls.unwrap_or(true) { 1 } else { 0 };
    let from_name = input.from_name.as_ref().map(|s| s.trim().to_string());

    sqlx::query(
        "INSERT INTO smtp_config (id, host, port, username, from_address, from_name, use_tls, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)
         ON CONFLICT(id) DO UPDATE SET
             host         = excluded.host,
             port         = excluded.port,
             username     = excluded.username,
             from_address = excluded.from_address,
             from_name    = excluded.from_name,
             use_tls      = excluded.use_tls,
             updated_at   = CURRENT_TIMESTAMP",
    )
    .bind(&host)
    .bind(port)
    .bind(&username)
    .bind(&from_address)
    .bind(&from_name)
    .bind(use_tls)
    .execute(pool)
    .await
    .context("upsert smtp_config")?;

    if let Some(pw) = password {
        let trimmed = pw.trim();
        if !trimmed.is_empty() {
            secrets::store_api_key(SMTP_PASSWORD_PROVIDER, trimmed)
                .map_err(|e| anyhow!("store smtp password: {e}"))?;
        }
    }

    get_config(pool)
        .await?
        .ok_or_else(|| anyhow!("smtp_config row not found after upsert"))
}

// ---------------------------------------------------------------------------
// email_sent rows
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EmailSent {
    pub id: i64,
    pub recipient: String,
    pub subject: String,
    pub body_preview: Option<String>,
    pub kind: Option<String>,
    pub related_kind: Option<String>,
    pub related_id: Option<i64>,
    pub status: String,
    pub error_message: Option<String>,
    pub sent_at: Option<String>,
    pub created_at: String,
}

pub async fn list_sent(pool: &SqlitePool, limit: i64) -> Result<Vec<EmailSent>> {
    let limit = limit.clamp(1, 500);
    let rows = sqlx::query_as::<_, EmailSent>(
        "SELECT id, recipient, subject, body_preview, kind, related_kind, related_id,
                status, error_message, sent_at, created_at
         FROM email_sent
         ORDER BY created_at DESC, id DESC
         LIMIT ?1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("list email_sent")?;
    Ok(rows)
}

pub(crate) async fn fetch_one(pool: &SqlitePool, id: i64) -> Result<EmailSent> {
    let row = sqlx::query_as::<_, EmailSent>(
        "SELECT id, recipient, subject, body_preview, kind, related_kind, related_id,
                status, error_message, sent_at, created_at
         FROM email_sent WHERE id = ?1",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .context("fetch email_sent")?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// send
// ---------------------------------------------------------------------------

/// Send an email via the configured SMTP transport, optionally attaching a
/// file. `attachment` is `(path, mime_type)` — for invoice PDFs, pass
/// `"application/pdf"`.
///
/// `_http_client` is accepted for symmetry with other modules that use the
/// shared `reqwest` client; SMTP doesn't need it.
pub async fn send(
    pool: &SqlitePool,
    _http_client: &reqwest::Client,
    recipient: &str,
    subject: &str,
    body: &str,
    attachment: Option<(PathBuf, &str)>,
    kind: Option<&str>,
    related_kind: Option<&str>,
    related_id: Option<i64>,
) -> Result<EmailSent> {
    let recipient = recipient.trim();
    if recipient.is_empty() {
        return Err(anyhow!("recipient is required"));
    }
    if subject.trim().is_empty() {
        return Err(anyhow!("subject is required"));
    }

    let config = get_config(pool)
        .await?
        .ok_or_else(|| anyhow!("smtp_config not set — configure SMTP in Settings first"))?;

    let body_preview = preview(body);

    // Insert as pending so a crash mid-send is still observable.
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
    .context("insert pending email_sent")?
    .last_insert_rowid();

    // Run the actual send and capture either Ok(()) or an error string for the
    // audit row.
    let send_result = do_send(&config, recipient, subject, body, attachment).await;

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
            .context("mark email sent")?;
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
            .context("mark email failed")?;
            // Surface the error to the caller AFTER the row has been updated.
            let row = fetch_one(pool, pending_id).await?;
            return Err(anyhow!("send failed: {msg}").context(format!(
                "email row id={} recipient={}",
                row.id, row.recipient
            )));
        }
    }

    fetch_one(pool, pending_id).await
}

async fn do_send(
    cfg: &SmtpConfig,
    recipient: &str,
    subject: &str,
    body: &str,
    attachment: Option<(PathBuf, &str)>,
) -> Result<()> {
    let password = secrets::get_api_key(SMTP_PASSWORD_PROVIDER)
        .ok_or_else(|| anyhow!("SMTP password not stored — call set_smtp_config with a password"))?;

    // From: "Name <addr>" or just "addr".
    let from = match &cfg.from_name {
        Some(name) if !name.is_empty() => format!("{name} <{}>", cfg.from_address),
        _ => cfg.from_address.clone(),
    };

    // Build message.
    let mut builder = Message::builder()
        .from(from.parse().map_err(|e| anyhow!("invalid from address: {e}"))?)
        .to(recipient.parse().map_err(|e| anyhow!("invalid recipient: {e}"))?)
        .subject(subject);
    // Reply-To = from_address so external recipients reply to a real inbox.
    builder = builder.reply_to(
        cfg.from_address
            .parse()
            .map_err(|e| anyhow!("invalid from_address for reply-to: {e}"))?,
    );

    let body_part = SinglePart::builder()
        .header(ContentType::TEXT_PLAIN)
        .body(body.to_string());

    // In lettre 0.11 `singlepart` / `multipart` return `Result<Message, Error>`
    // when required headers (e.g. From) are missing; we propagate that.
    let message: Message = match attachment {
        None => builder
            .singlepart(body_part)
            .map_err(|e| anyhow!("build message: {e}"))?,
        Some((path, mime)) => {
            let bytes = std::fs::read(&path)
                .with_context(|| format!("read attachment {}", path.display()))?;
            let filename = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "attachment".to_string());
            let content_type: ContentType = mime
                .parse()
                .map_err(|e| anyhow!("invalid attachment mime '{mime}': {e}"))?;
            let attachment_part = Attachment::new(filename).body(bytes, content_type);
            let multi = MultiPart::mixed().singlepart(body_part).singlepart(attachment_part);
            builder
                .multipart(multi)
                .map_err(|e| anyhow!("build multipart message: {e}"))?
        }
    };

    // Build transport.
    let creds = Credentials::new(cfg.username.clone(), password);
    let port: u16 = u16::try_from(cfg.port).map_err(|_| anyhow!("invalid smtp port {}", cfg.port))?;

    let transport = if cfg.use_tls_bool() {
        // 587 is the typical "submission" port using STARTTLS; 465 is
        // implicit-TLS. Heuristic: port 465 -> wrapper TLS, anything else with
        // use_tls=1 -> STARTTLS.
        let tls_params = TlsParameters::new(cfg.host.clone())
            .map_err(|e| anyhow!("build TLS params: {e}"))?;
        if port == 465 {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(cfg.host.clone())
                .port(port)
                .tls(Tls::Wrapper(tls_params))
                .credentials(creds)
                .build()
        } else {
            AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(cfg.host.clone())
                .port(port)
                .tls(Tls::Required(tls_params))
                .credentials(creds)
                .build()
        }
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(cfg.host.clone())
            .port(port)
            .credentials(creds)
            .build()
    };

    transport
        .send(message)
        .await
        .map_err(|e| anyhow!("smtp send: {e}"))?;
    Ok(())
}

pub(crate) fn preview(body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut out: String = trimmed.chars().take(160).collect();
    if trimmed.chars().count() > 160 {
        out.push('…');
    }
    Some(out)
}
