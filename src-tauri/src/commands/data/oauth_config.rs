//! Google OAuth 客户端 ID 的本地配置。
//!
//! 背景：Gmail 走 OAuth 需要「自己的」Google 桌面客户端 ID。上游实现只从环境变量
//! `ONEMAIL_GOOGLE_CLIENT_ID` 读取（微软那边有内置默认值，所以 Outlook 开箱可用），
//! 于是包装成桌面应用后用户无法完成 Google 授权。
//!
//! 这里增加一条本地持久化通道：把 Client ID 存进 `onemail_app_settings`，
//! 启动时若环境变量为空则注入进程环境，让 `oauth::provider` 无需改动即可使用。

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use tauri::State;

use crate::{db, state::AppState};

const SETTING_KEY: &str = "oauth.google_client_id";
const ENV_KEY: &str = "ONEMAIL_GOOGLE_CLIENT_ID";

/// 应用内保存的 Client ID。
///
/// 不往进程环境变量里写：`std::env::set_var` 在多线程下修改环境是不安全的
/// （其它线程可能正在读 environ），这里用进程内静态存储，读取方走
/// `google_client_id_override()`。
static GOOGLE_CLIENT_ID_OVERRIDE: std::sync::RwLock<Option<String>> = std::sync::RwLock::new(None);

/// 供 `oauth::provider` 读取的应用内 Client ID。
pub(crate) fn google_client_id_override() -> Option<String> {
    GOOGLE_CLIENT_ID_OVERRIDE
        .read()
        .ok()
        .and_then(|value| value.clone())
        .filter(|value| !value.trim().is_empty())
}

fn set_google_client_id_override(value: Option<String>) {
    if let Ok(mut guard) = GOOGLE_CLIENT_ID_OVERRIDE.write() {
        *guard = value;
    }
}

/// 读取本地保存的 Client ID（不含环境变量）。
pub(crate) fn stored_google_client_id(connection: &Connection) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT setting_value FROM onemail_app_settings WHERE setting_key=?1",
            [SETTING_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map(|value| {
            value
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
        })
        .map_err(|error| format!("读取 Google 客户端 ID 失败：{error}"))
}

fn write_stored_google_client_id(connection: &Connection, value: Option<&str>) -> Result<(), String> {
    match value {
        Some(client_id) => connection
            .execute(
                "INSERT INTO onemail_app_settings (setting_key,setting_value,value_type)
                 VALUES (?1,?2,'string')
                 ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value,
                   value_type=excluded.value_type,
                   updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                params![SETTING_KEY, client_id],
            )
            .map(|_| ())
            .map_err(|error| format!("保存 Google 客户端 ID 失败：{error}")),
        None => connection
            .execute(
                "DELETE FROM onemail_app_settings WHERE setting_key=?1",
                [SETTING_KEY],
            )
            .map(|_| ())
            .map_err(|error| format!("清除 Google 客户端 ID 失败：{error}")),
    }
}

/// 启动时把本地保存的 Client ID 注入进程环境（环境变量优先，不覆盖用户显式设置的值）。
pub(crate) fn apply_stored_google_client_id(state: &AppState) {
    let env_present = std::env::var(ENV_KEY)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if env_present {
        return;
    }
    let Ok(connection) = db::open(state) else {
        return;
    };
    match stored_google_client_id(&connection) {
        Ok(Some(client_id)) => set_google_client_id_override(Some(client_id)),
        Ok(None) => {}
        Err(error) => eprintln!("OneMail google client id restore failed: {error}"),
    }
}

/// 读取当前生效的 Google OAuth 客户端 ID 及其来源（`env` / `stored` / `none`）。
#[tauri::command]
pub fn oauth_google_client_id(state: State<'_, AppState>) -> Result<Value, String> {
    let from_env = std::env::var(ENV_KEY)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    let connection = db::open(&state)?;
    let stored = stored_google_client_id(&connection)?;

    let (client_id, source) = match (&from_env, &stored) {
        (Some(value), _) => (Some(value.clone()), "env"),
        (None, Some(value)) => (Some(value.clone()), "stored"),
        (None, None) => (None, "none"),
    };
    // 环境变量优先级最高；本地保存值同时用于进程内覆盖

    Ok(json!({ "clientId": client_id, "source": source }))
}

/// 保存（或清除）Google OAuth 客户端 ID；保存后立即注入进程环境，无需重启应用。
#[tauri::command]
pub fn oauth_set_google_client_id(
    state: State<'_, AppState>,
    client_id: Option<String>,
) -> Result<Value, String> {
    let normalized = client_id
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());

    if let Some(value) = &normalized {
        if value.contains(char::is_whitespace) {
            return Err("Google 客户端 ID 不能包含空格。".to_string());
        }
        // 桌面客户端 ID 形如 1234567890-abc123.apps.googleusercontent.com
        if !value.ends_with(".apps.googleusercontent.com") {
            return Err(
                "这看起来不是 Google 桌面 OAuth 客户端 ID（应以 .apps.googleusercontent.com 结尾）。"
                    .to_string(),
            );
        }
    }

    let connection = db::open(&state)?;
    write_stored_google_client_id(&connection, normalized.as_deref())?;

    set_google_client_id_override(normalized.clone());

    Ok(json!({ "clientId": normalized, "saved": normalized.is_some() }))
}
