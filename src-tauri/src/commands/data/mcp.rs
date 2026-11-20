//! MCP（Model Context Protocol）设置的命令入口。

use serde_json::Value;
use tauri::{AppHandle, State};

use crate::{db, mcp, state::AppState};

#[tauri::command]
pub fn mcp_settings_get(state: State<'_, AppState>) -> Result<Value, String> {
    let connection = db::open(&state)?;
    Ok(mcp::settings_to_json(&connection))
}

#[tauri::command]
pub fn mcp_settings_save(
    app: AppHandle,
    state: State<'_, AppState>,
    input: Value,
) -> Result<Value, String> {
    {
        let connection = db::open(&state)?;
        mcp::save_settings(&connection, &input)?;
    }
    // 保存后立刻按新配置启停服务
    mcp::restart(&app)?;
    let connection = db::open(&state)?;
    Ok(mcp::settings_to_json(&connection))
}
