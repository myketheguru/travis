//! Tauri commands wrapping the `email` module.

use std::path::PathBuf;

use tauri::State;

use crate::email::{self, EmailSent, SmtpConfig, SmtpConfigInput};
use crate::AppState;

#[tauri::command]
pub async fn get_smtp_config(state: State<'_, AppState>) -> Result<Option<SmtpConfig>, String> {
    email::get_config(&state.db.pool)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_smtp_config(
    state: State<'_, AppState>,
    input: SmtpConfigInput,
    password: Option<String>,
) -> Result<SmtpConfig, String> {
    email::set_config(&state.db.pool, input, password)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_emails_sent(
    state: State<'_, AppState>,
    limit: Option<i64>,
) -> Result<Vec<EmailSent>, String> {
    email::list_sent(&state.db.pool, limit.unwrap_or(50))
        .await
        .map_err(|e| e.to_string())
}

/// Send a rendered invoice PDF as an email attachment. L2E-specific
/// (depends on the pack's invoice + PDF modules); compiled only when
/// the `pack-lead-to-empower` feature is enabled.
#[cfg(feature = "pack-lead-to-empower")]
#[tauri::command]
pub async fn send_invoice_email(
    state: State<'_, AppState>,
    invoice_id: i64,
    recipient: String,
    custom_message: Option<String>,
) -> Result<EmailSent, String> {
    let recipient = recipient.trim().to_string();
    if recipient.is_empty() {
        return Err("recipient is required".into());
    }

    // Need the user profile for the org name (subject / from-block on the PDF).
    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| format!("load user profile: {e}"))?
        .ok_or_else(|| {
            "user profile not set up — finish onboarding before sending invoices".to_string()
        })?;

    // Fetch invoice for subject / body.
    let invoice = crate::packs::lead_to_empower::domain::invoice::fetch_one(&state.db.pool, invoice_id)
        .await
        .map_err(|e| format!("fetch invoice: {e}"))?;

    // Render PDF to a temp file.
    let mut pdf_path: PathBuf = std::env::temp_dir();
    pdf_path.push(format!(
        "travis-invoice-{}-{}.pdf",
        invoice.id,
        sanitize_filename(&invoice.number)
    ));

    crate::packs::lead_to_empower::pdf::export_invoice(&state.db.pool, invoice_id, &pdf_path, &profile)
        .await
        .map_err(|e| format!("render invoice pdf: {e}"))?;

    // Compose subject + body.
    let subject = format!("Invoice {} from {}", invoice.number, profile.org);
    let body = match custom_message {
        Some(msg) if !msg.trim().is_empty() => msg,
        _ => default_body(&invoice.number, &profile.org, &profile.name),
    };

    let result = email::send(
        &state.db.pool,
        &state.http,
        &recipient,
        &subject,
        &body,
        Some((pdf_path.clone(), "application/pdf")),
        Some("invoice"),
        Some("invoice"),
        Some(invoice_id),
    )
    .await;

    // Best-effort cleanup; ignore failures.
    let _ = std::fs::remove_file(&pdf_path);

    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_email_outlook(
    state: State<'_, AppState>,
    to: String,
    subject: String,
    body: String,
    related_kind: Option<String>,
    related_id: Option<i64>,
) -> Result<EmailSent, String> {
    let to = to.trim().to_string();
    let subject = subject.trim().to_string();
    if to.is_empty() {
        return Err("recipient is required".into());
    }
    if subject.is_empty() {
        return Err("subject is required".into());
    }
    crate::email::outlook::send(
        &state.db.pool,
        &state.http,
        &to,
        &subject,
        &body,
        Some("outlook"),
        related_kind.as_deref(),
        related_id,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn send_email_gmail(
    state: State<'_, AppState>,
    to: String,
    subject: String,
    body: String,
    related_kind: Option<String>,
    related_id: Option<i64>,
) -> Result<EmailSent, String> {
    let to = to.trim().to_string();
    let subject = subject.trim().to_string();
    if to.is_empty() {
        return Err("recipient is required".into());
    }
    if subject.is_empty() {
        return Err("subject is required".into());
    }
    crate::email::gmail::send(
        &state.db.pool,
        &state.http,
        &to,
        &subject,
        &body,
        Some("gmail"),
        related_kind.as_deref(),
        related_id,
    )
    .await
    .map_err(|e| e.to_string())
}

fn default_body(number: &str, org: &str, sender: &str) -> String {
    format!(
        "Hi,\n\n\
         Please find attached invoice #{number} from {org}.\n\n\
         Let me know if you have any questions.\n\n\
         Thanks,\n{sender}\n"
    )
}

fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
