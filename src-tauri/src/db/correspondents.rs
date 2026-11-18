//! 往来联系人索引。
//!
//! 把「谁和我有过来往」从运行时聚合物化成三张表：
//!
//! - `onemail_correspondents`：一个来往地址一行（跨全部邮箱账号唯一）
//! - `onemail_correspondent_accounts`：联系人 × 我的邮箱账号（徽标、分账号未读）
//! - `onemail_correspondent_messages`：联系人 × 邮件（左栏最后一条、中栏时间线）
//!
//! 口径（与产品决策一致）：
//! - 联系人口径 = 单个归一化邮箱地址，不做域名合并
//! - 抄送也算往来：参与人取 `addresses.kind IN ('from','to','cc')`
//! - 只发未收也算往来：发件箱里 `status='sent'` 的 to/cc/bcc 都会建关系
//! - 垃圾邮件（`folders.role='junk'`）单独计数，且不参与普通组排序键 `last_at`
//! - 回收站与草稿不参与往来（`role IN ('trash','drafts')` 与 `is_draft=1` 排除）

use std::collections::{HashMap, HashSet};

use rusqlite::{params, params_from_iter, Connection, OptionalExtension, ToSql};
use serde_json::{json, Value};


/// 收到的邮件只按发件人算往来人。
///
/// 口径（用户裁定）：往来 = 「发给我的（含抄送）+ 我发出去的」。
/// 所以收件邮件只取 `from`；`to`/`cc` 只在我自己发出的邮件上展开
/// （见 `ingest_outbox`）。之前把收件邮件的共同收件人也算进来，
/// 会让「别人群发、我恰好在收件人里」的地址变成往来人。
const PARTICIPANT_KINDS: &str = "'from'";

pub const GROUP_NORMAL: &str = "normal";
pub const GROUP_JUNK: &str = "junk";

/// 内置服务商的默认短徽标（≤2 字）与颜色，新建账号时落库。
pub fn default_badge_for_provider(provider_key: &str) -> (String, String) {
    match provider_key {
        "gmail" => ("G".to_owned(), "#ea4335".to_owned()),
        "outlook" => ("O".to_owned(), "#0f6cbd".to_owned()),
        "qq" => ("QQ".to_owned(), "#12b7f5".to_owned()),
        "tencentEnterprise" => ("企".to_owned(), "#12b7f5".to_owned()),
        "163" | "netease163" => ("163".to_owned(), "#c0342b".to_owned()),
        "aliyun" => ("阿".to_owned(), "#ff6a00".to_owned()),
        "aliyunEnterprise" => ("阿里".to_owned(), "#ff6a00".to_owned()),
        "yahoo" => ("Y".to_owned(), "#7b61c9".to_owned()),
        "icloud" => ("i".to_owned(), "#0a749b".to_owned()),
        "189" => ("天".to_owned(), "#0a749b".to_owned()),
        "sina" => ("新".to_owned(), "#c0342b".to_owned()),
        "139" => ("移".to_owned(), "#227a4b".to_owned()),
        "sohu" => ("狐".to_owned(), "#227a4b".to_owned()),
        "yandex" => ("Ya".to_owned(), "#c0342b".to_owned()),
        "mailru" => ("M".to_owned(), "#0f6cbd".to_owned()),
        "aol" => ("A".to_owned(), "#0a749b".to_owned()),
        "perfect" => ("88".to_owned(), "#227a4b".to_owned()),
        "21cn" => ("21".to_owned(), "#7b61c9".to_owned()),
        _ => ("@".to_owned(), "#2d70d2".to_owned()),
    }
}

/// 全量重建：初次回填、或索引结构变化后调用。
pub fn rebuild_all(connection: &Connection) -> Result<(), String> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("开启联系人索引事务失败：{error}"))?;
    rebuild_links(&transaction, None)?;
    transaction
        .commit()
        .map_err(|error| format!("提交联系人索引失败：{error}"))
}

/// 增量重建单个账号：同步完成、发信成功、邮件增删后调用。
pub fn reindex_account(connection: &Connection, account_id: i64) -> Result<(), String> {
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| format!("开启联系人索引事务失败：{error}"))?;
    rebuild_links(&transaction, Some(account_id))?;
    transaction
        .commit()
        .map_err(|error| format!("提交联系人索引失败：{error}"))
}

/// 已读状态变化后刷新该邮件涉及的联系人未读计数。

