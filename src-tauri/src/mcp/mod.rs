//! MCP（Model Context Protocol）服务端：让外部 AI 客户端读取邮件、写草稿。
//!
//! 传输：仅监听 `127.0.0.1`，走 MCP 的 Streamable HTTP 风格（POST JSON-RPC，JSON 响应），
//! 鉴权用 `Authorization: Bearer <token>`（也接受 `?token=` 便于简单客户端）。
//!
//! 权限分三档，写在应用设置里：
//! - 读取邮件 / 状态：始终允许（服务启用即生效）
//! - 写草稿：`allow_draft`
//! - 发送邮件：`allow_send`（默认关闭；打开后 AI 才能真的把草稿发出去）
//!
//! 附件二进制不通过 MCP 暴露，只给元数据。

use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::{commands::data::compose, db, mail::smtp, state::AppState};

const PROTOCOL_VERSION: &str = "2024-11-05";
const DEFAULT_PORT: u16 = 8787;
const SETTING_PREFIX: &str = "mcp.";
const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

/// 运行中的服务实例；重启时 abort 旧任务。
static CURRENT: std::sync::Mutex<Option<RunningServer>> = std::sync::Mutex::new(None);
static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);

struct RunningServer {
    port: u16,
    handle: tauri::async_runtime::JoinHandle<()>,
}

#[derive(Clone, Debug)]
pub struct McpSettings {
    pub enabled: bool,
    pub port: u16,
    pub token: String,
    pub allow_draft: bool,
    pub allow_send: bool,
}

