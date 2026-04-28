use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::UserProfile;
use crate::llm::{self, ChatOptions, ChatResponse, Message, PingResult};
use crate::secrets;
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub version: String,
    pub db_ready: bool,
    pub onboarded: bool,
}

#[tauri::command]
pub async fn app_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let onboarded_flag = state
        .db
        .meta("onboarded")
        .await
        .map_err(|e| e.to_string())?
        .map(|v| v == "true")
        .unwrap_or(false);

    // Defensive: if a user_profile row already exists, treat as onboarded even
    // if the meta flag is missing/wrong. Guards against any migration or
    // partial write that leaves the flag inconsistent. Also self-heals.
    let profile_exists = state
        .db
        .user_profile()
        .await
        .map_err(|e| e.to_string())?
        .is_some();

    let onboarded = onboarded_flag || profile_exists;

    if onboarded && !onboarded_flag {
        tracing::warn!("app_status: profile exists but onboarded flag was false — healing");
        if let Err(e) = state.db.set_meta("onboarded", "true").await {
            tracing::warn!("failed to heal onboarded flag: {e}");
        }
    }

    tracing::debug!(
        "app_status: flag={onboarded_flag} profile={profile_exists} -> onboarded={onboarded}"
    );

    Ok(AppStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        db_ready: true,
        onboarded,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnboardingPayload {
    pub name: String,
    pub role: String,
    pub org: String,
    pub provider: String,
    pub api_key: Option<String>,
    pub ollama_url: Option<String>,
    pub model: Option<String>,
}

async fn write_profile_and_key(
    state: &AppState,
    payload: &OnboardingPayload,
) -> Result<(), String> {
    let provider = payload.provider.trim().to_lowercase();
    if !matches!(provider.as_str(), "claude" | "openai" | "ollama") {
        return Err(format!("unknown provider: {provider}"));
    }

    let profile = UserProfile {
        name: payload.name.trim().to_string(),
        role: payload.role.trim().to_string(),
        org: payload.org.trim().to_string(),
        llm_provider: provider.clone(),
        ollama_url: payload
            .ollama_url
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        model: payload
            .model
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };

    if profile.name.is_empty() || profile.role.is_empty() || profile.org.is_empty() {
        return Err("name, role, and org are required".into());
    }

    state
        .db
        .upsert_user_profile(&profile)
        .await
        .map_err(|e| e.to_string())?;

    if let Some(key) = payload
        .api_key
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        secrets::store_api_key(&provider, key).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn complete_onboarding(
    state: State<'_, AppState>,
    payload: OnboardingPayload,
) -> Result<(), String> {
    write_profile_and_key(&state, &payload).await?;

    state
        .db
        .set_meta("onboarded", "true")
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn update_profile(
    state: State<'_, AppState>,
    payload: OnboardingPayload,
) -> Result<(), String> {
    write_profile_and_key(&state, &payload).await
}

#[tauri::command]
pub async fn get_user_profile(
    state: State<'_, AppState>,
) -> Result<Option<UserProfile>, String> {
    state.db.user_profile().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn has_api_key(provider: String) -> bool {
    secrets::has_api_key(&provider.trim().to_lowercase())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProactiveConfig {
    pub enabled: bool,
    pub last_at: Option<String>,
}

#[tauri::command]
pub async fn get_proactive_config(state: State<'_, AppState>) -> Result<ProactiveConfig, String> {
    // Default ON: only off if the user has explicitly set it to "false".
    // Matches proactive::is_enabled.
    let raw = state
        .db
        .meta("proactive_enabled")
        .await
        .map_err(|e| e.to_string())?;
    let enabled = raw.as_deref() != Some("false");
    let last_at = state
        .db
        .meta("proactive_last_at")
        .await
        .map_err(|e| e.to_string())?;
    Ok(ProactiveConfig { enabled, last_at })
}

#[tauri::command]
pub async fn set_proactive_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state
        .db
        .set_meta("proactive_enabled", if enabled { "true" } else { "false" })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_shell_enabled(state: State<'_, AppState>) -> Result<bool, String> {
    let v = state
        .db
        .meta("shell_enabled")
        .await
        .map_err(|e| e.to_string())?;
    Ok(v.as_deref() == Some("true"))
}

#[tauri::command]
pub async fn set_shell_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<(), String> {
    state
        .db
        .set_meta("shell_enabled", if enabled { "true" } else { "false" })
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_api_key(provider: String, key: String) -> Result<(), String> {
    let p = provider.trim().to_lowercase();
    let k = key.trim();
    if k.is_empty() {
        return Err("api key cannot be empty".into());
    }
    if !matches!(p.as_str(), "claude" | "openai") {
        return Err(format!("provider {p} does not use an api key"));
    }
    secrets::store_api_key(&p, k).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProviderPayload {
    pub provider: String,
    pub api_key: Option<String>,
    pub ollama_url: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
pub async fn test_provider(
    state: State<'_, AppState>,
    payload: TestProviderPayload,
) -> Result<PingResult, String> {
    let provider = payload.provider.trim().to_lowercase();
    let api_key = payload.api_key.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let ollama_url = payload.ollama_url.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let model = payload.model.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let provider = llm::build(&provider, api_key, ollama_url, model, state.http.clone())
        .map_err(|e| e.to_string())?;
    provider.ping().await.map_err(|e| e.to_string())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPayload {
    pub messages: Vec<Message>,
    #[serde(default)]
    pub options: ChatOptions,
}

#[tauri::command]
pub async fn chat(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    payload: ChatPayload,
) -> Result<ChatResponse, String> {
    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no user profile yet".to_string())?;

    let api_key = match profile.llm_provider.as_str() {
        "claude" | "openai" => secrets::get_api_key(&profile.llm_provider),
        _ => None,
    };

    let provider = llm::build(
        &profile.llm_provider,
        api_key.as_deref(),
        profile.ollama_url.as_deref(),
        profile.model.as_deref(),
        state.http.clone(),
    )
    .map_err(|e| e.to_string())?;

    match provider.chat(payload.messages, payload.options).await {
        Ok(r) => {
            state.health.clear(&app);
            Ok(r)
        }
        Err(e) => {
            let msg = e.to_string();
            let kind = crate::health::classify_llm_error(&msg);
            state.health.report(&app, kind, format!("Chat call failed: {msg}"));
            Err(msg)
        }
    }
}