/// 把某个联系人的全部往来邮件标记为已读（只改本地，不回写服务器 \Seen）。
///
/// 用 `read_state_override=1` 固定本地状态，避免下一次同步把 \Seen 状态覆盖回未读。
pub fn mark_all_read(connection: &Connection, correspondent_id: i64) -> Result<usize, String> {
    let changed = connection
        .execute(
            "UPDATE onemail_mail_messages SET is_read=1, read_state_override=1,
               updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE message_id IN (
               SELECT message_id FROM onemail_correspondent_messages
               WHERE correspondent_id=?1 AND source='mail' AND is_read=0
             )",
            [correspondent_id],
        )
        .map_err(|error| format!("标记已读失败：{error}"))?;
    connection
        .execute(
            "UPDATE onemail_correspondent_messages SET is_read=1
             WHERE correspondent_id=?1 AND source='mail' AND is_read=0",
            [correspondent_id],
        )
        .map_err(|error| format!("标记已读失败：{error}"))?;
    refresh_aggregates(connection)?;
    Ok(changed)
}

pub fn on_read_state_changed(connection: &Connection, message_id: i64) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT COALESCE(read_state_override, is_read)
             FROM onemail_mail_messages WHERE message_id=?1",
        )
        .map_err(|error| format!("读取邮件已读状态失败：{error}"))?;
    let is_read: Option<i64> = statement
        .query_row([message_id], |row| row.get(0))
        .optional()
        .map_err(|error| format!("读取邮件已读状态失败：{error}"))?;
    let Some(is_read) = is_read else {
        return Ok(());
    };
    connection
        .execute(
            "UPDATE onemail_correspondent_messages SET is_read=?2
             WHERE source='mail' AND message_id=?1",
            params![message_id, is_read],
        )
        .map_err(|error| format!("更新联系人已读状态失败：{error}"))?;
    refresh_aggregates(connection)
}