fn read_setting(connection: &Connection, name: &str) -> Option<String> {
    connection
        .query_row(
            "SELECT setting_value FROM onemail_app_settings WHERE setting_key=?1",
            [format!("{SETTING_PREFIX}{name}")],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn write_setting(connection: &Connection, name: &str, value: Option<&str>) -> Result<(), String> {
    let key = format!("{SETTING_PREFIX}{name}");
    match value {
        Some(text) => connection
            .execute(
                "INSERT INTO onemail_app_settings (setting_key,setting_value,value_type)
                 VALUES (?1,?2,'string')
                 ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value,
                   value_type=excluded.value_type,
                   updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                params![key, text],
            )
            .map(|_| ())
            .map_err(|error| format!("保存 MCP 设置失败：{error}")),
        None => connection
            .execute("DELETE FROM onemail_app_settings WHERE setting_key=?1", [key])
            .map(|_| ())
            .map_err(|error| format!("保存 MCP 设置失败：{error}")),
    }
}

fn new_token() -> String {
    // 32 字节十六进制；不引额外依赖，用时间戳 + 计数器 + 随机数拼接后做散列式混淆
    let stamp = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let nonce = REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut seed = format!("{stamp}-{nonce}-{:?}-{:?}", std::process::id(), std::time::UNIX_EPOCH);
    let mut token = String::with_capacity(64);
    while token.len() < 64 {
        // FNV-1a 一轮一轮地搅，得到稳定且不可预测的派生串
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in seed.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        token.push_str(&format!("{hash:016x}"));
        seed.push_str(&format!("{hash:016x}"));
    }
    token.truncate(64);
    token
}

pub fn load_settings(connection: &Connection) -> McpSettings {
    let port = read_setting(connection, "port")
        .and_then(|value| value.parse::<u16>().ok())
        .filter(|port| *port >= 1024)
        .unwrap_or(DEFAULT_PORT);
    let token = read_setting(connection, "token").unwrap_or_else(|| {
        // 首次读取即生成，保证开启服务时一定有令牌可展示
        let generated = new_token();
        let _ = write_setting(connection, "token", Some(&generated));
        generated
    });
    McpSettings {
        enabled: read_setting(connection, "enabled").as_deref() == Some("1"),
        port,
        token,
        allow_draft: read_setting(connection, "allow_draft").as_deref() != Some("0"),
        allow_send: read_setting(connection, "allow_send").as_deref() == Some("1"),
    }
}

pub fn settings_to_json(connection: &Connection) -> Value {
    let settings = load_settings(connection);
    json!({
        "enabled": settings.enabled,
        "port": settings.port,
        "token": settings.token,
        "allowDraft": settings.allow_draft,
        "allowSend": settings.allow_send,
        "running": current_port().is_some(),
        "url": format!("http://127.0.0.1:{}/mcp", current_port().unwrap_or(settings.port)),
    })
}

pub fn save_settings(connection: &Connection, input: &Value) -> Result<(), String> {
    if let Some(enabled) = input.get("enabled").and_then(Value::as_bool) {
        write_setting(connection, "enabled", Some(if enabled { "1" } else { "0" }))?;
    }
    if let Some(port) = input.get("port").and_then(Value::as_i64) {
        if !(1024..=65535).contains(&port) {
            return Err("端口需要落在 1024–65535 之间。".to_string());
        }
        write_setting(connection, "port", Some(&port.to_string()))?;
    }
    if let Some(draft) = input.get("allowDraft").and_then(Value::as_bool) {
        write_setting(connection, "allow_draft", Some(if draft { "1" } else { "0" }))?;
    }
    if let Some(send) = input.get("allowSend").and_then(Value::as_bool) {
        write_setting(connection, "allow_send", Some(if send { "1" } else { "0" }))?;
    }
    if input.get("regenerateToken").and_then(Value::as_bool) == Some(true) {
        let token = new_token();
        write_setting(connection, "token", Some(&token))?;
    }
    Ok(())
}

pub fn current_port() -> Option<u16> {
    CURRENT.lock().ok().and_then(|guard| guard.as_ref().map(|run| run.port))
}

/// 按设置启动或停止服务；设置保存后调用。
pub fn restart(app: &AppHandle) -> Result<(), String> {
    stop();
    let state = app.state::<AppState>();
    let connection = db::open(&state)?;
    let settings = load_settings(&connection);
    drop(connection);
    if !settings.enabled {
        return Ok(());
    }
    start(app, settings)
}

pub fn stop() {
    if let Ok(mut guard) = CURRENT.lock() {
        if let Some(run) = guard.take() {
            run.handle.abort();
        }
    }
}

fn start(app: &AppHandle, settings: McpSettings) -> Result<(), String> {
    // 先在同步上下文里探一下端口，好把「端口被占用」这类错误直接返回给设置界面；
    // 真正的监听必须在 tokio 运行时内创建（from_std 不能在主线程调用）。
    std::net::TcpListener::bind(("127.0.0.1", settings.port))
        .map_err(|error| format!("MCP 端口 {} 无法监听：{error}", settings.port))?;

    let app = app.clone();
    let token = settings.token.clone();
    let port = settings.port;
    let handle = tauri::async_runtime::spawn(async move {
        let listener = match TcpListener::bind(("127.0.0.1", port)).await {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("OneMail MCP 监听失败（端口 {port}）：{error}");
                return;
            }
        };
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let app = app.clone();
                    let token = token.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Err(error) = handle_connection(stream, &app, &token).await {
                            eprintln!("OneMail MCP 请求失败：{error}");
                        }
                    });
                }
                Err(error) => {
                    eprintln!("OneMail MCP accept 失败：{error}");
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                }
            }
        }
    });
    if let Ok(mut guard) = CURRENT.lock() {
        *guard = Some(RunningServer { port, handle });
    }
    Ok(())
}

