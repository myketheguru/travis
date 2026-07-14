//! Circles — v0.28.48.
//!
//! HTTP clients for `/circles/*` on travis-cloud. Circles are named
//! groups; anyone in the same circle is auto-discoverable as a
//! contact in the desktop app.

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::cloud::{read_jwt, CLOUD_BASE};

fn auth_header() -> Result<String> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    Ok(format!("Bearer {jwt}"))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Circle {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub join_code: String,
    pub creator_id: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    pub role: String,
    pub member_count: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircleMember {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub email: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CircleContact {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub email: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JoinResult {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub role: String,
    pub already_member: bool,
}

pub async fn create_circle(
    http: &reqwest::Client,
    name: &str,
    description: Option<&str>,
) -> Result<Circle> {
    #[derive(Serialize)]
    struct Req<'a> {
        name: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<&'a str>,
    }
    let resp = http
        .post(format!("{CLOUD_BASE}/circles"))
        .header("authorization", auth_header()?)
        .json(&Req { name, description })
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

pub async fn list_circles(http: &reqwest::Client) -> Result<Vec<Circle>> {
    #[derive(Deserialize)]
    struct Body {
        circles: Vec<Circle>,
    }
    let resp = http
        .get(format!("{CLOUD_BASE}/circles"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    let body: Body = resp.json().await?;
    Ok(body.circles)
}

pub async fn join_circle(http: &reqwest::Client, code: &str) -> Result<JoinResult> {
    #[derive(Serialize)]
    struct Req<'a> {
        code: &'a str,
    }
    let resp = http
        .post(format!("{CLOUD_BASE}/circles/join"))
        .header("authorization", auth_header()?)
        .json(&Req { code })
        .send()
        .await?
        .error_for_status()?;
    Ok(resp.json().await?)
}

pub async fn leave_circle(http: &reqwest::Client, id: &str) -> Result<()> {
    http.post(format!("{CLOUD_BASE}/circles/{id}/leave"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

pub async fn list_members(http: &reqwest::Client, id: &str) -> Result<Vec<CircleMember>> {
    #[derive(Deserialize)]
    struct Body {
        members: Vec<CircleMember>,
    }
    let resp = http
        .get(format!("{CLOUD_BASE}/circles/{id}/members"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    let body: Body = resp.json().await?;
    Ok(body.members)
}

pub async fn list_contacts(http: &reqwest::Client) -> Result<Vec<CircleContact>> {
    #[derive(Deserialize)]
    struct Body {
        contacts: Vec<CircleContact>,
    }
    let resp = http
        .get(format!("{CLOUD_BASE}/circles/contacts"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    let body: Body = resp.json().await?;
    Ok(body.contacts)
}

pub async fn delete_circle(http: &reqwest::Client, id: &str) -> Result<()> {
    http.delete(format!("{CLOUD_BASE}/circles/{id}"))
        .header("authorization", auth_header()?)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