/// 左栏联系人列表。`group` 只影响归属、排序键与角标取数，不影响内容。
pub fn list(
    connection: &Connection,
    group: &str,
    keyword: Option<&str>,
    cursor: Option<&str>,
    limit: i64,
    account: Option<i64>,
) -> Result<Value, String> {
    let junk = group == GROUP_JUNK;
    let order_column = if junk { "junk_last_at" } else { "last_at" };
    let group_filter = if junk {
        "c.junk_last_at IS NOT NULL"
    } else {
        "c.last_at IS NOT NULL"
    };

    let mut sql = format!(
        "SELECT c.correspondent_id, c.address_norm, c.address_display, c.display_name, c.alias,
                c.last_at, c.junk_last_at, c.unread_count, c.junk_unread_count,
                c.inbound_count, c.outbound_count,
                (SELECT cm.subject FROM onemail_correspondent_messages cm
                  WHERE cm.correspondent_id=c.correspondent_id
                  ORDER BY cm.sent_at DESC LIMIT 1) AS last_subject,
                (SELECT cm.is_junk FROM onemail_correspondent_messages cm
                  WHERE cm.correspondent_id=c.correspondent_id
                  ORDER BY cm.sent_at DESC LIMIT 1) AS last_is_junk,
                (SELECT cm.has_attachment FROM onemail_correspondent_messages cm
                  WHERE cm.correspondent_id=c.correspondent_id
                  ORDER BY cm.sent_at DESC LIMIT 1) AS last_has_attachment
         FROM onemail_correspondents c
         WHERE {group_filter}"
    );

    let mut bindings: Vec<Box<dyn ToSql>> = Vec::new();
    if let Some(account_id) = account {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM onemail_correspondent_accounts ca
                          WHERE ca.correspondent_id=c.correspondent_id AND ca.account_id=?)",
        );
        bindings.push(Box::new(account_id));
    }
    if let Some(keyword) = keyword.map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = format!("%{keyword}%");
        sql.push_str(" AND (c.address_norm LIKE ? OR c.display_name LIKE ? OR c.alias LIKE ?)");
        bindings.push(Box::new(pattern.clone()));
        bindings.push(Box::new(pattern.clone()));
        bindings.push(Box::new(pattern));
    }
    if let Some(cursor) = cursor.and_then(parse_cursor) {
        sql.push_str(&format!(
            " AND (c.{order_column} < ? OR (c.{order_column} = ? AND c.correspondent_id < ?))"
        ));
        bindings.push(Box::new(cursor.0.clone()));
        bindings.push(Box::new(cursor.0));
        bindings.push(Box::new(cursor.1));
    }
    sql.push_str(&format!(
        " ORDER BY c.{order_column} DESC, c.correspondent_id DESC LIMIT ?"
    ));
    bindings.push(Box::new(limit.clamp(1, 200)));

    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("读取联系人列表失败：{error}"))?;
    let rows = statement
        .query_map(params_from_iter(bindings.iter().map(|value| value.as_ref())), |row| {
            Ok(CorrespondentRow {
                correspondent_id: row.get(0)?,
                address_norm: row.get(1)?,
                address_display: row.get(2)?,
                display_name: row.get(3)?,
                alias: row.get(4)?,
                last_at: row.get(5)?,
                junk_last_at: row.get(6)?,
                unread_count: row.get(7)?,
                junk_unread_count: row.get(8)?,
                inbound_count: row.get(9)?,
                outbound_count: row.get(10)?,
                last_subject: row.get(11)?,
                last_is_junk: row.get::<_, Option<i64>>(12)?.unwrap_or(0) != 0,
                last_has_attachment: row.get::<_, Option<i64>>(13)?.unwrap_or(0) != 0,
            })
        })
        .map_err(|error| format!("读取联系人列表失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取联系人列表失败：{error}"))?;

    let badges = load_badges(connection, &rows)?;
    let accounts = load_account_meta(connection)?;
    let items = rows
        .into_iter()
        .map(|row| {
            let mut account_badges = badges
                .get(&row.correspondent_id)
                .cloned()
                .unwrap_or_default();
            account_badges.sort_by(|left, right| right.last_at.cmp(&left.last_at));
            let badge_total = account_badges.len();
            let mut value = row.into_value();
            value["badgeTotal"] = json!(badge_total);
            value["accounts"] = Value::Array(
                account_badges
                    .into_iter()
                    .map(|badge| {
                        let meta = accounts.get(&badge.account_id);
                        json!({
                            "accountId": badge.account_id,
                            "label": meta.map(|m| m.0.clone()).unwrap_or_default(),
                            "email": meta.map(|m| m.1.clone()).unwrap_or_default(),
                            "badge": meta.map(|m| m.2.clone()).unwrap_or_else(|| "@".to_owned()),
                            "color": meta.and_then(|m| m.3.clone()),
                            "lastAt": badge.last_at,
                            "unreadCount": badge.unread_count,
                            "junkUnreadCount": badge.junk_unread_count
                        })
                    })
                    .collect(),
            );
            value
        })
        .collect::<Vec<_>>();

    let next_cursor = items.last().and_then(|item| {
        let at = if junk {
            item.get("junkLastAt")
        } else {
            item.get("lastAt")
        }?;
        let at = at.as_str()?;
        let id = item.get("correspondentId")?.as_i64()?;
        Some(format!("{at}|{id}"))
    });

    Ok(json!({ "group": group, "items": items, "nextCursor": next_cursor }))
}