async fn handle_connection(mut stream: TcpStream, app: &AppHandle, token: &str) -> Result<(), String> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end;
    loop {
        let read = stream.read(&mut chunk).await.map_err(|error| error.to_string())?;
        if read == 0 {
            return Ok(());
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(position) = find_header_end(&buffer) {
            header_end = position;
            break;
        }
        if buffer.len() > 64 * 1024 {
            return respond(&mut stream, 431, "application/json", &error_body("请求头过大")).await;
        }
    }

    let head = String::from_utf8_lossy(&buffer[..header_end]).to_string();
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_string();
    let target = parts.next().unwrap_or("/mcp").to_string();

    let mut content_length = 0usize;
    let mut authorized = false;
    let mut query_token: Option<String> = None;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name == "content-length" {
            content_length = value.parse::<usize>().unwrap_or(0);
        } else if name == "authorization" {
            let expected = format!("Bearer {token}");
            authorized = value == expected;
        }
    }
    if let Some(index) = target.find("token=") {
        let rest = &target[index + 6..];
        let end = rest.find('&').unwrap_or(rest.len());
        query_token = Some(rest[..end].to_string());
    }
    if query_token.as_deref() == Some(token) {
        authorized = true;
    }

    if method == "GET" && target.starts_with("/health") {
        return respond(&mut stream, 200, "application/json", &json!({"ok": true})).await;
    }
    if !authorized {
        return respond(
            &mut stream,
            401,
            "application/json",
            &error_body("缺少或错误的访问令牌（Authorization: Bearer <token>）"),
        )
        .await;
    }
    if method != "POST" {
        return respond(&mut stream, 405, "application/json", &error_body("仅支持 POST JSON-RPC")).await;
    }

    let mut body = buffer[header_end + 4..].to_vec();
    while body.len() < content_length {
        let read = stream.read(&mut chunk).await.map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        body.extend_from_slice(&chunk[..read]);
        if body.len() > MAX_BODY_BYTES {
            return respond(&mut stream, 413, "application/json", &error_body("请求体过大")).await;
        }
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            return respond(
                &mut stream,
                200,
                "application/json",
                &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":format!("JSON 解析失败：{error}")}}),
            )
            .await;
        }
    };

    match dispatch(app, payload).await {
        Some(response) => respond(&mut stream, 200, "application/json", &response).await,
        // 通知类消息按 JSON-RPC 约定不返回内容
        None => respond(&mut stream, 202, "application/json", &json!({})).await,
    }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn respond(
    stream: &mut TcpStream,
    status: u16,
    content_type: &str,
    body: &Value,
) -> Result<(), String> {
    let payload = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    let reason = match status {
        200 => "OK",
        202 => "Accepted",
        401 => "Unauthorized",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        _ => "OK",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    stream
        .write_all(head.as_bytes())
        .await
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&payload)
        .await
        .map_err(|error| error.to_string())?;
    let _ = stream.flush().await;
    Ok(())
}

fn error_body(message: &str) -> Value {
    json!({ "error": message })
}

async fn dispatch(app: &AppHandle, payload: Value) -> Option<Value> {
    let id = payload.get("id").cloned();
    let method = payload.get("method").and_then(Value::as_str).unwrap_or_default().to_string();
    let params = payload.get("params").cloned().unwrap_or_else(|| json!({}));

    // 通知（没有 id）不需要回包
    let is_notification = id.is_none() || id == Some(Value::Null);

    let result = match method.as_str() {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "one-mail-wanglai", "version": env!("CARGO_PKG_VERSION") }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_definitions() })),
        "resources/list" => Ok(json!({ "resources": [] })),
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "tools/call" => call_tool(app, &params).await,
        other => Err(format!("不支持的方法：{other}")),
    };

    if is_notification {
        return None;
    }

    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": message }
        }),
    })
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "list_accounts",
            "description": "列出已配置的邮箱账号（邮箱地址、别名、徽标、同步状态、是否启用）。",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "mail_status",
            "description": "查看邮箱整体状态：每个账号的未读/总数、最近同步时间、错误信息，以及 MCP 的写/发权限。",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "list_correspondents",
            "description": "按往来联系人列出（一人一个邮箱地址），带未读数与最近邮件时间。group 可选 normal 或 junk。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "group": { "type": "string", "enum": ["normal", "junk"] },
                    "keyword": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "list_messages",
            "description": "列出邮件摘要，可按联系人、账号、关键词或只看未读过滤，按最新在前排序。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "correspondentId": { "type": "integer" },
                    "accountId": { "type": "integer" },
                    "keyword": { "type": "string" },
                    "unreadOnly": { "type": "boolean" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 200 }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "get_message",
            "description": "读取单封邮件的完整信息：主题、发件人、收件人、抄送、时间、已读/垃圾状态、正文纯文本与附件元数据。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "messageId": { "type": "integer" },
                    "maxChars": { "type": "integer", "minimum": 200, "maximum": 100000 }
                },
                "required": ["messageId"],
                "additionalProperties": false
            }
        },
        {
            "name": "create_draft",
            "description": "新建草稿（不会发送）。需要允许写草稿。accountId 省略时用第一个可用账号。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "accountId": { "type": "integer" },
                    "to": { "type": "array", "items": { "type": "string" } },
                    "cc": { "type": "array", "items": { "type": "string" } },
                    "bcc": { "type": "array", "items": { "type": "string" } },
                    "subject": { "type": "string" },
                    "body": { "type": "string" }
                },
                "required": ["to", "subject", "body"],
                "additionalProperties": false
            }
        },
        {
            "name": "list_drafts",
            "description": "列出发件箱中的草稿（未发送）。",
            "inputSchema": {
                "type": "object",
                "properties": { "limit": { "type": "integer", "minimum": 1, "maximum": 100 } },
                "additionalProperties": false
            }
        },
        {
            "name": "delete_draft",
            "description": "删除指定草稿。需要允许写草稿。",
            "inputSchema": {
                "type": "object",
                "properties": { "outboxId": { "type": "integer" } },
                "required": ["outboxId"],
                "additionalProperties": false
            }
        },
        {
            "name": "send_draft",
            "description": "把指定草稿真正发送出去。仅在「允许 AI 发送邮件」打开时可用。",
            "inputSchema": {
                "type": "object",
                "properties": { "outboxId": { "type": "integer" } },
                "required": ["outboxId"],
                "additionalProperties": false
            }
        }
    ])
}

