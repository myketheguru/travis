//! Cloud file storage — wraps the `/sync/files/*` endpoints.
//!
//! Two-step upload:
//!   1. POST /sync/files/put-url with the content hash + mime type →
//!      returns a signed upload URL (HMAC token in querystring, no JWT
//!      needed for the actual PUT).
//!   2. PUT bytes to that URL with the appropriate content-type.
//!
//! Bytes are content-addressed by SHA-256. The cloud stores them in R2
//! at `users/<userId>/<contentHash>`. Identical content uploaded twice
//! is a no-op on the cloud's side.

use std::time::Duration;

use serde::Deserialize;

use super::{read_jwt, CLOUD_BASE};

#[derive(Debug, Deserialize)]
struct PutUrlResponse {
    #[allow(dead_code)]
    key: String,
    #[serde(rename = "uploadUrl")]
    upload_url: String,
    #[allow(dead_code)]
    #[serde(rename = "contentType", default)]
    content_type: Option<String>,
}

/// Upload a single content-addressed blob to the cloud. The caller
/// supplies the SHA-256 hex hash and the bytes; we negotiate the
/// signed upload URL and PUT the bytes.
///
/// Idempotent: re-uploading the same hash overwrites the R2 object
/// with identical bytes, so duplicate calls are safe.
pub async fn upload_one(
    http: &reqwest::Client,
    content_hash: &str,
    bytes: Vec<u8>,
    mime_type: &str,
) -> anyhow::Result<()> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;

    let put_url_resp = http
        .post(format!("{CLOUD_BASE}/sync/files/put-url"))
        .header("authorization", format!("Bearer {jwt}"))
        .timeout(Duration::from_secs(15))
        .json(&serde_json::json!({
            "contentHash": content_hash,
            "contentType": mime_type,
            "sizeBytes": bytes.len(),
        }))
        .send()
        .await?;
    if !put_url_resp.status().is_success() {
        let status = put_url_resp.status().as_u16();
        let body = put_url_resp.text().await.unwrap_or_default();
        anyhow::bail!("/sync/files/put-url {status}: {body}");
    }
    let put: PutUrlResponse = put_url_resp.json().await?;

    let upload_resp = http
        .put(&put.upload_url)
        .header("content-type", mime_type)
        .timeout(Duration::from_secs(120))
        .body(bytes)
        .send()
        .await?;
    if !upload_resp.status().is_success() {
        let status = upload_resp.status().as_u16();
        let body = upload_resp.text().await.unwrap_or_default();
        anyhow::bail!("PUT /sync/files/upload {status}: {body}");
    }
    Ok(())
}

/// Download a content-addressed blob from the cloud. Used by lazy
/// document fetch when a user opens a doc whose bytes aren't local
/// yet (pulled metadata from another device).
#[allow(dead_code)]
pub async fn download_one(
    http: &reqwest::Client,
    content_hash: &str,
) -> anyhow::Result<Vec<u8>> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    let resp = http
        .get(format!("{CLOUD_BASE}/sync/files/{content_hash}"))
        .header("authorization", format!("Bearer {jwt}"))
        .timeout(Duration::from_secs(120))
        .send()
        .await?;
    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        anyhow::bail!("GET /sync/files/{content_hash}: {status}");
    }
    Ok(resp.bytes().await?.to_vec())
}
