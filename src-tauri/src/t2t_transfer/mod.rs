//! v0.28.53 — Travis-to-Travis secure file transfer.
//!
//! Uses the crypto module for the E2EE primitives; the Worker
//! endpoints in travis-cloud src/routes/t2t.ts for transport. The
//! server sees only ciphertext + the sender's ephemeral pubkey.
//!
//! Two public entry points, both async, both return `Result<T,String>`
//! so they're straight tauri command bodies:
//!
//!   * `publish_my_pubkey` — POST /me/keys/x25519 with this machine's
//!     static X25519 pubkey. Called once after sign-in; safe to call
//!     repeatedly (server upserts).
//!   * `send_file(peer_id, path)` — read `path`, fetch peer's pubkey,
//!     encrypt, upload ciphertext + ephem pub as query params. Returns
//!     the transfer id.
//!   * `poll_inbox()` — list pending incoming files.
//!   * `download_and_decrypt(id, dest)` — pull ciphertext, decrypt
//!     with our static secret + the sender's ephem pubkey, write
//!     plaintext to `dest`. Ack the server on success.
//!
//! The peripheral-role BLE handoff is genuinely blocked on per-OS
//! work (btleplug is scan-only cross-platform; macOS needs
//! objc2-core-bluetooth, Windows needs WinRT GATTServer, Linux needs
//! bluer). Cloud relay ships now; that's the transport that reaches
//! users today. A follow-up will add the BLE peripheral path once
//! each OS's server-role crate is wired.

pub mod cmd;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::cloud::{read_jwt, CLOUD_BASE};
use crate::crypto;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxFile {
    pub id: String,
    pub from_user_id: String,
    pub from_email: Option<String>,
    pub from_name: Option<String>,
    pub filename: String,
    pub content_type: Option<String>,
    pub ciphertext_bytes: u64,
    pub sender_ephem_pub: String,
    pub created_at: String,
}

/// Push our static X25519 pubkey to the cloud so peers can encrypt to
/// us. Idempotent on the server. Safe to call at every sign-in.
pub async fn publish_my_pubkey(http: &reqwest::Client) -> Result<()> {
    let jwt = read_jwt().ok_or_else(|| anyhow!("not signed in"))?;
    let pubkey_hex = crypto::static_public_hex()?;
    let url = format!("{}/t2t/me/keys/x25519", CLOUD_BASE);
    let resp = http
        .post(&url)
        .bearer_auth(jwt)
        .json(&serde_json::json!({ "pubkey_hex": pubkey_hex }))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "publish pubkey: {} — {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    Ok(())
}

/// Look up a peer's static X25519 pubkey. Requires an active
/// relationship — the server 403s otherwise.
async fn fetch_peer_pubkey(http: &reqwest::Client, peer_id: &str) -> Result<String> {
    let jwt = read_jwt().ok_or_else(|| anyhow!("not signed in"))?;
    let url = format!(
        "{}/t2t/users/{}/pubkey/x25519",
        CLOUD_BASE,
        urlencoding::encode(peer_id)
    );
    let resp = http.get(&url).bearer_auth(jwt).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "fetch peer pubkey: {} — {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    #[derive(Deserialize)]
    struct KeyResp {
        pubkey_hex: String,
    }
    let body: KeyResp = resp.json().await?;
    Ok(body.pubkey_hex)
}

/// Encrypt `path` to `peer_id`, upload, return the server-side
/// transfer id. This is the code the "Send file" button ends up
/// running.
pub async fn send_file(
    http: &reqwest::Client,
    peer_id: &str,
    path: &Path,
) -> Result<String> {
    let jwt = read_jwt().ok_or_else(|| anyhow!("not signed in"))?;
    let filename = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let content_type = mime_from_extension(path);

    let plaintext = tokio::fs::read(path).await?;
    let peer_pub = fetch_peer_pubkey(http, peer_id).await?;

    let transfer_id = crypto::new_transfer_id();
    let (ciphertext, ephem_pub) =
        crypto::encrypt_for_recipient(&peer_pub, &transfer_id, &plaintext)?;

    let url = format!(
        "{}/t2t/files/send?to={}&filename={}&content_type={}&ephem_pub={}&transfer_id={}",
        CLOUD_BASE,
        urlencoding::encode(peer_id),
        urlencoding::encode(&filename),
        urlencoding::encode(&content_type),
        ephem_pub,
        transfer_id,
    );
    let resp = http
        .post(&url)
        .bearer_auth(jwt)
        .header("content-type", "application/octet-stream")
        .body(ciphertext)
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "send file: {} — {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    #[derive(Deserialize)]
    struct SendResp {
        id: String,
    }
    let body: SendResp = resp.json().await?;
    Ok(body.id)
}

pub async fn poll_inbox(http: &reqwest::Client) -> Result<Vec<InboxFile>> {
    let jwt = read_jwt().ok_or_else(|| anyhow!("not signed in"))?;
    let url = format!("{}/t2t/files/inbox", CLOUD_BASE);
    let resp = http.get(&url).bearer_auth(jwt).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "poll inbox: {} — {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    #[derive(Deserialize)]
    struct InboxResp {
        files: Vec<InboxFile>,
    }
    let body: InboxResp = resp.json().await?;
    Ok(body.files)
}

pub async fn download_and_decrypt(
    http: &reqwest::Client,
    transfer_id: &str,
    dest_dir: &Path,
) -> Result<std::path::PathBuf> {
    let jwt = read_jwt().ok_or_else(|| anyhow!("not signed in"))?;
    let url = format!(
        "{}/t2t/files/{}/download",
        CLOUD_BASE,
        urlencoding::encode(transfer_id)
    );
    let resp = http.get(&url).bearer_auth(&jwt).send().await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "download: {} — {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    let ephem_pub = resp
        .headers()
        .get("x-travis-ephem-pub")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| anyhow!("missing x-travis-ephem-pub header"))?
        .to_string();
    let filename = resp
        .headers()
        .get("x-travis-filename")
        .and_then(|v| v.to_str().ok())
        .map(|s| urlencoding::decode(s).map(|c| c.into_owned()).unwrap_or_else(|_| s.to_string()))
        .unwrap_or_else(|| format!("t2t-{transfer_id}.bin"));
    let ciphertext = resp.bytes().await?;

    let plaintext = crypto::decrypt_from_sender(&ephem_pub, transfer_id, &ciphertext)?;

    tokio::fs::create_dir_all(dest_dir).await?;
    // Guard against filename traversal — take only the basename.
    let safe_name = std::path::Path::new(&filename)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("received")
        .to_string();
    let dest = dest_dir.join(&safe_name);
    tokio::fs::write(&dest, &plaintext).await?;

    // Ack — server prunes the R2 blob + marks delivered.
    let ack_url = format!(
        "{}/t2t/files/{}/ack",
        CLOUD_BASE,
        urlencoding::encode(transfer_id)
    );
    let _ = http.post(&ack_url).bearer_auth(&jwt).send().await;
    Ok(dest)
}

fn mime_from_extension(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" => "text/plain",
        "md" => "text/markdown",
        "json" => "application/json",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "zip" => "application/zip",
        "wav" => "audio/wav",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
    .to_string()
}