async fn call_tool(app: &AppHandle, params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "缺少工具名。".to_string())?;
    let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
    let state = app.state::<AppState>();
    let connection = db::open(&state)?;
    let settings = load_settings(&connection);

    let result = match name {
        "list_accounts" => list_accounts(&connection),
        "mail_status" => mail_status(&connection, &settings),
        "list_correspondents" => list_correspondents(&connection, &args),
        "list_messages" => list_messages(&connection, &args),
        "get_message" => {
            drop(connection);
            let value = get_message(&state, &args)?;
            return Ok(text_result(value));
        }
        "create_draft" => {
            if !settings.allow_draft {
                return Err("未授予「写草稿」权限：请在 OneMail 设置 → MCP 中开启。".to_string());
            }
            drop(connection);
            let value = create_draft(&state, &args)?;
            return Ok(text_result(value));
        }
        "list_drafts" => list_drafts(&connection, &args),
        "delete_draft" => {
            if !settings.allow_draft {
                return Err("未授予「写草稿」权限：请在 OneMail 设置 → MCP 中开启。".to_string());
            }
            let outbox_id = require_i64(&args, "outboxId")?;
            drop(connection);
            compose::delete_outbox(&state, outbox_id)?;
            return Ok(text_result(json!({ "deleted": outbox_id })));
        }
        "send_draft" => {
            if !settings.allow_send {
                return Err(
                    "未授予「发送邮件」权限：请在 OneMail 设置 → MCP 中打开（默认关闭）。".to_string(),
                );
            }
            let outbox_id = require_i64(&args, "outboxId")?;
            drop(connection);
            let value = send_draft(app, &state, outbox_id).await?;
            return Ok(text_result(value));
        }
        other => return Err(format!("未知工具：{other}")),
    }?;
    Ok(text_result(result))
}

fn text_result(value: Value) -> Value {
    let pretty = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    json!({
        "content": [{ "type": "text", "text": pretty }],
        "isError": false
    })
}

fn require_i64(args: &Value, name: &str) -> Result<i64, String> {
    args.get(name)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("缺少整数参数：{name}"))
}

fn opt_i64(args: &Value, name: &str) -> Option<i64> {
    args.get(name).and_then(Value::as_i64)
}

