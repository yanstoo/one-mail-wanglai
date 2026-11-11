//! 翻译相关的命令入口。

use serde_json::Value;
use tauri::State;

use crate::{db, state::AppState, translate};

#[tauri::command]
pub async fn translate_text(
    state: State<'_, AppState>,
    text: String,
    backend: Option<String>,
) -> Result<Value, String> {
    let translated = translate::translate(&state, &text, backend.as_deref()).await?;
    Ok(serde_json::json!({ "text": translated }))
}

#[tauri::command]
pub fn translate_settings_get(state: State<'_, AppState>) -> Result<Value, String> {
    let connection = db::open(&state)?;
    Ok(translate::settings_to_json(&connection))
}

#[tauri::command]
pub fn translate_settings_save(
    state: State<'_, AppState>,
    input: Value,
) -> Result<Value, String> {
    let connection = db::open(&state)?;
    translate::save_settings(&connection, &input)
}
