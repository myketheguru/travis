//! Travis-to-Travis (T2T) client for desktop.
//!
//! Cloud endpoints live at `/t2t/*`; this module wraps them so the
//! desktop UI + WorkflowLoop can consume without hand-crafting HTTP
//! calls. Every fn requires a signed-in session — call `read_jwt()`
//! first and route to sign-in if it's missing.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cloud::{read_jwt, CLOUD_BASE};

// ─── Types ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipStatus {
    Pending,
    Active,
    Revoked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relationship {
    pub id: String,
    pub from_user_id: String,
    pub to_user_id: String,
    pub status: RelationshipStatus,
    #[serde(default)]
    pub invited_at: Option<String>,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub revoked_at: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    /// Populated by the cloud join — the other side's identity.
    #[serde(default)]
    pub other_email: Option<String>,
    #[serde(default)]
    pub other_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryStatus {
    Pending,
    Drafted,
    Approved,
    Declined,
    Answered,
    Expired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct T2tQuery {
    pub id: String,
    pub from_user_id: String,
    pub to_user_id: String,
    #[serde(default)]
    pub from_conversation_id: Option<String>,
    pub question: String,
    #[serde(default)]
    pub context_json: Option<String>,
    pub status: QueryStatus,
    #[serde(default)]
    pub drafted_response: Option<String>,
    #[serde(default)]
    pub drafted_at: Option<String>,
    #[serde(default)]
    pub response: Option<String>,
    #[serde(default)]
    pub responded_at: Option<String>,
    #[serde(default)]
    pub declined_reason: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    /// Populated by the cloud join — other side's identity.
    #[serde(default)]
    pub from_email: Option<String>,
    #[serde(default)]
    pub from_name: Option<String>,
    #[serde(default)]
    pub to_email: Option<String>,
    #[serde(default)]
    pub to_name: Option<String>,
}

// ─── Helpers ──────────────────────────────────────────────────────

fn auth_header() -> Result<String> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    Ok(format!("Bearer {jwt}"))
}

// ─── Relationships ────────────────────────────────────────────────

pub async fn list_relationships(http: &reqwest::Client) -> Result<Vec<Relationship>> {
    let resp = http
        .get(format!("{CLOUD_BASE}/t2t/relationships"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    #[derive(Deserialize)]
    struct Body {
        relationships: Vec<Relationship>,
    }
    let body: Body = resp.json().await?;
    Ok(body.relationships)
}

pub async fn invite_relationship(
    http: &reqwest::Client,
    email: &str,
    reason: Option<&str>,
) -> Result<String> {
    #[derive(Serialize)]
    struct Req<'a> {
        email: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<&'a str>,
    }
    #[derive(Deserialize)]
    struct Resp {
        id: String,
    }
    let resp = http
        .post(format!("{CLOUD_BASE}/t2t/relationships/invite"))
        .header("authorization", auth_header()?)
        .json(&Req { email, reason })
        .send()
        .await?
        .error_for_status()?;
    let body: Resp = resp.json().await?;
    Ok(body.id)
}

pub async fn accept_relationship(http: &reqwest::Client, id: &str) -> Result<()> {
    http.post(format!("{CLOUD_BASE}/t2t/relationships/{id}/accept"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn revoke_relationship(
    http: &reqwest::Client,
    id: &str,
    reason: Option<&str>,
) -> Result<()> {
    #[derive(Serialize)]
    struct Req<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<&'a str>,
    }
    http.post(format!("{CLOUD_BASE}/t2t/relationships/{id}/revoke"))
        .header("authorization", auth_header()?)
        .json(&Req { reason })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

// ─── Pair tokens (v0.28.46) ──────────────────────────────────────
// Short-lived codes that let two Travises pair beyond the LAN.
// Backend endpoints POST /t2t/pair/token and POST /t2t/pair/redeem.

#[derive(Debug, Deserialize, Serialize)]
pub struct PairToken {
    pub token: String,
    pub expires_at: String,
    pub deep_link: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PairRedeemResult {
    pub ok: bool,
    pub relationship_id: String,
    pub other_user: Option<PairOtherUser>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PairOtherUser {
    pub id: String,
    pub name: Option<String>,
    pub email: String,
}

pub async fn create_pair_token(http: &reqwest::Client) -> Result<PairToken> {
    let resp = http
        .post(format!("{CLOUD_BASE}/t2t/pair/token"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

pub async fn redeem_pair_token(
    http: &reqwest::Client,
    token: &str,
) -> Result<PairRedeemResult> {
    #[derive(Serialize)]
    struct Req<'a> {
        token: &'a str,
    }
    let resp = http
        .post(format!("{CLOUD_BASE}/t2t/pair/redeem"))
        .header("authorization", auth_header()?)
        .json(&Req { token })
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

// ─── Queries ──────────────────────────────────────────────────────

pub async fn send_query(
    http: &reqwest::Client,
    to_user_id: &str,
    question: &str,
    from_conversation_id: Option<&str>,
    expires_after_days: Option<u32>,
) -> Result<String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Req<'a> {
        to_user_id: &'a str,
        question: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_conversation_id: Option<&'a str>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expires_after_days: Option<u32>,
    }
    #[derive(Deserialize)]
    struct Resp {
        id: String,
    }
    let resp = http
        .post(format!("{CLOUD_BASE}/t2t/queries"))
        .header("authorization", auth_header()?)
        .json(&Req {
            to_user_id,
            question,
            from_conversation_id,
            expires_after_days,
        })
        .send()
        .await?
        .error_for_status()?;
    let body: Resp = resp.json().await?;
    Ok(body.id)
}

pub async fn inbox(http: &reqwest::Client) -> Result<Vec<T2tQuery>> {
    let resp = http
        .get(format!("{CLOUD_BASE}/t2t/queries/inbox"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    #[derive(Deserialize)]
    struct Body {
        queries: Vec<T2tQuery>,
    }
    let body: Body = resp.json().await?;
    Ok(body.queries)
}

pub async fn outbox(http: &reqwest::Client) -> Result<Vec<T2tQuery>> {
    let resp = http
        .get(format!("{CLOUD_BASE}/t2t/queries/outbox"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    #[derive(Deserialize)]
    struct Body {
        queries: Vec<T2tQuery>,
    }
    let body: Body = resp.json().await?;
    Ok(body.queries)
}

pub async fn draft_reply(
    http: &reqwest::Client,
    id: &str,
    drafted_response: &str,
) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Req<'a> {
        drafted_response: &'a str,
    }
    http.post(format!("{CLOUD_BASE}/t2t/queries/{id}/draft"))
        .header("authorization", auth_header()?)
        .json(&Req { drafted_response })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn approve_reply(
    http: &reqwest::Client,
    id: &str,
    final_response: Option<&str>,
) -> Result<()> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Req<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        final_response: Option<&'a str>,
    }
    http.post(format!("{CLOUD_BASE}/t2t/queries/{id}/approve"))
        .header("authorization", auth_header()?)
        .json(&Req { final_response })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn decline_reply(
    http: &reqwest::Client,
    id: &str,
    reason: Option<&str>,
) -> Result<()> {
    #[derive(Serialize)]
    struct Req<'a> {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<&'a str>,
    }
    http.post(format!("{CLOUD_BASE}/t2t/queries/{id}/decline"))
        .header("authorization", auth_header()?)
        .json(&Req { reason })
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
