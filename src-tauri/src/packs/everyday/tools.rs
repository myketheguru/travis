//! Everyday pack LLM tools.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::cloud::{read_jwt, CLOUD_BASE};
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};

// ─── save_place ───────────────────────────────────────────────────

pub struct SavePlaceTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavePlaceInput {
    name: String,
    address: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[async_trait]
impl Tool for SavePlaceTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "save_place".into(),
            description: "Save a place the user cares about (a doctor's office, \
                a friend's flat, a coffee shop). Geocodes the address once so \
                future routes don't re-look-up. Returns the saved place id + \
                its lat/lng.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Short human name, e.g. 'Dr. Chen's office'." },
                    "address": { "type": "string", "description": "Full address to geocode." },
                    "tags": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Tags like 'clinic', 'friend', 'work'."
                    },
                    "notes": { "type": "string", "description": "Freeform notes." }
                },
                "required": ["name", "address"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: SavePlaceInput = serde_json::from_value(input)?;
        let (lat, lng, label) = geocode(&ctx.http, &p.address).await?;
        let tags_json = serde_json::to_string(&p.tags).unwrap_or_else(|_| "[]".to_string());
        let id: i64 = sqlx::query(
            "INSERT INTO saved_place (name, address, lat, lng, tags, notes)
             VALUES (?, ?, ?, ?, ?, ?)
             RETURNING id",
        )
        .bind(&p.name)
        .bind(label.as_deref().unwrap_or(&p.address))
        .bind(lat)
        .bind(lng)
        .bind(&tags_json)
        .bind(p.notes.as_deref())
        .fetch_one(&ctx.db.pool)
        .await?
        .get(0);
        Ok(json!({
            "id": id,
            "name": p.name,
            "lat": lat,
            "lng": lng,
            "resolved_label": label.unwrap_or(p.address),
        })
        .to_string())
    }
}

// ─── route_to_place ───────────────────────────────────────────────

pub struct RouteToPlaceTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteToPlaceInput {
    /// Name or partial name to match against saved_place.name.
    place_query: String,
    /// User's current location. If missing, tool bails asking for it.
    #[serde(default)]
    from_lat: Option<f64>,
    #[serde(default)]
    from_lng: Option<f64>,
    /// driving-car (default) | cycling-regular | foot-walking
    #[serde(default)]
    profile: Option<String>,
}

#[async_trait]
impl Tool for RouteToPlaceTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "route_to_place".into(),
            description: "Get directions from the user's current location to a \
                saved place. Match saved place by name (case-insensitive contains). \
                Returns distance + duration; you should also emit a `map` message \
                part in your response so the user sees the route as a card. \
                Requires from_lat + from_lng — if the user hasn't shared their \
                location, ask them or use their known 'home' saved place as \
                fallback.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "placeQuery": { "type": "string" },
                    "fromLat": { "type": "number", "nullable": true },
                    "fromLng": { "type": "number", "nullable": true },
                    "profile": {
                        "type": "string",
                        "enum": ["driving-car", "cycling-regular", "foot-walking"],
                        "nullable": true
                    }
                },
                "required": ["placeQuery"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: RouteToPlaceInput = serde_json::from_value(input)?;
        let (from_lat, from_lng) = match (p.from_lat, p.from_lng) {
            (Some(a), Some(b)) => (a, b),
            _ => anyhow::bail!(
                "No origin provided. Ask the user for their current location, \
                 or check if they have a saved place named 'home' and use its \
                 coordinates as the from_lat/from_lng."
            ),
        };
        let row = sqlx::query(
            "SELECT id, name, address, lat, lng
             FROM saved_place
             WHERE LOWER(name) LIKE ?
             ORDER BY updated_at DESC
             LIMIT 1",
        )
        .bind(format!("%{}%", p.place_query.to_lowercase()))
        .fetch_optional(&ctx.db.pool)
        .await?;
        let row = row.ok_or_else(|| {
            anyhow::anyhow!(
                "No saved place matches '{}'. Use save_place first, or ask the \
                 user which address they want.",
                p.place_query
            )
        })?;
        let to_lat: f64 = row.get("lat");
        let to_lng: f64 = row.get("lng");
        let name: String = row.get("name");
        let address: String = row.get("address");
        let profile = p.profile.unwrap_or_else(|| "driving-car".to_string());

        let route = fetch_directions(&ctx.http, from_lat, from_lng, to_lat, to_lng, &profile).await?;

        Ok(json!({
            "to_name": name,
            "to_address": address,
            "to_lat": to_lat,
            "to_lng": to_lng,
            "from_lat": from_lat,
            "from_lng": from_lng,
            "profile": profile,
            "distance_meters": route.distance_meters,
            "duration_seconds": route.duration_seconds,
            "note": "Emit a `map` message part in your reply so the user sees the route as a card, not just text.",
        })
        .to_string())
    }
}

// ─── list_saved_places ────────────────────────────────────────────

pub struct ListSavedPlacesTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListPlacesInput {
    /// Optional filter — case-insensitive substring match on name or tag.
    #[serde(default)]
    query: Option<String>,
}