/// 中栏往来时间线：与分��无关，始终返回该联系人的全部往来。
/// 按时间倒序（新→旧），配合 `before` 游标向上翻页；前端渲染时反转为正序。
pub fn thread(
    connection: &Connection,
    correspondent_id: i64,
    before: Option<&str>,
    limit: i64,
) -> Result<Value, String> {
    let meta = connection
        .query_row(
            "SELECT correspondent_id, address_norm, address_display, display_name, alias,
                    last_at, junk_last_at, unread_count, junk_unread_count,
                    inbound_count, outbound_count
             FROM onemail_correspondents WHERE correspondent_id=?1",
            [correspondent_id],
            |row| {
                Ok(CorrespondentRow {
                    correspondent_id: row.get(0)?,
                    address_norm: row.get(1)?,
                    address_display: row.get(2)?,
                    display_name: row.get(3)?,
                    alias: row.get(4)?,
                    last_at: row.get(5)?,
                    junk_last_at: row.get(6)?,
                    unread_count: row.get(7)?,
                    junk_unread_count: row.get(8)?,
                    inbound_count: row.get(9)?,
                    outbound_count: row.get(10)?,
                    last_subject: None,
                    last_is_junk: false,
                    last_has_attachment: false,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取联系人失败：{error}"))?;
    let Some(meta) = meta else {
        return Err(format!("联系人不存在：{correspondent_id}"));
    };

    let limit = limit.clamp(1, 500);
    let mut statement = connection
        .prepare(
            "SELECT source, message_id, account_id, direction, sent_at, subject,
                    is_read, has_attachment, is_junk
             FROM onemail_correspondent_messages
             WHERE correspondent_id=?1 AND (?2 IS NULL OR sent_at < ?2)
             ORDER BY sent_at DESC, message_id DESC
             LIMIT ?3",
        )
        .map_err(|error| format!("读取往来记录失败：{error}"))?;
    let messages = statement
        .query_map(params![correspondent_id, before, limit], |row| {
            Ok(json!({
                "source": row.get::<_, String>(0)?,
                "messageId": row.get::<_, i64>(1)?,
                "accountId": row.get::<_, i64>(2)?,
                "direction": row.get::<_, String>(3)?,
                "sentAt": row.get::<_, String>(4)?,
                "subject": row.get::<_, Option<String>>(5)?,
                "isRead": row.get::<_, i64>(6)? != 0,
                "hasAttachment": row.get::<_, i64>(7)? != 0,
                "isJunk": row.get::<_, i64>(8)? != 0
            }))
        })
        .map_err(|error| format!("读取往来记录失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取往来记录失败：{error}"))?;

    let next_cursor = messages
        .last()
        .and_then(|message| message.get("sentAt"))
        .and_then(Value::as_str)
        .map(str::to_owned);

    let badges = load_badges(connection, std::slice::from_ref(&meta))?;
    let accounts = load_account_meta(connection)?;
    let mut meta_value = meta.into_value();
    meta_value["accounts"] = Value::Array(
        badges
            .get(&correspondent_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|badge| {
                let account = accounts.get(&badge.account_id);
                json!({
                    "accountId": badge.account_id,
                    "label": account.map(|m| m.0.clone()).unwrap_or_default(),
                    "badge": account.map(|m| m.2.clone()).unwrap_or_else(|| "@".to_owned()),
                    "color": account.and_then(|m| m.3.clone()),
                    "unreadCount": badge.unread_count,
                    "junkUnreadCount": badge.junk_unread_count
                })
            })
            .collect(),
    );

    Ok(json!({
        "correspondent": meta_value,
        "messages": messages,
        "nextCursor": next_cursor
    }))
}

/// 左栏两个分组 tab 的角标：普通组取普通未读，垃圾组取垃圾未读，两者不相加。
pub fn group_counts(connection: &Connection) -> Result<Value, String> {
    let (normal_contacts, normal_unread, junk_contacts, junk_unread) = connection
        .query_row(
            "SELECT
               COALESCE(SUM(CASE WHEN last_at IS NOT NULL THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN last_at IS NOT NULL THEN unread_count ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN junk_last_at IS NOT NULL THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN junk_last_at IS NOT NULL THEN junk_unread_count ELSE 0 END), 0)
             FROM onemail_correspondents",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .map_err(|error| format!("读取联系人分组统计失败：{error}"))?;

    Ok(json!({
        "normal": { "contacts": normal_contacts, "unread": normal_unread },
        "junk": { "contacts": junk_contacts, "unread": junk_unread }
    }))
}

// ------------------------------------------------------------------ 内部实现

struct CorrespondentRow {
    correspondent_id: i64,
    address_norm: String,
    address_display: String,
    display_name: Option<String>,
    alias: Option<String>,
    last_at: Option<String>,
    junk_last_at: Option<String>,
    unread_count: i64,
    junk_unread_count: i64,
    inbound_count: i64,
    outbound_count: i64,
    last_subject: Option<String>,
    last_is_junk: bool,
    last_has_attachment: bool,
}

impl CorrespondentRow {
    fn into_value(self) -> Value {
        json!({
            "correspondentId": self.correspondent_id,
            "address": self.address_norm,
            "addressDisplay": self.address_display,
            "displayName": self.display_name,
            "alias": self.alias,
            "lastAt": self.last_at,
            "junkLastAt": self.junk_last_at,
            "unreadCount": self.unread_count,
            "junkUnreadCount": self.junk_unread_count,
            "inboundCount": self.inbound_count,
            "outboundCount": self.outbound_count,
            "lastSubject": self.last_subject,
            "lastIsJunk": self.last_is_junk,
            "lastHasAttachment": self.last_has_attachment
        })
    }
}

#[derive(Clone)]
struct BadgeRow {
    account_id: i64,
    last_at: String,
    unread_count: i64,
    junk_unread_count: i64,
}

/// 设置/清除联系人备注名（本地生效，不写回邮件服务器）。
pub fn set_alias(connection: &Connection, correspondent_id: i64, alias: Option<&str>) -> Result<(), String> {
    let normalized = alias
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(64).collect::<String>());
    let changed = connection
        .execute(
            "UPDATE onemail_correspondents SET alias=?2,
               updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE correspondent_id=?1",
            rusqlite::params![correspondent_id, normalized],
        )
        .map_err(|error| format!("保存备注名失败：{error}"))?;
    if changed == 0 {
        return Err("联系人不存在。".to_string());
    }
    Ok(())
}

type AccountMeta = (String, String, String, Option<String>);

fn parse_cursor(cursor: &str) -> Option<(String, i64)> {
    let (at, id) = cursor.rsplit_once('|')?;
    Some((at.to_owned(), id.parse::<i64>().ok()?))
}

fn load_account_meta(connection: &Connection) -> Result<HashMap<i64, AccountMeta>, String> {
    let mut statement = connection
        .prepare(
            "SELECT account_id, account_label, email,
                    COALESCE(NULLIF(badge_label,''), NULLIF(avatar_text,''), UPPER(SUBSTR(email,1,2)))
             , badge_color
             FROM onemail_mail_accounts",
        )
        .map_err(|error| format!("读取账号徽标失败：{error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                (
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ),
            ))
        })
        .map_err(|error| format!("读取账号徽标失败：{error}"))?
        .collect::<Result<HashMap<_, _>, _>>()
        .map_err(|error| format!("读取账号徽标失败：{error}"))?;
    Ok(rows)
}

fn load_badges(
    connection: &Connection,
    rows: &[CorrespondentRow],
) -> Result<HashMap<i64, Vec<BadgeRow>>, String> {
    let mut result: HashMap<i64, Vec<BadgeRow>> = HashMap::new();
    if rows.is_empty() {
        return Ok(result);
    }
    let placeholders = std::iter::repeat("?")
        .take(rows.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT correspondent_id, account_id, COALESCE(last_at,''), unread_count, junk_unread_count
         FROM onemail_correspondent_accounts
         WHERE correspondent_id IN ({placeholders})"
    );
    let ids = rows.iter().map(|row| row.correspondent_id).collect::<Vec<_>>();
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("读取联系人徽标失败：{error}"))?;
    let mut query = statement
        .query(params_from_iter(ids.iter()))
        .map_err(|error| format!("读取联系人徽标失败：{error}"))?;
    while let Some(row) = query
        .next()
        .map_err(|error| format!("读取联系人徽标失败：{error}"))?
    {
        let correspondent_id: i64 = row.get(0).map_err(|error| error.to_string())?;
        result.entry(correspondent_id).or_default().push(BadgeRow {
            account_id: row.get(1).map_err(|error| error.to_string())?,
            last_at: row.get(2).map_err(|error| error.to_string())?,
            unread_count: row.get(3).map_err(|error| error.to_string())?,
            junk_unread_count: row.get(4).map_err(|error| error.to_string())?,
        });
    }
    Ok(result)
}

