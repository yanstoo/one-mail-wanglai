//! 往来联系人相关命令：左栏列表、分组角标、中栏往来时间线。

use serde_json::{json, Value};
use tauri::State;

use crate::{
    db::{self, correspondents},
    state::AppState,
};

fn string_field(query: &Value, key: &str) -> Option<String> {
    query
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn limit_field(query: &Value, fallback: i64) -> i64 {
    query
        .get("limit")
        .and_then(Value::as_i64)
        .unwrap_or(fallback)
}

/// 左栏联系人列表。
/// - `group`: `normal`（普通组）或 `junk`（垃圾组），只影响归属、排序与角标取数
/// - `keyword`: 匹配地址或显示名
/// - `cursor`: 上一页返回的 `nextCursor`
#[tauri::command]
pub fn correspondents_list(
    state: State<'_, AppState>,
    query: Option<Value>,
) -> Result<Value, String> {
    let query = query.unwrap_or_else(|| json!({}));
    let group = string_field(&query, "group").unwrap_or_else(|| correspondents::GROUP_NORMAL.to_owned());
    let group = if group == correspondents::GROUP_JUNK {
        correspondents::GROUP_JUNK
    } else {
        correspondents::GROUP_NORMAL
    };
    let keyword = string_field(&query, "keyword");
    let cursor = string_field(&query, "cursor");
    let limit = limit_field(&query, 30);
    let account = query.get("accountId").and_then(Value::as_i64);

    let connection = db::open(&state)?;
    correspondents::list(
        &connection,
        group,
        keyword.as_deref(),
        cursor.as_deref(),
        limit,
        account,
    )
}

/// 中栏：与某个联系人的全部往来（含垃圾与我的去信），按时间倒序返回，配合 `before` 向上翻页。
#[tauri::command]
pub fn correspondents_thread(
    state: State<'_, AppState>,
    correspondent_id: i64,
    before: Option<String>,
    limit: Option<i64>,
) -> Result<Value, String> {
    let connection = db::open(&state)?;
    correspondents::thread(
        &connection,
        correspondent_id,
        before.as_deref().map(str::trim).filter(|value| !value.is_empty()),
        limit.unwrap_or(100),
    )
}

/// 左栏两个分组 tab 的角标（普通取普通未读、垃圾取垃圾未读，二者不相加）。
#[tauri::command]
pub fn correspondents_group_counts(state: State<'_, AppState>) -> Result<Value, String> {
    let connection = db::open(&state)?;
    correspondents::group_counts(&connection)
}

/// 同步/发信/已读变更后由内部调用：重建单个账号的联系人索引。
pub(crate) fn reindex_account(state: &AppState, account_id: i64) {
    match db::open(state) {
        Ok(connection) => {
            if let Err(error) = correspondents::reindex_account(&connection, account_id) {
                eprintln!("OneMail correspondent reindex failed for account {account_id}: {error}");
            }
        }
        Err(error) => eprintln!("OneMail correspondent reindex open failed: {error}"),
    }
}

/// 设置联系人备注名；传空串或 null 表示清除。
#[tauri::command]
pub fn correspondents_set_alias(
    state: State<'_, AppState>,
    correspondent_id: i64,
    alias: Option<String>,
) -> Result<Value, String> {
    let connection = db::open(&state)?;
    correspondents::set_alias(&connection, correspondent_id, alias.as_deref())?;
    let stored: Option<String> = connection
        .query_row(
            "SELECT alias FROM onemail_correspondents WHERE correspondent_id=?1",
            [correspondent_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("读取备注名失败：{error}"))?;
    Ok(json!({ "correspondentId": correspondent_id, "alias": stored }))
}
