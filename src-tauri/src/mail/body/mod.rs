pub(crate) mod format;
mod participants;

pub(crate) use format::html_to_text;
use format::{normalize_body_text, sanitize_html};

pub(crate) use participants::{first_header_address, parse_participant_headers, participants_need_repair, persist_participants, ParticipantHeader};

use mailparse::{
    parse_mail, DispositionType, MailHeader, MailHeaderMap, ParsedMail,
};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

use crate::{
    db,
    mail::transport::{self as mail_transport, MailAccount},
    state::AppState,
};

#[derive(Default)]
struct ParsedMessageBody {
    raw_headers: String,
    headers: ParsedMessageHeaders,
    text: Option<String>,
    html: Option<String>,
    attachments: Vec<ParsedAttachment>,
}

#[derive(Default)]
struct ParsedMessageHeaders {
    subject: Option<String>,
    message_id: Option<String>,
    from_name: Option<String>,
    from_email: Option<String>,
    received_at: Option<String>,
    in_reply_to: Option<String>,
    references_header: Option<String>,
    participants: Vec<ParticipantHeader>,
}

struct ParsedAttachment {
    filename: String,
    mime_type: String,
    disposition: String,
    size_bytes: usize,
}


/// 在原文件夹找不到时，去该账号的其他文件夹按 Message-ID 重新定位。
/// 返回 (文件夹路径, uid)；命中后调用方负责把新的 folder_id/uid 写回本地库。