fn self_addresses(connection: &Connection) -> Result<HashSet<String>, String> {
    let mut statement = connection
        .prepare("SELECT normalized_email FROM onemail_mail_accounts")
        .map_err(|error| format!("读取自有地址失败：{error}"))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| format!("读取自有地址失败：{error}"))?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|error| format!("读取自有地址失败：{error}"))?;
    Ok(rows)
}

fn rebuild_links(connection: &Connection, account: Option<i64>) -> Result<(), String> {
    // 全量重建会删掉联系人行，但备注名（alias）是用户手写的数据，必须先备份再回填。
    let aliases = if account.is_none() {
        load_aliases(connection)?
    } else {
        Vec::new()
    };

    match account {
        Some(account_id) => {
            connection
                .execute(
                    "DELETE FROM onemail_correspondent_messages WHERE account_id=?1",
                    [account_id],
                )
                .map_err(|error| format!("清理联系人索引失败：{error}"))?;
        }
        None => {
            connection
                .execute_batch(
                    "DELETE FROM onemail_correspondent_messages;
                     DELETE FROM onemail_correspondent_accounts;
                     DELETE FROM onemail_correspondents;",
                )
                .map_err(|error| format!("清理联系人索引失败：{error}"))?;
        }
    }

    let self_addresses = self_addresses(connection)?;
    ingest_mail(connection, account, &self_addresses)?;
    ingest_outbox(connection, account, &self_addresses)?;

    connection
        .execute(
            "DELETE FROM onemail_correspondents
             WHERE correspondent_id NOT IN (
               SELECT correspondent_id FROM onemail_correspondent_messages
             )",
            [],
        )
        .map_err(|error| format!("清理空联系人失败：{error}"))?;

    if !aliases.is_empty() {
        let mut statement = connection
            .prepare("UPDATE onemail_correspondents SET alias=?2 WHERE address_norm=?1")
            .map_err(|error| format!("恢复备注名失败：{error}"))?;
        for (address_norm, alias) in &aliases {
            let _ = statement.execute(rusqlite::params![address_norm, alias]);
        }
    }

    refresh_aggregates(connection)
}