fn list_accounts(connection: &Connection) -> Result<Value, String> {
    let mut statement = connection
        .prepare(
            "SELECT account_id,email,account_label,badge_label,badge_color,sync_enabled,status,
                    connection_state,last_sync_at,last_error
             FROM onemail_mail_accounts ORDER BY account_id",
        )
        .map_err(|error| format!("读取账号失败：{error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "accountId": row.get::<_, i64>(0)?,
                "email": row.get::<_, String>(1)?,
                "accountLabel": row.get::<_, Option<String>>(2)?,
                "badge": row.get::<_, Option<String>>(3)?,
                "badgeColor": row.get::<_, Option<String>>(4)?,
                "syncEnabled": row.get::<_, i64>(5)? != 0,
                "status": row.get::<_, String>(6)?,
                "connectionStatus": row.get::<_, String>(7)?,
                "lastSyncAt": row.get::<_, Option<String>>(8)?,
                "lastError": row.get::<_, Option<String>>(9)?
            }))
        })
        .map_err(|error| format!("读取账号失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取账号失败：{error}"))?;
    Ok(json!({ "accounts": rows }))
}

fn mail_status(connection: &Connection, settings: &McpSettings) -> Result<Value, String> {
    let unread: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM onemail_mail_messages WHERE is_read=0 AND remote_deleted=0",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let total: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM onemail_mail_messages WHERE remote_deleted=0",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let junk_unread: i64 = connection
        .query_row(
            "SELECT COALESCE(SUM(junk_unread_count),0) FROM onemail_correspondents",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    let drafts: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM onemail_outbox_messages WHERE status='draft' AND deleted_at IS NULL",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);
    Ok(json!({
        "unread": unread,
        "total": total,
        "junkUnread": junk_unread,
        "drafts": drafts,
        "accounts": list_accounts(connection)?
            .get("accounts")
            .cloned()
            .unwrap_or_else(|| json!([])),
        "permissions": {
            "read": true,
            "draft": settings.allow_draft,
            "send": settings.allow_send
        }
    }))
}

fn list_correspondents(connection: &Connection, args: &Value) -> Result<Value, String> {
    let group = args.get("group").and_then(Value::as_str).unwrap_or("normal");
    let keyword = args.get("keyword").and_then(Value::as_str).map(str::to_string);
    let limit = opt_i64(args, "limit").unwrap_or(50).clamp(1, 200);
    crate::db::correspondents::list(connection, group, keyword.as_deref(), None, limit, None)
}

fn list_messages(connection: &Connection, args: &Value) -> Result<Value, String> {
    let limit = opt_i64(args, "limit").unwrap_or(50).clamp(1, 200);
    let keyword = args
        .get("keyword")
        .and_then(Value::as_str)
        .map(|value| format!("%{}%", value.trim()))
        .filter(|value| value.len() > 2);
    let correspondent_id = opt_i64(args, "correspondentId");
    let account_id = opt_i64(args, "accountId");
    let unread_only = args.get("unreadOnly").and_then(Value::as_bool).unwrap_or(false);

    let mut statement = connection
        .prepare(
            "SELECT m.message_id, m.account_id, m.subject, m.from_name, m.from_email,
                    COALESCE(m.received_at, m.sent_at, m.created_at),
                    COALESCE(m.read_state_override, m.is_read), m.has_attachments, m.snippet,
                    (SELECT f.role FROM onemail_mail_folders f WHERE f.folder_id=m.folder_id)
             FROM onemail_mail_messages m
             WHERE m.remote_deleted=0 AND m.user_deleted=0 AND m.is_deleted=0
               AND (?1 IS NULL OR m.account_id=?1)
               AND (?2 IS NULL OR EXISTS (
                 SELECT 1 FROM onemail_correspondent_messages cm
                 WHERE cm.source='mail' AND cm.message_id=m.message_id
                   AND cm.correspondent_id=?2
               ))
               AND (?3 IS NULL OR m.subject LIKE ?3 OR m.from_email LIKE ?3 OR m.snippet LIKE ?3)
               AND (?4=0 OR COALESCE(m.read_state_override, m.is_read)=0)
             ORDER BY COALESCE(m.received_at, m.sent_at, m.created_at) DESC
             LIMIT ?5",
        )
        .map_err(|error| format!("读取邮件失败：{error}"))?;
    let rows = statement
        .query_map(
            params![account_id, correspondent_id, keyword, i64::from(unread_only), limit],
            |row| {
                Ok(json!({
                    "messageId": row.get::<_, i64>(0)?,
                    "accountId": row.get::<_, i64>(1)?,
                    "subject": row.get::<_, Option<String>>(2)?,
                    "fromName": row.get::<_, Option<String>>(3)?,
                    "fromEmail": row.get::<_, Option<String>>(4)?,
                    "receivedAt": row.get::<_, Option<String>>(5)?,
                    "isRead": row.get::<_, i64>(6).unwrap_or(0) != 0,
                    "hasAttachment": row.get::<_, i64>(7).unwrap_or(0) != 0,
                    "snippet": row.get::<_, Option<String>>(8)?,
                    "folderRole": row.get::<_, Option<String>>(9)?
                }))
            },
        )
        .map_err(|error| format!("读取邮件失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取邮件失败：{error}"))?;
    Ok(json!({ "messages": rows }))
}

fn get_message(state: &AppState, args: &Value) -> Result<Value, String> {
    let message_id = require_i64(args, "messageId")?;
    let max_chars = opt_i64(args, "maxChars").unwrap_or(20000).clamp(200, 100_000) as usize;
    let connection = db::open(state)?;

    let mut value = connection
        .query_row(
            "SELECT m.message_id, m.account_id, m.subject, m.from_name, m.from_email,
                    COALESCE(m.received_at, m.sent_at, m.created_at),
                    COALESCE(m.read_state_override, m.is_read), m.snippet, m.has_attachments,
                    (SELECT f.role FROM onemail_mail_folders f WHERE f.folder_id=m.folder_id)
             FROM onemail_mail_messages m WHERE m.message_id=?1",
            [message_id],
            |row| {
                Ok(json!({
                    "messageId": row.get::<_, i64>(0)?,
                    "accountId": row.get::<_, i64>(1)?,
                    "subject": row.get::<_, Option<String>>(2)?,
                    "fromName": row.get::<_, Option<String>>(3)?,
                    "fromEmail": row.get::<_, Option<String>>(4)?,
                    "receivedAt": row.get::<_, Option<String>>(5)?,
                    "isRead": row.get::<_, i64>(6).unwrap_or(0) != 0,
                    "snippet": row.get::<_, Option<String>>(7)?,
                    "hasAttachment": row.get::<_, i64>(8).unwrap_or(0) != 0,
                    "folderRole": row.get::<_, Option<String>>(9)?
                }))
            },
        )
        .optional()
        .map_err(|error| format!("读取邮件失败：{error}"))?
        .ok_or_else(|| format!("邮件不存在：{message_id}"))?;

    let mut recipients = Vec::new();
    if let Ok(mut statement) = connection.prepare(
        "SELECT kind,email,name FROM onemail_message_addresses WHERE message_id=?1 ORDER BY kind,id",
    ) {
        if let Ok(rows) = statement.query_map([message_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        }) {
            for row in rows.flatten() {
                recipients.push(json!({ "kind": row.0, "email": row.1, "name": row.2 }));
            }
        }
    }
    value["recipients"] = Value::Array(recipients);

    let body: Option<String> = connection
        .query_row(
            "SELECT body_text FROM onemail_message_bodies WHERE message_id=?1",
            [message_id],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();
    let (text, truncated) = match body {
        Some(text) if text.chars().count() > max_chars => {
            (text.chars().take(max_chars).collect::<String>(), true)
        }
        Some(text) => (text, false),
        None => (String::new(), false),
    };
    value["bodyText"] = Value::String(text);
    value["bodyTruncated"] = Value::Bool(truncated);
    value["bodyCached"] = Value::Bool(!value["bodyText"].as_str().unwrap_or("").is_empty());

    let mut attachments = Vec::new();
    if let Ok(mut statement) = connection.prepare(
        "SELECT filename,mime_type,size_bytes FROM onemail_message_attachments
         WHERE message_id=?1 ORDER BY attachment_id",
    ) {
        if let Ok(rows) = statement.query_map([message_id], |row| {
            Ok(json!({
                "filename": row.get::<_, String>(0)?,
                "mimeType": row.get::<_, Option<String>>(1)?,
                "sizeBytes": row.get::<_, i64>(2)?
            }))
        }) {
            for row in rows.flatten() {
                attachments.push(row);
            }
        }
    }
    value["attachments"] = Value::Array(attachments);
    Ok(value)
}

fn create_draft(state: &AppState, args: &Value) -> Result<Value, String> {
    let connection = db::open(state)?;
    let account_id = match opt_i64(args, "accountId") {
        Some(id) => id,
        None => connection
            .query_row(
                "SELECT account_id FROM onemail_mail_accounts ORDER BY account_id LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| format!("读取账号失败：{error}"))?
            .ok_or_else(|| "还没有配置任何邮箱账号。".to_string())?,
    };
    drop(connection);

    let to = string_array(args, "to");
    if to.is_empty() {
        return Err("收件人不能为空。".to_string());
    }
    let input = json!({
        "accountId": account_id,
        "mode": "new",
        "to": to.iter().map(|address| json!({ "email": address })).collect::<Vec<_>>(),
        "cc": string_array(args, "cc").iter().map(|address| json!({ "email": address })).collect::<Vec<_>>(),
        "bcc": string_array(args, "bcc").iter().map(|address| json!({ "email": address })).collect::<Vec<_>>(),
        "subject": args.get("subject").and_then(Value::as_str).unwrap_or(""),
        "bodyText": args.get("body").and_then(Value::as_str).unwrap_or("")
    });
    let saved = compose::save_draft(state, input)?;
    Ok(json!({
        "outboxId": saved.get("outboxId").cloned().unwrap_or(Value::Null),
        "accountId": account_id,
        "to": to,
        "subject": args.get("subject").cloned().unwrap_or(Value::Null),
        "status": "draft"
    }))
}

fn string_array(args: &Value, name: &str) -> Vec<String> {
    args.get(name)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn list_drafts(connection: &Connection, args: &Value) -> Result<Value, String> {
    let limit = opt_i64(args, "limit").unwrap_or(20).clamp(1, 100);
    let mut statement = connection
        .prepare(
            "SELECT outbox_id,account_id,to_json,cc_json,bcc_json,subject,body_text,created_at,updated_at
             FROM onemail_outbox_messages
             WHERE status='draft' AND deleted_at IS NULL
             ORDER BY COALESCE(updated_at, created_at) DESC LIMIT ?1",
        )
        .map_err(|error| format!("读取草稿失败：{error}"))?;
    let rows = statement
        .query_map([limit], |row| {
            let body: Option<String> = row.get(6)?;
            Ok(json!({
                "outboxId": row.get::<_, i64>(0)?,
                "accountId": row.get::<_, i64>(1)?,
                "to": row.get::<_, String>(2)?,
                "cc": row.get::<_, String>(3)?,
                "bcc": row.get::<_, String>(4)?,
                "subject": row.get::<_, Option<String>>(5)?,
                "bodyPreview": body.unwrap_or_default().chars().take(200).collect::<String>(),
                "createdAt": row.get::<_, Option<String>>(7)?,
                "updatedAt": row.get::<_, Option<String>>(8)?
            }))
        })
        .map_err(|error| format!("读取草稿失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取草稿失败：{error}"))?;
    Ok(json!({ "drafts": rows }))
}

async fn send_draft(app: &AppHandle, state: &AppState, outbox_id: i64) -> Result<Value, String> {
    let connection = db::open(state)?;
    let draft = compose::get_outbox(&connection, outbox_id)?
        .ok_or_else(|| format!("草稿不存在：{outbox_id}"))?;
    drop(connection);
    if draft.get("status").and_then(Value::as_str) != Some("draft") {
        return Err("只能发送状态为 draft 的记录。".to_string());
    }
    let mut input = draft.clone();
    input["mode"] = json!("new");
    let result = smtp::send_message(state, input).await?;
    let _ = tauri::Emitter::emit(
        app,
        "compose/sent",
        serde_json::json!({ "outboxId": outbox_id, "accountId": draft.get("accountId") }),
    );
    if let Some(account_id) = draft.get("accountId").and_then(Value::as_i64) {
        crate::commands::data::correspondents::reindex_account(state, account_id);
    }
    Ok(json!({ "sent": true, "outboxId": outbox_id, "result": result }))
}
