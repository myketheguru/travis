//! Tauri commands for MCP server management.

use tauri::State;

use crate::mcp::client::McpClient;
use crate::mcp::db::{self, McpServer};
use crate::AppState;

#[tauri::command]
pub async fn mcp_list_servers(state: State<'_, AppState>) -> Result<Vec<McpServer>, String> {
    db::list(&state.db.pool).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_add_server(
    state: State<'_, AppState>,
    slug: String,
    label: String,
    url: String,
    auth_token: Option<String>,
) -> Result<i64, String> {
    db::upsert(&state.db.pool, &slug, &label, &url, auth_token.as_deref(), true)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_delete_server(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    db::delete(&state.db.pool, id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_set_enabled(
    state: State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    db::set_enabled(&state.db.pool, id, enabled)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn mcp_ping_server(
    state: State<'_, AppState>,
    url: String,
    auth_token: Option<String>,
) -> Result<Vec<String>, String> {
    let client = McpClient::new(state.http.clone(), url, auth_token);
    let tools = client.list_tools().await.map_err(|e| e.to_string())?;
    Ok(tools.into_iter().map(|t| t.name).collect())
}