/// 读出全部非空备注名，用于全量重建后的回填。
fn load_aliases(connection: &Connection) -> Result<Vec<(String, String)>, String> {
    let mut statement = connection
        .prepare("SELECT address_norm, alias FROM onemail_correspondents WHERE alias IS NOT NULL AND alias<>''")
        .map_err(|error| format!("读取备注名失败：{error}"))?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
        .map_err(|error| format!("读取备注名失败：{error}"))?;
    Ok(rows.filter_map(Result::ok).collect())
}

struct Participant {
    address_norm: String,
    address_display: String,
    name: Option<String>,
}

struct MessageEntry {
    message_id: i64,
    account_id: i64,
    direction: &'static str,
    sent_at: String,
    subject: Option<String>,
    has_attachment: bool,
    is_read: bool,
    is_junk: bool,
    participants: Vec<Participant>,
}

fn ingest_mail(
    connection: &Connection,
    account: Option<i64>,
    self_addresses: &HashSet<String>,
) -> Result<(), String> {
    let sql = format!(
        "SELECT m.message_id, m.account_id, m.subject, m.has_attachments,
                COALESCE(m.read_state_override, m.is_read),
                f.role, m.junk_override,
                -- 左栏按「最近一封邮件的到达时间」倒序：优先服务器接收时间 received_at，
                -- 缺失时才回退到发件人声明时间/内部日期
                COALESCE(NULLIF(m.received_at,''), NULLIF(m.sent_at,''),
                         NULLIF(m.internal_date,''), m.created_at),
                a.normalized_email, a.email, a.name
         FROM onemail_mail_messages m
         JOIN onemail_mail_folders f ON f.folder_id = m.folder_id
         JOIN onemail_message_addresses a ON a.message_id = m.message_id
         WHERE m.is_draft = 0 AND m.user_deleted = 0 AND m.is_deleted = 0
           AND m.remote_deleted = 0
           AND f.role NOT IN ('trash','drafts')
           AND (
             -- 收到的邮件：往来人 = 发件人
             (f.role <> 'sent' AND a.kind = 'from')
             OR
             -- 自己发出的邮件（Sent 文件夹）：往来人 = 收件人/抄送/密送
             (f.role = 'sent' AND a.kind IN ('to','cc','bcc'))
           )
           AND (?1 IS NULL OR m.account_id = ?1)
         ORDER BY m.message_id"
    );

    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| format!("读取往来邮件失败：{error}"))?;
    let mut rows = statement
        .query(params![account])
        .map_err(|error| format!("读取往来邮件失败：{error}"))?;

    let mut entries: Vec<MessageEntry> = Vec::new();
    while let Some(row) = rows.next().map_err(|error| format!("读取往来邮件失败：{error}"))? {
        let message_id: i64 = row.get(0).map_err(|error| error.to_string())?;
        let account_id: i64 = row.get(1).map_err(|error| error.to_string())?;
        let subject: Option<String> = row.get(2).map_err(|error| error.to_string())?;
        let has_attachment: i64 = row.get(3).map_err(|error| error.to_string())?;
        let is_read: i64 = row.get(4).map_err(|error| error.to_string())?;
        let role: String = row.get(5).map_err(|error| error.to_string())?;
        let junk_override: Option<i64> = row.get(6).map_err(|error| error.to_string())?;
        let sent_at: String = row.get(7).map_err(|error| error.to_string())?;
        let address_norm: String = row.get(8).map_err(|error| error.to_string())?;
        let address_display: String = row.get(9).map_err(|error| error.to_string())?;
        let name: Option<String> = row.get(10).map_err(|error| error.to_string())?;

        let direction = if role == "sent" { "out" } else { "in" };
        // 单封覆盖优先于文件夹角色：NULL=跟随文件夹，1=强制垃圾，0=强制非垃圾
        let is_junk = match junk_override {
            Some(value) => value == 1,
            None => role == "junk",
        };
        let participant = Participant {
            address_norm,
            address_display,
            name,
        };

        match entries.last_mut() {
            Some(entry) if entry.message_id == message_id => {
                entry.participants.push(participant);
            }
            _ => entries.push(MessageEntry {
                message_id,
                account_id,
                direction,
                sent_at,
                subject,
                has_attachment: has_attachment != 0,
                is_read: is_read != 0,
                is_junk,
                participants: vec![participant],
            }),
        }
    }

    for entry in &entries {
        for participant in &entry.participants {
            if self_addresses.contains(&participant.address_norm) {
                continue;
            }
            let correspondent_id = upsert_correspondent(connection, participant, &entry.sent_at)?;
            connection
                .execute(
                    "INSERT OR REPLACE INTO onemail_correspondent_messages
                     (correspondent_id, message_id, source, account_id, direction,
                      sent_at, subject, has_attachment, is_read, is_junk)
                     VALUES (?1, ?2, 'mail', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        correspondent_id,
                        entry.message_id,
                        entry.account_id,
                        entry.direction,
                        entry.sent_at,
                        entry.subject,
                        entry.has_attachment as i64,
                        entry.is_read as i64,
                        entry.is_junk as i64
                    ],
                )
                .map_err(|error| format!("写入联系人往来记录失败：{error}"))?;
        }
    }
    Ok(())
}

