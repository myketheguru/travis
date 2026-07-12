//! v0.28.36 — client-side route geometry fetch.
//!
//! MapCanvas invokes this when it renders a route without a
//! geometry_geojson field (the LLM often drops the payload). We call
//! the same authenticated cloud maps proxy the everyday pack uses,
//! but from a Tauri command instead of a tool. The frontend swaps
//! the initial straight-line fallback for the returned LineString
//! as soon as it resolves — like Google Maps loading the actual path.

use serde::Serialize;
use serde_json::json;
use tauri::State;

use crate::cloud::{read_jwt, CLOUD_BASE};
use crate::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteGeometryResult {
    pub distance_meters: f64,
    pub duration_seconds: f64,
    pub geometry: serde_json::Value,
}

/// v0.28.40 — one ring per minute cutoff. Frontend renders them as
/// stacked polygon overlays (larger cutoffs render underneath so
/// nested contours read clearly).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsochroneFeature {
    pub minutes: u32,
    pub area_km2: f64,
    pub geometry: serde_json::Value,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IsochronesResult {
    pub profile: String,
    pub features: Vec<IsochroneFeature>,
}

/// v0.28.40 — reachability polygons ("isochrones"). Given a center +
/// minute cutoffs, returns nested Polygon features. Renders on the
/// map as translucent brand-purple halos with hover callouts.
#[tauri::command]
pub async fn fetch_isochrones(
    state: State<'_, AppState>,
    center_lat: f64,
    center_lng: f64,
    minutes: Vec<u32>,
    profile: Option<String>,
) -> Result<IsochronesResult, String> {
    let jwt = read_jwt().ok_or_else(|| "not signed in".to_string())?;
    let profile = profile.unwrap_or_else(|| "driving-car".to_string());
    let url = format!("{CLOUD_BASE}/maps/reach");
    let resp = state
        .http
        .post(&url)
        .bearer_auth(jwt)
        .json(&json!({
            "center": { "lat": center_lat, "lng": center_lng },
            "minutes": minutes,
            "profile": profile,
        }))
        .send()
        .await
        .map_err(|e| format!("cloud reach request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("cloud reach returned {}", resp.status()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("cloud reach parse failed: {e}"))?;
    let profile_out = body
        .get("profile")
        .and_then(|v| v.as_str())
        .unwrap_or("driving-car")
        .to_string();
    let features_raw = body
        .get("features")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let mut features = Vec::with_capacity(features_raw.len());
    for f in features_raw {
        let minutes = f
            .get("minutes")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32)
            .unwrap_or(0);
        let area_km2 = f.get("areaKm2").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let geometry = f.get("geometry").cloned().unwrap_or(serde_json::Value::Null);
        features.push(IsochroneFeature { minutes, area_km2, geometry });
    }
    Ok(IsochronesResult { profile: profile_out, features })
}

#[tauri::command]
pub async fn fetch_route_geometry(
    state: State<'_, AppState>,
    from_lat: f64,
    from_lng: f64,
    to_lat: f64,
    to_lng: f64,
    profile: Option<String>,
) -> Result<RouteGeometryResult, String> {
    let jwt = read_jwt().ok_or_else(|| "not signed in".to_string())?;
    let profile = profile.unwrap_or_else(|| "driving-car".to_string());
    let url = format!("{CLOUD_BASE}/maps/directions");
    let resp = state
        .http
        .post(&url)
        .bearer_auth(jwt)
        .json(&json!({
            "from": { "lat": from_lat, "lng": from_lng },
            "to":   { "lat": to_lat,   "lng": to_lng },
            "profile": profile,
        }))
        .send()
        .await
        .map_err(|e| format!("cloud maps request failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("cloud maps returned {}", resp.status()));
    }
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("cloud maps parse failed: {e}"))?;
    let distance_meters = body
        .get("distanceMeters")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "missing distanceMeters".to_string())?;
    let duration_seconds = body
        .get("durationSeconds")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "missing durationSeconds".to_string())?;
    let geometry = body
        .get("geometry")
        .cloned()
        .ok_or_else(|| "missing geometry".to_string())?;
    Ok(RouteGeometryResult {
        distance_meters,
        duration_seconds,
        geometry,
    })
}