/// 邮件被移动到别的文件夹后，把新的 folder_id/uid 写回本地记录。
/// 邮件被移动到别的文件夹后，把新的 folder_id/uid 写回本地记录。
///
/// 参数是 RFC822 Message-ID（字符串），不是 mail_messages.message_id；
/// 按 Message-ID 定位行、按该行取 account_id。
fn rebind_message_location(
    state: &AppState,
    rfc822_message_id: &str,
    folder_path: &str,
    uid: u32,
) -> Result<(), String> {
    let connection = db::open(state)?;
    let account_id: i64 = connection
        .query_row(
            "SELECT account_id FROM onemail_mail_messages WHERE rfc822_message_id=?1 LIMIT 1",
            [rfc822_message_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("读取邮件所属账号失败：{error}"))?;

    let existing: Option<i64> = connection
        .query_row(
            "SELECT folder_id FROM onemail_mail_folders WHERE account_id=?1 AND path=?2",
            rusqlite::params![account_id, folder_path],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| format!("读取文件夹失败：{error}"))?;

    let folder_id = match existing {
        Some(id) => id,
        None => {
            // 这个文件夹之前没同步过：补一条记录，保证下次直接命中
            connection
                .execute(
                    "INSERT INTO onemail_mail_folders (account_id,path,name,role,sync_enabled,sort_order)
                     VALUES (?1,?2,?2,'custom',0,99)
                     ON CONFLICT(account_id,path) DO NOTHING",
                    rusqlite::params![account_id, folder_path],
                )
                .map_err(|error| format!("登记新文件夹失败：{error}"))?;
            connection
                .query_row(
                    "SELECT folder_id FROM onemail_mail_folders WHERE account_id=?1 AND path=?2",
                    rusqlite::params![account_id, folder_path],
                    |row| row.get(0),
                )
                .map_err(|error| format!("读取文件夹失败：{error}"))?
        }
    };

    connection
        .execute(
            "UPDATE onemail_mail_messages SET folder_id=?2, uid=?3 WHERE rfc822_message_id=?1",
            rusqlite::params![rfc822_message_id, folder_id, i64::from(uid)],
        )
        .map_err(|error| format!("更新邮件位置失败：{error}"))?;
    Ok(())
}

async fn relocate_message(
    state: &AppState,
    locator: &MessageLocator,
    message_id: &str,
) -> Option<(String, u32)> {
    // 服务器上的全部可选文件夹（含从未同步过的），按「最可能包含全量邮件」排序：
    // \All / Archive 最先（这类文件夹通常收拢了所有邮件），其次垃圾箱，最后其余
    let mut folders = match mail_transport::list_all_folders(state, &locator.account).await {
        Ok(list) => list,
        Err(error) => {
            eprintln!("OneMail list folders failed: {error}");
            Vec::new()
        }
    };
    if folders.is_empty() {
        // 列举失败时退回本地已知文件夹
        if let Ok(connection) = db::open(state) {
            if let Ok(mut statement) = connection.prepare(
                "SELECT path, role FROM onemail_mail_folders WHERE account_id=?1",
            ) {
                if let Ok(rows) = statement.query_map(
                    rusqlite::params![locator.account.account_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                ) {
                    folders = rows.filter_map(Result::ok).collect();
                }
            }
        }
    }

    let rank = |role: &str| match role {
        "all_mail" => 0,
        "archive" => 1,
        "junk" => 2,
        "trash" | "drafts" => 9,
        _ => 5,
    };
    folders.sort_by_key(|(_, role)| rank(role));

    for (folder, role) in folders {
        if role == "drafts" || role == "trash" || folder == locator.folder_path {
            continue;
        }
        match mail_transport::search_uid_by_message_id(state, &locator.account, &folder, message_id)
            .await
        {
            Ok(Some(uid)) => return Some((folder, uid)),
            Ok(None) => continue,
            Err(error) => eprintln!("OneMail relocate search failed in {folder}: {error}"),
        }
    }
    None
}


/// 同步后预取正文的限额：只补最近 30 天、单封 ≤5MB 的邮件。
const PREFETCH_WINDOW_DAYS: i64 = 30;
const PREFETCH_MAX_BYTES: i64 = 5 * 1024 * 1024;
/// 每轮最多补多少封，避免一次同步把带宽打满。
const PREFETCH_BATCH: usize = 40;

/// 列出需要预取正文的邮件（本地还没有正文、体积在限额内、发生在窗口期内）。
fn prefetch_candidates(state: &AppState, account_id: Option<i64>) -> Result<Vec<i64>, String> {
    let connection = db::open(state)?;
    let mut statement = connection
        .prepare(
            "SELECT m.message_id
             FROM onemail_mail_messages m
             JOIN onemail_mail_folders f ON f.folder_id = m.folder_id
             LEFT JOIN onemail_message_bodies b ON b.message_id = m.message_id
             WHERE b.message_id IS NULL
               AND m.is_draft = 0 AND m.user_deleted = 0 AND m.is_deleted = 0
               AND f.role NOT IN ('drafts','trash')
               -- 体积未知（0）也纳入：很多服务器不返回 size，漏掉会导致永远不预取
               AND (m.size_bytes = 0 OR m.size_bytes <= ?1)
               AND COALESCE(m.received_at, m.sent_at, m.internal_date) >=
                   strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?2)
               AND (?3 IS NULL OR m.account_id = ?3)
             ORDER BY COALESCE(m.received_at, m.sent_at, m.internal_date) DESC
             LIMIT ?4",
        )
        .map_err(|error| format!("读取预取列表失败：{error}"))?;
    let rows = statement
        .query_map(
            rusqlite::params![
                PREFETCH_MAX_BYTES,
                format!("-{PREFETCH_WINDOW_DAYS} day"),
                account_id,
                PREFETCH_BATCH as i64
            ],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("读取预取列表失败：{error}"))?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    Ok(rows)
}

/// 预取正文：同步结束后调用，把窗口期内、体积可接受的邮件正文提前落库，
/// 这样打开邮件不再依赖服务器文件夹状态。



/// 「下载」：把附件保存到用户的下载目录（重名自动加序号），返回最终路径。
pub async fn download_attachment(
    state: &AppState,
    message_id: i64,
    index: usize,
) -> Result<String, String> {
    let (filename, _, bytes) = read_attachment(state, message_id, index).await?;
    let dir = download_dir();
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建下载目录失败：{error}"))?;

    let safe_name: String = filename
        .chars()
        .map(|ch| if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { ch })
        .collect();
    let stem = Path::new(&safe_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("attachment")
        .to_string();
    let ext = Path::new(&safe_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{value}"))
        .unwrap_or_default();

    let mut target = dir.join(format!("{stem}{ext}"));
    let mut counter = 1;
    while target.exists() {
        target = dir.join(format!("{stem} ({counter}){ext}"));
        counter += 1;
        if counter > 999 {
            break;
        }
    }
    std::fs::write(&target, bytes).map_err(|error| format!("写入文件失败：{error}"))?;
    Ok(target.to_string_lossy().to_string())
}

/// 用户下载目录（Linux/macOS/Windows 都取 $HOME/Downloads）。
fn download_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join("Downloads")
}


/// 只重新解析并落库附件元数据（不动正文）。
///
/// 用于老邮件：正文在早期版本里解析过但没有附件记录，
/// 列表就永远是空的；这里强制再取一次原始邮件补齐。
pub async fn refresh_attachments(
    state: &AppState,
    message_id: i64,
) -> Result<Vec<Value>, String> {
    let locator = get_message_locator(state, message_id)?
        .ok_or_else(|| format!("邮件不存在：{message_id}"))?;
    let (raw_message, _) = mail_transport::fetch_raw_message(
        state,
        &locator.account,
        &locator.folder_path,
        locator.uid,
        locator.rfc822_message_id.as_deref(),
    )
    .await?;
    let parsed = parse_mail(&raw_message).map_err(|error| format!("解析邮件失败：{error}"))?;

    let mut leaves = Vec::new();
    collect_leaf_parts(&parsed, &mut leaves);
    let connection = db::open(state)?;
    connection
        .execute(
            "DELETE FROM onemail_message_attachments WHERE message_id=?1",
            [message_id],
        )
        .map_err(|error| format!("更新附件信息失败：{error}"))?;

    let mut result = Vec::new();
    let mut index = 0usize;
    for part in &leaves {
        let disposition = part.get_content_disposition();
        let filename = disposition
            .params
            .get("filename")
            .or_else(|| part.ctype.params.get("name"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(filename) = filename else { continue };
        if !matches!(
            disposition.disposition,
            DispositionType::Attachment | DispositionType::Inline
        ) {
            continue;
        }
        let size_bytes = part.get_body_raw().map(|value| value.len()).unwrap_or(0) as i64;
        connection
            .execute(
                "INSERT INTO onemail_message_attachments
                   (message_id,filename,mime_type,content_disposition,size_bytes)
                 VALUES (?1,?2,?3,?4,?5)",
                rusqlite::params![
                    message_id,
                    filename,
                    part.ctype.mimetype,
                    match disposition.disposition {
                        DispositionType::Inline => "inline",
                        _ => "attachment",
                    },
                    size_bytes
                ],
            )
            .map_err(|error| format!("保存附件信息失败：{error}"))?;
        result.push(json!({
            "index": index,
            "filename": filename,
            "mimeType": part.ctype.mimetype,
            "sizeBytes": size_bytes
        }));
        index += 1;
    }

    connection
        .execute(
            "UPDATE onemail_mail_messages SET has_attachments=?2 WHERE message_id=?1",
            rusqlite::params![message_id, i64::from(!result.is_empty())],
        )
        .map_err(|error| format!("更新附件标记失败：{error}"))?;
    Ok(result)
}

/// 读出附件字节（预览用）。仅用于小文件，调用方需自带大小上限。
pub async fn attachment_payload(
    state: &AppState,
    message_id: i64,
    index: usize,
) -> Result<Value, String> {
    let (filename, mime_type, bytes) = read_attachment(state, message_id, index).await?;
    Ok(json!({
        "filename": filename,
        "mimeType": mime_type,
        "sizeBytes": bytes.len(),
        "base64": base64_encode(&bytes)
    }))
}

/// 把附件写到调用方指定的路径（「下载 / 另存为」）。
pub async fn save_attachment_as(
    state: &AppState,
    message_id: i64,
    index: usize,
    target_path: &str,
) -> Result<String, String> {
    let (_, _, bytes) = read_attachment(state, message_id, index).await?;
    std::fs::write(target_path, bytes).map_err(|error| format!("写入文件失败：{error}"))?;
    Ok(target_path.to_string())
}

/// 取第 index 个附件的 (文件名, MIME, 字节)。
async fn read_attachment(
    state: &AppState,
    message_id: i64,
    index: usize,
) -> Result<(String, String, Vec<u8>), String> {
    let locator = get_message_locator(state, message_id)?
        .ok_or_else(|| format!("邮件不存在：{message_id}"))?;
    let (raw_message, _) = mail_transport::fetch_raw_message(
        state,
        &locator.account,
        &locator.folder_path,
        locator.uid,
        locator.rfc822_message_id.as_deref(),
    )
    .await?;
    let parsed = parse_mail(&raw_message).map_err(|error| format!("解析邮件失败：{error}"))?;
    let mut leaves = Vec::new();
    collect_leaf_parts(&parsed, &mut leaves);

    let mut attachments = Vec::new();
    for part in &leaves {
        let disposition = part.get_content_disposition();
        let filename = disposition
            .params
            .get("filename")
            .or_else(|| part.ctype.params.get("name"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(filename) = filename else { continue };
        if !matches!(
            disposition.disposition,
            DispositionType::Attachment | DispositionType::Inline
        ) {
            continue;
        }
        attachments.push((filename, part.ctype.mimetype.clone(), part));
    }

    let (filename, mime_type, part) = attachments
        .get(index)
        .ok_or_else(|| "找不到该附件，请重新打开邮件后再试。".to_string())?;
    let bytes = part
        .get_body_raw()
        .map_err(|error| format!("读取附件内容失败：{error}"))?;
    Ok((filename.clone(), mime_type.clone(), bytes.to_vec()))
}

/// 极简 base64 编码（避免为预览引入新依赖）。
fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 把指定序号的附件写到本地临时目录，返回文件路径。
///
/// 附件不在正文缓存里（只存文本/HTML），所以这里按需重新取原始邮件再解析。
pub async fn save_attachment(
    state: &AppState,
    message_id: i64,
    index: usize,
) -> Result<String, String> {
    let locator = get_message_locator(state, message_id)?
        .ok_or_else(|| format!("邮件不存在：{message_id}"))?;
    let (raw_message, _) = mail_transport::fetch_raw_message(
        state,
        &locator.account,
        &locator.folder_path,
        locator.uid,
        locator.rfc822_message_id.as_deref(),
    )
    .await?;
    let parsed = parse_mail(&raw_message).map_err(|error| format!("解析邮件失败：{error}"))?;
    let mut leaves = Vec::new();
    collect_leaf_parts(&parsed, &mut leaves);

    let mut attachments = Vec::new();
    for part in &leaves {
        let disposition = part.get_content_disposition();
        let filename = disposition
            .params
            .get("filename")
            .or_else(|| part.ctype.params.get("name"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let Some(filename) = filename else { continue };
        if !matches!(disposition.disposition, DispositionType::Attachment | DispositionType::Inline) {
            continue;
        }
        attachments.push((filename, part));
    }

    let (filename, part) = attachments
        .get(index)
        .ok_or_else(|| "找不到该附件，请重新打开邮件后再试。".to_string())?;
    let bytes = part
        .get_body_raw()
        .map_err(|error| format!("读取附件内容失败：{error}"))?;

    let safe_name = filename
        .chars()
        .map(|ch| if matches!(ch, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { ch })
        .collect::<String>();
    let dir = std::env::temp_dir().join("onemail-attachments").join(message_id.to_string());
    std::fs::create_dir_all(&dir).map_err(|error| format!("创建附件目录失败：{error}"))?;
    let path = dir.join(if safe_name.trim().is_empty() { "attachment".to_string() } else { safe_name });
    std::fs::write(&path, bytes).map_err(|error| format!("写入附件失败：{error}"))?;
    Ok(path.to_string_lossy().to_string())
}

pub async fn prefetch_recent_bodies(
    state: &AppState,
    account_id: Option<i64>,
) -> Result<(usize, usize), String> {
    let candidates = prefetch_candidates(state, account_id)?;
    let mut ok = 0usize;
    let mut failed = 0usize;
    for message_id in candidates {
        match load_message_body(state, message_id).await {
            Ok(_) => {
                ok += 1;
            }
            Err(_) => failed += 1,
        }
        // 给服务器留点喘息的间隔
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    }
    Ok((ok, failed))
}

/// 在后台跑一轮预取，不阻塞同步返回。
///
/// 同步是逐账号回调的，多个账号会重复触发；用 60 秒冷却做去重。
pub fn spawn_body_prefetch(app: &tauri::AppHandle, account_id: Option<i64>) {
    use std::sync::atomic::{AtomicI64, Ordering};
    static LAST_RUN: AtomicI64 = AtomicI64::new(0);
    let now = chrono::Utc::now().timestamp();
    let previous = LAST_RUN.load(Ordering::Relaxed);
    if now - previous < 60 {
        return;
    }
    if LAST_RUN
        .compare_exchange(previous, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        use tauri::Manager as _;
        let state = app.state::<crate::state::AppState>();
        match prefetch_recent_bodies(&state, account_id).await {
            Ok((ok, failed)) if ok > 0 || failed > 0 => {
                let _ = crate::client_log::write(
                    &app,
                    &format!("INFO Body prefetch ok={ok} failed={failed}"),
                );
            }
            Ok(_) => {}
            Err(error) => {
                let _ = crate::client_log::write(&app, &format!("WARN Body prefetch error {error}"));
            }
        }
    });
}

pub async fn load_message_body(state: &AppState, message_id: i64) -> Result<Value, String> {
    let locator = get_message_locator(state, message_id)?
        .ok_or_else(|| format!("邮件不存在：{message_id}"))?;
    // 已有可用正文直接返回；空行视为未缓存（并清掉，保证能重试）
    if let Some(body) = cached_body(state, message_id) {
        return Ok(body);
    }
    drop_empty_body(state, message_id);
    set_body_status(state, message_id, "loading", None)?;

    let result: Result<(), String> = async {
        // 先试原文件夹；不行再跨文件夹按 Message-ID 定位（邮件可能被服务端规则移走）
        let fetched = match mail_transport::fetch_raw_message(
            state,
            &locator.account,
            &locator.folder_path,
            locator.uid,
            locator.rfc822_message_id.as_deref(),
        )
        .await
        {
            Ok(value) => Ok(Some(value)),
            Err(error) => {
                if let Some(message_id) = locator.rfc822_message_id.as_deref() {
                    if let Some((folder, uid)) = relocate_message(state, &locator, message_id).await {
                        match mail_transport::fetch_raw_message(
                            state,
                            &locator.account,
                            &folder,
                            uid,
                            Some(message_id),
                        )
                        .await
                        {
                            Ok((raw, _)) => {
                                rebind_message_location(state, message_id, &folder, uid)?;
                                Ok(Some((raw, Some(uid))))
                            }
                            Err(folder_error) => Err(format!("{error}；在其他文件夹重试也失败：{folder_error}")),
                        }
                    } else {
                        Err(format!("{error}；按 Message-ID 在所有文件夹都未找到该邮件"))
                    }
                } else {
                    Err(error)
                }
            }
        }?;
        let (raw_message, resolved_uid) = fetched.expect("fetch guarded above");
        let parsed = parse_message(&raw_message)?;
        persist_message_body(state, message_id, locator.account.account_id, parsed)?;
        // UID 发生过漂移时把新 UID 写回，下次直接命中
        if let Some(uid) = resolved_uid {
            if uid != locator.uid {
                let connection = db::open(state)?;
                connection
                    .execute(
                        "UPDATE onemail_mail_messages SET uid=?2 WHERE message_id=?1",
                        rusqlite::params![message_id, i64::from(uid)],
                    )
                    .map_err(|error| format!("更新邮件 UID 失败：{error}"))?;
            }
        }
        Ok(())
    }
    .await;

    match result {
        Ok(()) => {
            let connection = db::open(state)?;
            let body = connection
                .query_row(
                    "SELECT body_text,body_html_sanitized,external_images_blocked
                     FROM onemail_message_bodies WHERE message_id=?1",
                    [message_id],
                    |row| {
                        Ok(json!({
                            "messageId": message_id,
                            "bodyText": row.get::<_, Option<String>>(0)?,
                            "bodyHtmlSanitized": row.get::<_, Option<String>>(1)?,
                            "externalImagesBlocked": row.get::<_, i64>(2)? != 0
                        }))
                    },
                )
                .optional()
                .map_err(|error| format!("读取邮件正文失败：{error}"))?;
            Ok(body.unwrap_or_else(|| json!(null)))
        }
        Err(error) => {
            let _ = set_body_status(state, message_id, "error", Some(&error));
            Err(error)
        }
    }
}

pub async fn repair_message_metadata(state: &AppState, message_id: i64) -> Result<(), String> {
    let locator = get_message_locator(state, message_id)?
        .ok_or_else(|| format!("邮件不存在：{message_id}"))?;
    let (raw_message, _resolved_uid) = mail_transport::fetch_raw_message(
        state,
        &locator.account,
        &locator.folder_path,
        locator.uid,
        locator.rfc822_message_id.as_deref(),
    )
    .await?;
    let parsed = parse_message(&raw_message)?;
    persist_message_body(state, message_id, locator.account.account_id, parsed).map(|_| ())
}

struct MessageLocator {
    account: MailAccount,
    folder_path: String,
    uid: u32,
    /// RFC822 Message-ID：UID 漂移时用它重新定位
    rfc822_message_id: Option<String>,
}

fn get_message_locator(
    state: &AppState,
    message_id: i64,
) -> Result<Option<MessageLocator>, String> {
    let connection = db::open(state)?;
    connection
        .query_row(
            "SELECT f.path,m.uid,m.account_id,m.rfc822_message_id
             FROM onemail_mail_messages m
             JOIN onemail_mail_folders f ON f.folder_id=m.folder_id
             WHERE m.message_id=?1",
            [message_id],
            |row| {
                let uid = row.get::<_, i64>(1)?;
                Ok((row.get::<_, String>(0)?, uid, row.get::<_, i64>(2)?, row.get::<_, Option<String>>(3)?))
            },
        )
        .optional()
        .map_err(|error| format!("读取邮件定位信息失败：{error}"))?
        .map(|(folder_path, uid, account_id, rfc822_message_id)| {
            let account = mail_transport::load_account(state, account_id)?;
            Ok(MessageLocator {
                account,
                folder_path,
                uid: u32::try_from(uid).map_err(|_| "邮件 UID 无效。".to_string())?,
                rfc822_message_id,
            })
        })
        .transpose()
}

fn set_body_status(
    state: &AppState,
    message_id: i64,
    status: &str,
    error: Option<&str>,
) -> Result<(), String> {
    let connection = db::open(state)?;
    connection
        .execute(
            "UPDATE onemail_mail_messages
             SET body_status=?2,body_error=?3,
                 updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE message_id=?1",
            params![message_id, status, error],
        )
        .map(|_| ())
        .map_err(|error| format!("更新正文状态失败：{error}"))
}

fn parse_message(raw_message: &[u8]) -> Result<ParsedMessageBody, String> {
    let parsed = parse_mail(raw_message).map_err(|error| format!("解析邮件正文失败：{error}"))?;
    let headers = parse_message_headers(&parsed.headers);
    let mut leaves = Vec::new();
    collect_leaf_parts(&parsed, &mut leaves);
    let text = leaves
        .iter()
        .filter(|part| is_message_body_part(part, "text/plain"))
        .find_map(|part| decode_text_part(part));
    let html = leaves
        .iter()
        .filter(|part| is_message_body_part(part, "text/html"))
        .find_map(|part| decode_text_part(part));
    let attachments = leaves
        .iter()
        .filter_map(|part| {
            let disposition = part.get_content_disposition();
            let disposition_name = match disposition.disposition {
                DispositionType::Attachment => "attachment",
                DispositionType::Inline => "inline",
                _ => return None,
            };
            let filename = disposition
                .params
                .get("filename")
                .or_else(|| part.ctype.params.get("name"))?
                .trim()
                .to_string();
            if filename.is_empty() {
                return None;
            }
            let size_bytes = part.get_body_raw().ok()?.len();
            Some(ParsedAttachment {
                filename,
                mime_type: part.ctype.mimetype.clone(),
                disposition: disposition_name.to_string(),
                size_bytes,
            })
        })
        .collect();

    Ok(ParsedMessageBody {
        raw_headers: parsed.headers.iter().map(|header| format!("{}: {}\r\n", header.get_key(), header.get_value())).collect(),
        headers,
        text,
        html,
        attachments,
    })
}

fn parse_message_headers(headers: &[MailHeader<'_>]) -> ParsedMessageHeaders {
    let (from_name, from_email) = first_header_address(headers, "From");

    ParsedMessageHeaders {
        subject: non_empty_header(headers, "Subject"),
        message_id: non_empty_header(headers, "Message-ID"),
        from_name,
        from_email,
        received_at: non_empty_header(headers, "Date"),
        in_reply_to: non_empty_header(headers, "In-Reply-To"),
        references_header: non_empty_header(headers, "References"),
        participants: parse_participant_headers(headers),
    }
}

fn non_empty_header(headers: &[MailHeader<'_>], name: &str) -> Option<String> {
    headers
        .get_first_value(name)
        .map(|value| value.replace('\0', "").trim().to_string())
        .filter(|value| !value.is_empty())
}

fn is_message_body_part(part: &ParsedMail<'_>, mime_type: &str) -> bool {
    if !part.ctype.mimetype.eq_ignore_ascii_case(mime_type) {
        return false;
    }
    let disposition = part.get_content_disposition();
    if matches!(disposition.disposition, DispositionType::Attachment) {
        return false;
    }
    !disposition.params.contains_key("filename") && !part.ctype.params.contains_key("name")
}

fn decode_text_part(part: &ParsedMail<'_>) -> Option<String> {
    part.get_body()
        .ok()
        .or_else(|| {
            part.get_body_raw()
                .ok()
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        })
        .map(|value| value.replace('\0', ""))
        .filter(|value| !value.trim().is_empty())
}

fn collect_leaf_parts<'a>(part: &'a ParsedMail<'a>, leaves: &mut Vec<&'a ParsedMail<'a>>) {
    if part.subparts.is_empty() {
        leaves.push(part);
        return;
    }
    for child in &part.subparts {
        collect_leaf_parts(child, leaves);
    }
}


/// 读取已缓存的正文；正文为空的记录视为「未缓存」，避免占位文案卡死。
fn cached_body(state: &AppState, message_id: i64) -> Option<Value> {
    let connection = db::open(state).ok()?;
    let value = connection
        .query_row(
            "SELECT body_text, body_html_sanitized, external_images_blocked
             FROM onemail_message_bodies WHERE message_id=?1",
            [message_id],
            |row| {
                Ok(json!({
                    "messageId": message_id,
                    "bodyText": row.get::<_, Option<String>>(0)?,
                    "bodyHtmlSanitized": row.get::<_, Option<String>>(1)?,
                    "externalImagesBlocked": row.get::<_, i64>(2).unwrap_or(0) != 0
                }))
            },
        )
        .ok()?;
    let has_text = value.get("bodyText").and_then(Value::as_str).is_some_and(|v| !v.trim().is_empty());
    let has_html = value.get("bodyHtmlSanitized").and_then(Value::as_str).is_some_and(|v| !v.trim().is_empty());
    if has_text || has_html {
        Some(value)
    } else {
        None
    }
}

/// 删掉空的正文行（历史上解析失败会写一行全 NULL，导致永远不再重试）。
fn drop_empty_body(state: &AppState, message_id: i64) {
    if let Ok(connection) = db::open(state) {
        let _ = connection.execute(
            "DELETE FROM onemail_message_bodies
             WHERE message_id=?1
               AND COALESCE(TRIM(body_text),'')=''
               AND COALESCE(TRIM(body_html_sanitized),'')=''",
            [message_id],
        );
    }
}

fn persist_message_body(
    state: &AppState,
    message_id: i64,
    account_id: i64,
    parsed: ParsedMessageBody,
) -> Result<Value, String> {
    let body_text = parsed
        .text
        .as_deref()
        .map(normalize_body_text)
        .filter(|value| !value.is_empty());
    let body_html = parsed.html.as_deref().and_then(sanitize_html);
    // 只有 HTML 的邮件（很多营销/机构邮件）没有 text/plain 副本：
    // 从净化后的 HTML 反推一份纯文本，保证搜索、复制与翻译都有内容可用。
    let body_text = body_text.or_else(|| body_html.as_deref().map(html_to_text));
    // 解析不出任何正文时不要落库：写进去会让后续重试被"已有记录"挡住
    if body_text.is_none() && body_html.is_none() {
        return Err("这封邮件没有可解析的正文（可能是纯附件、加密或服务器未返回内容）".to_string());
    }
    let search_text = body_text
        .as_deref()
        .map(str::to_string)
        .or_else(|| body_html.as_deref().map(html_to_text))
        .unwrap_or_default();
    let snippet = build_snippet(&search_text);
    let connection = db::open(state)?;
    connection
        .execute(
            "INSERT INTO onemail_message_bodies
               (message_id,body_text,body_html_sanitized,external_images_blocked,sanitized_at)
             VALUES (?1,?2,?3,1,strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(message_id) DO UPDATE SET
               body_text=excluded.body_text,body_html_sanitized=excluded.body_html_sanitized,
               external_images_blocked=excluded.external_images_blocked,
               sanitized_at=excluded.sanitized_at,loaded_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
               updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
            params![message_id, body_text, body_html],
        )
        .map_err(|error| format!("保存邮件正文失败：{error}"))?;
    connection
        .execute(
            "DELETE FROM onemail_message_attachments WHERE message_id=?1",
            [message_id],
        )
        .map_err(|error| format!("更新附件信息失败：{error}"))?;
    for attachment in &parsed.attachments {
        connection
            .execute(
                "INSERT INTO onemail_message_attachments
                   (message_id,filename,mime_type,content_disposition,size_bytes)
                 VALUES (?1,?2,?3,?4,?5)",
                params![
                    message_id,
                    attachment.filename,
                    attachment.mime_type,
                    attachment.disposition,
                    attachment.size_bytes as i64
                ],
            )
            .map_err(|error| format!("保存附件信息失败：{error}"))?;
    }
    connection
        .execute(
            "UPDATE onemail_mail_messages SET has_attachments=?2,body_status='ready',
               body_error=NULL,updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE message_id=?1",
            params![
                message_id,
                if parsed.attachments.is_empty() {
                    0_i64
                } else {
                    1_i64
                }
            ],
        )
        .map_err(|error| format!("更新正文状态失败：{error}"))?;

    persist_message_headers(&connection, message_id, &parsed.headers, snippet.as_deref())?;
    persist_participants(&connection, message_id, &parsed.headers.participants)?;
    connection.execute(
        "UPDATE onemail_mail_messages SET raw_headers=?2 WHERE message_id=?1",
        params![message_id, parsed.raw_headers],
    ).map_err(|error| format!("保存原始邮件头失败：{error}"))?;
    update_search_index(&connection, message_id, account_id, &search_text)?;

    Ok(json!({
        "messageId": message_id,
        "bodyText": body_text,
        "bodyHtmlSanitized": body_html,
        "externalImagesBlocked": true
    }))
}

fn persist_message_headers(
    connection: &rusqlite::Connection,
    message_id: i64,
    headers: &ParsedMessageHeaders,
    snippet: Option<&str>,
) -> Result<(), String> {
    connection
        .execute(
            "UPDATE onemail_mail_messages SET
               rfc822_message_id=COALESCE(NULLIF(TRIM(rfc822_message_id),''),?2),
               in_reply_to=COALESCE(NULLIF(TRIM(in_reply_to),''),?3),
               references_header=COALESCE(NULLIF(TRIM(references_header),''),?4),
               subject=COALESCE(NULLIF(TRIM(subject),''),?5),
               from_name=COALESCE(NULLIF(TRIM(from_name),''),?6),
               from_email=COALESCE(NULLIF(TRIM(from_email),''),?7),
               received_at=COALESCE(NULLIF(TRIM(received_at),''),?8),
               snippet=COALESCE(NULLIF(TRIM(snippet),''),?9),
               updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE message_id=?1",
            params![
                message_id,
                headers.message_id,
                headers.in_reply_to,
                headers.references_header,
                headers.subject,
                headers.from_name,
                headers.from_email,
                headers.received_at,
                snippet,
            ],
        )
        .map_err(|error| format!("回填邮件摘要失败：{error}"))?;
    Ok(())
}

fn build_snippet(value: &str) -> Option<String> {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let snippet = value.chars().take(240).collect::<String>();
    (!snippet.is_empty()).then_some(snippet)
}

fn update_search_index(
    connection: &rusqlite::Connection,
    message_id: i64,
    account_id: i64,
    body_text: &str,
) -> Result<(), String> {
    let metadata = connection
        .query_row(
            "SELECT folder_id,subject,from_name,from_email,snippet
             FROM onemail_mail_messages WHERE message_id=?1",
            [message_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                ))
            },
        )
        .map_err(|error| format!("读取搜索索引信息失败：{error}"))?;
    connection
        .execute(
            "DELETE FROM onemail_message_search WHERE message_id=?1",
            [message_id],
        )
        .map_err(|error| format!("更新搜索索引失败：{error}"))?;
    connection
        .execute(
            "INSERT INTO onemail_message_search
               (message_id,account_id,folder_id,subject,from_name,from_email,snippet,body_text)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                message_id, account_id, metadata.0, metadata.1, metadata.2, metadata.3, metadata.4,
                body_text
            ],
        )
        .map(|_| ())
        .map_err(|error| format!("更新搜索索引失败：{error}"))
}

#[cfg(test)]
mod tests;