fn ingest_outbox(
    connection: &Connection,
    account: Option<i64>,
    self_addresses: &HashSet<String>,
) -> Result<(), String> {
    // 已被 sent 文件夹同步到的发件记录会被 mail_messages 覆盖，这里跳过以免重复计数。
    let mut statement = connection
        .prepare(
            "SELECT o.outbox_id, o.account_id, o.subject, o.to_json, o.cc_json, o.bcc_json,
                    COALESCE(NULLIF(o.sent_at,''), o.created_at),
                    EXISTS(SELECT 1 FROM onemail_outbox_attachments oa WHERE oa.outbox_id=o.outbox_id)
             FROM onemail_outbox_messages o
             WHERE o.status='sent' AND o.deleted_at IS NULL
               AND (?1 IS NULL OR o.account_id=?1)
               AND NOT EXISTS (
                 SELECT 1 FROM onemail_mail_messages m
                 WHERE m.account_id=o.account_id
                   AND m.rfc822_message_id=o.rfc822_message_id
               )
             ORDER BY o.outbox_id",
        )
        .map_err(|error| format!("读取发件记录失败：{error}"))?;
    let rows = statement
        .query_map(params![account], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })
        .map_err(|error| format!("读取发件记录失败：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("读取发件记录失败：{error}"))?;

    for (outbox_id, account_id, subject, to_json, cc_json, bcc_json, sent_at, has_attachment) in rows {
        let mut recipients: Vec<Participant> = Vec::new();
        for raw in [to_json, cc_json, bcc_json] {
            let parsed: Value = serde_json::from_str(&raw).unwrap_or_else(|_| json!([]));
            let Some(items) = parsed.as_array() else {
                continue;
            };
            for item in items {
                let email = item
                    .get("email")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("address").and_then(Value::as_str))
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let Some(email) = email else { continue };
                let normalized = normalized_address(email);
                if self_addresses.contains(&normalized) {
                    continue;
                }
                recipients.push(Participant {
                    address_norm: normalized,
                    address_display: email.to_owned(),
                    name: item
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_owned),
                });
            }
        }

        for participant in recipients {
            let correspondent_id = upsert_correspondent(connection, &participant, &sent_at)?;
            connection
                .execute(
                    "INSERT OR REPLACE INTO onemail_correspondent_messages
                     (correspondent_id, message_id, source, account_id, direction,
                      sent_at, subject, has_attachment, is_read, is_junk)
                     VALUES (?1, ?2, 'outbox', ?3, 'out', ?4, ?5, ?6, 1, 0)",
                    params![
                        correspondent_id,
                        outbox_id,
                        account_id,
                        sent_at,
                        subject,
                        has_attachment
                    ],
                )
                .map_err(|error| format!("写入发件往来记录失败：{error}"))?;
        }
    }
    Ok(())
}