#[async_trait]
impl Tool for ListSavedPlacesTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_saved_places".into(),
            description: "List the user's saved places, optionally filtered by \
                a case-insensitive substring match on the name. Use when the \
                user says things like 'do I have a saved place for the vet?' \
                or 'what places have I saved?'.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "nullable": true }
                }
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: ListPlacesInput = serde_json::from_value(input).unwrap_or(ListPlacesInput { query: None });
        let rows = if let Some(q) = p.query.filter(|s| !s.trim().is_empty()) {
            sqlx::query(
                "SELECT id, name, address, lat, lng, tags, notes
                 FROM saved_place
                 WHERE LOWER(name) LIKE ? OR LOWER(tags) LIKE ?
                 ORDER BY updated_at DESC
                 LIMIT 40",
            )
            .bind(format!("%{}%", q.to_lowercase()))
            .bind(format!("%{}%", q.to_lowercase()))
            .fetch_all(&ctx.db.pool)
            .await?
        } else {
            sqlx::query(
                "SELECT id, name, address, lat, lng, tags, notes
                 FROM saved_place
                 ORDER BY updated_at DESC
                 LIMIT 40",
            )
            .fetch_all(&ctx.db.pool)
            .await?
        };
        let out: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                let id: i64 = r.get("id");
                let name: String = r.get("name");
                let address: String = r.get("address");
                let lat: f64 = r.get("lat");
                let lng: f64 = r.get("lng");
                let tags: String = r.get("tags");
                let notes: Option<String> = r.get("notes");
                json!({
                    "id": id,
                    "name": name,
                    "address": address,
                    "lat": lat,
                    "lng": lng,
                    "tags": serde_json::from_str::<serde_json::Value>(&tags).unwrap_or(json!([])),
                    "notes": notes,
                })
            })
            .collect();
        Ok(serde_json::to_string(&out)?)
    }
}

// ─── add_note ─────────────────────────────────────────────────────

pub struct AddNoteTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddNoteInput {
    /// Short title — one-line summary.
    title: String,
    /// Full note body.
    body: String,
}

#[async_trait]
impl Tool for AddNoteTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "add_note".into(),
            description: "Quick-capture a personal note. Use when the user says \
                things like 'remember that Kim mentioned they're moving to \
                Portland' or 'log that Amanda wants to reschedule'. Persists as \
                a document of kind 'note' so it's searchable via find_documents \
                / search_conversations later.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "body":  { "type": "string" }
                },
                "required": ["title", "body"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: AddNoteInput = serde_json::from_value(input)?;
        // Persist as a plain document; reuses core storage + sync.
        let id: i64 = sqlx::query(
            "INSERT INTO document (title, kind, source, mime_type, content, created_at)
             VALUES (?, 'note', 'travis:everyday', 'text/plain', ?, CURRENT_TIMESTAMP)
             RETURNING id",
        )
        .bind(&p.title)
        .bind(&p.body)
        .fetch_one(&ctx.db.pool)
        .await?
        .get(0);
        Ok(json!({
            "document_id": id,
            "title": p.title,
            "note": "Note saved. Emit a `doc_ref` part in your reply referencing this document_id so the user sees a card."
        })
        .to_string())
    }
}

// ─── HTTP helpers (call the cloud maps proxy) ─────────────────────

async fn geocode(
    http: &reqwest::Client,
    query: &str,
) -> anyhow::Result<(f64, f64, Option<String>)> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    let url = format!("{}/maps/geocode", CLOUD_BASE);
    let resp = http
        .post(&url)
        .bearer_auth(jwt)
        .json(&json!({ "query": query }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let results = resp.get("results").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    let top = results
        .first()
        .ok_or_else(|| anyhow::anyhow!("no geocode results for '{query}'"))?;
    let lat = top.get("lat").and_then(|v| v.as_f64()).ok_or_else(|| anyhow::anyhow!("geocode: missing lat"))?;
    let lng = top.get("lng").and_then(|v| v.as_f64()).ok_or_else(|| anyhow::anyhow!("geocode: missing lng"))?;
    let label = top.get("label").and_then(|v| v.as_str()).map(String::from);
    Ok((lat, lng, label))
}

struct Route {
    distance_meters: f64,
    duration_seconds: f64,
}

async fn fetch_directions(
    http: &reqwest::Client,
    from_lat: f64,
    from_lng: f64,
    to_lat: f64,
    to_lng: f64,
    profile: &str,
) -> anyhow::Result<Route> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    let url = format!("{}/maps/directions", CLOUD_BASE);
    let resp = http
        .post(&url)
        .bearer_auth(jwt)
        .json(&json!({
            "from": { "lat": from_lat, "lng": from_lng },
            "to":   { "lat": to_lat,   "lng": to_lng },
            "profile": profile,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let distance = resp
        .get("distanceMeters")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow::anyhow!("directions: missing distanceMeters"))?;
    let duration = resp
        .get("durationSeconds")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| anyhow::anyhow!("directions: missing durationSeconds"))?;
    Ok(Route {
        distance_meters: distance,
        duration_seconds: duration,
    })
}
