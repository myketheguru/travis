use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

const MIN_SUPPORTED_URL: &str =
    "https://github.com/myketheguru/travis-releases/raw/main/min-supported.json";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MinSupportedManifest {
    min_version: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    latest_version: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ForceUpgradeStatus {
    pub required: bool,
    pub current_version: String,
    pub min_version: Option<String>,
    pub latest_version: Option<String>,
    pub reason: Option<String>,
}

fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let trimmed = v.trim().trim_start_matches('v');
    let head = trimmed.split('-').next().unwrap_or(trimmed);
    let mut parts = head.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// Hit the small min-supported.json sentinel and decide whether the running
/// build is too old to keep using. Network failure / missing file → not
/// required (we never block the user on a transient outage). Existing builds
/// without this command stay unchanged.
#[tauri::command]
pub async fn check_force_upgrade(
    state: State<'_, AppState>,
) -> Result<ForceUpgradeStatus, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let default = ForceUpgradeStatus {
        required: false,
        current_version: current_version.clone(),
        min_version: None,
        latest_version: None,
        reason: None,
    };

    let resp = match state
        .http
        .get(MIN_SUPPORTED_URL)
        .header("user-agent", format!("travis/{current_version}"))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::debug!("force_upgrade fetch failed: {e}");
            return Ok(default);
        }
    };

    if !resp.status().is_success() {
        return Ok(default);
    }

    let manifest: MinSupportedManifest = match resp.json().await {
        Ok(m) => m,
        Err(e) => {
            tracing::debug!("force_upgrade parse failed: {e}");
            return Ok(default);
        }
    };

    let Some(min) = parse_semver(&manifest.min_version) else {
        return Ok(default);
    };
    let Some(cur) = parse_semver(&current_version) else {
        return Ok(default);
    };

    Ok(ForceUpgradeStatus {
        required: cur < min,
        current_version,
        min_version: Some(manifest.min_version),
        latest_version: manifest.latest_version,
        reason: manifest.reason,
    })
}

/// Exit the app immediately. Used by the force-upgrade gate's "Quit" button
/// when the user opts to bail rather than install. Safe to call from any
/// window context.
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::parse_semver;

    #[test]
    fn parses_basic() {
        assert_eq!(parse_semver("0.20.2"), Some((0, 20, 2)));
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("0.20.2-rc.1"), Some((0, 20, 2)));
    }

    #[test]
    fn ordering_blocks_when_old() {
        let cur = parse_semver("0.19.9").unwrap();
        let min = parse_semver("0.20.0").unwrap();
        assert!(cur < min);
    }

    #[test]
    fn ordering_allows_when_current() {
        let cur = parse_semver("0.20.2").unwrap();
        let min = parse_semver("0.20.0").unwrap();
        assert!(cur >= min);
    }
}