fn upsert_correspondent(
    connection: &Connection,
    participant: &Participant,
    at: &str,
) -> Result<i64, String> {
    let domain = participant
        .address_norm
        .split_once('@')
        .map(|(_, domain)| domain.to_owned());
    connection
        .execute(
            "INSERT INTO onemail_correspondents
             (address_norm, address_display, display_name, domain, first_at, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5,
                     strftime('%Y-%m-%dT%H:%M:%fZ','now'), strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(address_norm) DO UPDATE SET
               address_display = excluded.address_display,
               display_name = COALESCE(excluded.display_name, onemail_correspondents.display_name),
               first_at = MIN(onemail_correspondents.first_at, excluded.first_at),
               updated_at = excluded.updated_at",
            params![
                participant.address_norm,
                participant.address_display,
                participant.name,
                domain,
                at
            ],
        )
        .map_err(|error| format!("写入联系人失败：{error}"))?;
    connection
        .query_row(
            "SELECT correspondent_id FROM onemail_correspondents WHERE address_norm=?1",
            [&participant.address_norm],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| format!("读取联系人标识失败：{error}"))
}

fn refresh_aggregates(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "UPDATE onemail_correspondents SET
               inbound_count = (SELECT COUNT(*) FROM onemail_correspondent_messages cm
                                 WHERE cm.correspondent_id = onemail_correspondents.correspondent_id
                                   AND cm.direction='in'),
               outbound_count = (SELECT COUNT(*) FROM onemail_correspondent_messages cm
                                 WHERE cm.correspondent_id = onemail_correspondents.correspondent_id
                                   AND cm.direction='out'),
               unread_count = (SELECT COUNT(*) FROM onemail_correspondent_messages cm
                                 WHERE cm.correspondent_id = onemail_correspondents.correspondent_id
                                   AND cm.is_read=0 AND cm.is_junk=0),
               junk_unread_count = (SELECT COUNT(*) FROM onemail_correspondent_messages cm
                                 WHERE cm.correspondent_id = onemail_correspondents.correspondent_id
                                   AND cm.is_read=0 AND cm.is_junk=1),
               last_at = (SELECT MAX(cm.sent_at) FROM onemail_correspondent_messages cm
                                 WHERE cm.correspondent_id = onemail_correspondents.correspondent_id
                                   AND cm.is_junk=0),
               junk_last_at = (SELECT MAX(cm.sent_at) FROM onemail_correspondent_messages cm
                                 WHERE cm.correspondent_id = onemail_correspondents.correspondent_id
                                   AND cm.is_junk=1),
               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now');

             DELETE FROM onemail_correspondent_accounts;

             INSERT INTO onemail_correspondent_accounts
               (correspondent_id, account_id, last_at, inbound_count, outbound_count,
                unread_count, junk_unread_count)
             SELECT cm.correspondent_id, cm.account_id, MAX(cm.sent_at),
                    SUM(CASE WHEN cm.direction='in' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN cm.direction='out' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN cm.is_read=0 AND cm.is_junk=0 THEN 1 ELSE 0 END),
                    SUM(CASE WHEN cm.is_read=0 AND cm.is_junk=1 THEN 1 ELSE 0 END)
             FROM onemail_correspondent_messages cm
             GROUP BY cm.correspondent_id, cm.account_id;",
        )
        .map_err(|error| format!("刷新联系人统计失败：{error}"))
}

fn normalized_address(email: &str) -> String {
    email.trim().to_lowercase()
}

/// 首次建库时把索引版本写入 `PRAGMA user_version`。
pub fn schema_version(connection: &Connection) -> Result<i64, String> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
        .map_err(|error| format!("读取数据库版本失败：{error}"))
}

pub fn set_schema_version(connection: &Connection, version: i64) -> Result<(), String> {
    connection
        .execute_batch(&format!("PRAGMA user_version = {version}"))
        .map_err(|error| format!("写入数据库版本失败：{error}"))
}

#[cfg(test)]
mod tests;
