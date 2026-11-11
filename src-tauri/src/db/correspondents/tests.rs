//! 往来联系人索引的行为测试：抄送建关系、只发也建关系、垃圾单独计数、徽标聚合、已读联动。

use rusqlite::{params, Connection};
use serde_json::Value;

use super::{
    group_counts, list, on_read_state_changed, rebuild_all, thread, GROUP_JUNK, GROUP_NORMAL,
};

const SEED: &str = "
  INSERT INTO onemail_provider_presets (provider_key,display_name,auth_type)
  VALUES ('gmail','Gmail','password'),('qq','QQ','password');

  INSERT INTO onemail_mail_accounts
    (account_id,provider_key,email,normalized_email,account_label,auth_type,
     imap_host,imap_port,imap_security,badge_label,badge_color)
  VALUES
    (1,'gmail','me@a.test','me@a.test','主邮箱','password','imap.a.test',993,'ssl_tls','G','#ea4335'),
    (2,'qq','me@b.test','me@b.test','备用','password','imap.b.test',993,'ssl_tls','QQ','#12b7f5');

  INSERT INTO onemail_mail_folders (folder_id,account_id,path,name,role,sync_enabled)
  VALUES
    (1,1,'INBOX','收件箱','inbox',1),
    (2,1,'Junk','垃圾邮件','junk',1),
    (3,1,'Sent','已发送','sent',1),
    (4,1,'Trash','已删除','trash',1),
    (5,2,'INBOX','收件箱','inbox',1);
";

fn setup() -> Connection {
    let connection = Connection::open_in_memory().expect("open in-memory database");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("enable foreign keys");
    connection
        .execute_batch(crate::db::SCHEMA_SQL)
        .expect("initialize schema");
    connection.execute_batch(SEED).expect("seed accounts");
    connection
}

fn insert_message(
    connection: &Connection,
    account_id: i64,
    folder_id: i64,
    uid: i64,
    subject: &str,
    sent_at: &str,
    is_read: i64,
) -> i64 {
    connection
        .execute(
            "INSERT INTO onemail_mail_messages
             (account_id,folder_id,uid,subject,sent_at,is_read,has_attachments)
             VALUES (?1,?2,?3,?4,?5,?6,0)",
            params![account_id, folder_id, uid, subject, sent_at, is_read],
        )
        .expect("insert message");
    connection.last_insert_rowid()
}

fn add_address(connection: &Connection, message_id: i64, kind: &str, email: &str, name: &str) {
    connection
        .execute(
            "INSERT INTO onemail_message_addresses (message_id,kind,name,email,normalized_email)
             VALUES (?1,?2,?3,?4,lower(?4))",
            params![message_id, kind, name, email],
        )
        .expect("insert address");
}

fn insert_sent_outbox(connection: &Connection, outbox_id: i64, to_json: &str) {
    connection
        .execute(
            "INSERT INTO onemail_outbox_messages
             (outbox_id,account_id,compose_kind,status,rfc822_message_id,from_email,to_json,sent_at)
             VALUES (?1,1,'new','sent',?2,'me@a.test',?3,'2026-11-02T10:00:00.000Z')",
            params![outbox_id, format!("mid-{outbox_id}@a.test"), to_json],
        )
        .expect("insert outbox");
}

fn find(items: &Value, address: &str) -> Value {
    // 允许传入整个返回体或直接的 items 数组
    let items = items.get("items").unwrap_or(items);
    items
        .as_array()
        .expect("items array")
        .iter()
        .find(|item| item["address"] == Value::String(address.to_owned()))
        .unwrap_or_else(|| panic!("correspondent {address} not found in {}", items))
        .clone()
}

#[test]
fn cc_recipients_create_correspondence_and_self_is_excluded() {
    let connection = setup();
    let message_id = insert_message(&connection, 1, 1, 1, "项目排期", "2026-11-01T09:00:00.000Z", 1);
    add_address(&connection, message_id, "from", "alice@corp.test", "Alice");
    add_address(&connection, message_id, "to", "me@a.test", "我");
    add_address(&connection, message_id, "cc", "bob@corp.test", "Bob");

    rebuild_all(&connection).expect("rebuild index");
    let items = list(&connection, GROUP_NORMAL, None, None, 50, None).expect("list normal");

    let alice = find(&items, "alice@corp.test");
    assert_eq!(alice["displayName"], "Alice");
    assert_eq!(alice["inboundCount"], 1);
    assert_eq!(alice["unreadCount"], 0);

    // 抄送也算往来
    let bob = find(&items, "bob@corp.test");
    assert_eq!(bob["inboundCount"], 1);

    // 自有地址不进联系人列表
    assert!(!items["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["address"] == "me@a.test"));
}

#[test]
fn outbound_only_contact_is_indexed_from_outbox() {
    let connection = setup();
    insert_sent_outbox(&connection, 1, r#"[{"name":"Carol","email":"carol@shop.test"}]"#);

    rebuild_all(&connection).expect("rebuild index");
    let items = list(&connection, GROUP_NORMAL, None, None, 50, None).expect("list normal");
    let carol = find(&items, "carol@shop.test");
    assert_eq!(carol["outboundCount"], 1);
    assert_eq!(carol["inboundCount"], 0);
    assert_eq!(carol["lastSubject"], Value::Null);
    assert!(carol["lastAt"].as_str().is_some(), "只发也应产生排序时间");
}

#[test]
fn junk_is_counted_separately_and_never_drives_normal_order() {
    let connection = setup();
    let normal = insert_message(&connection, 1, 1, 1, "正常往来", "2026-11-01T09:00:00.000Z", 1);
    add_address(&connection, normal, "from", "bob@corp.test", "Bob");
    add_address(&connection, normal, "to", "me@a.test", "我");

    let junk = insert_message(&connection, 1, 2, 2, "中奖通知", "2026-11-09T09:00:00.000Z", 0);
    add_address(&connection, junk, "from", "bob@corp.test", "Bob");
    add_address(&connection, junk, "to", "me@a.test", "我");

    rebuild_all(&connection).expect("rebuild index");

    let normal_items = list(&connection, GROUP_NORMAL, None, None, 50, None).expect("list normal");
    let bob = find(&normal_items, "bob@corp.test");
    assert_eq!(bob["lastAt"], "2026-11-01T09:00:00.000Z", "垃圾邮件不得顶到普通组最前");
    assert_eq!(bob["junkLastAt"], "2026-11-09T09:00:00.000Z");
    assert_eq!(bob["unreadCount"], 0);
    assert_eq!(bob["junkUnreadCount"], 1);

    let junk_items = list(&connection, GROUP_JUNK, None, None, 50, None).expect("list junk");
    let bob_junk = find(&junk_items, "bob@corp.test");
    assert_eq!(bob_junk["junkUnreadCount"], 1);

    let counts = group_counts(&connection).expect("group counts");
    assert_eq!(counts["normal"]["unread"], 0);
    assert_eq!(counts["junk"]["unread"], 1);
}

#[test]
fn badges_roll_up_per_account_and_overflow_can_be_rendered() {
    let connection = setup();
    let first = insert_message(&connection, 1, 1, 1, "来自主邮箱", "2026-11-01T09:00:00.000Z", 1);
    add_address(&connection, first, "from", "dave@corp.test", "Dave");
    add_address(&connection, first, "to", "me@a.test", "我");

    let second = insert_message(&connection, 2, 5, 2, "来自备用邮箱", "2026-11-03T09:00:00.000Z", 0);
    add_address(&connection, second, "from", "dave@corp.test", "Dave");
    add_address(&connection, second, "to", "me@b.test", "我");

    rebuild_all(&connection).expect("rebuild index");
    let items = list(&connection, GROUP_NORMAL, None, None, 50, None).expect("list normal");
    let dave = find(&items, "dave@corp.test");
    assert_eq!(dave["badgeTotal"], 2);
    let accounts = dave["accounts"].as_array().expect("accounts array");
    assert_eq!(accounts.len(), 2);
    assert!(accounts
        .iter()
        .any(|account| account["badge"] == "G" && account["color"] == "#ea4335"));
    assert!(accounts.iter().any(|account| account["badge"] == "QQ"));
    // 未读只统计备用邮箱那封
    assert_eq!(dave["unreadCount"], 1);
}

#[test]
fn read_state_change_updates_unread_counts() {
    let connection = setup();
    let message_id = insert_message(&connection, 1, 1, 1, "待读邮件", "2026-11-01T09:00:00.000Z", 0);
    add_address(&connection, message_id, "from", "erin@corp.test", "Erin");
    add_address(&connection, message_id, "to", "me@a.test", "我");
    rebuild_all(&connection).expect("rebuild index");

    let before = find(&list(&connection, GROUP_NORMAL, None, None, 50, None).unwrap(), "erin@corp.test");
    assert_eq!(before["unreadCount"], 1);

    connection
        .execute(
            "UPDATE onemail_mail_messages SET is_read=1 WHERE message_id=?1",
            [message_id],
        )
        .expect("mark read");
    on_read_state_changed(&connection, message_id).expect("refresh after read");

    let after = find(&list(&connection, GROUP_NORMAL, None, None, 50, None).unwrap(), "erin@corp.test");
    assert_eq!(after["unreadCount"], 0);
}

#[test]
fn thread_returns_full_history_including_junk() {
    let connection = setup();
    let normal = insert_message(&connection, 1, 1, 1, "第一封", "2026-11-01T09:00:00.000Z", 1);
    add_address(&connection, normal, "from", "frank@corp.test", "Frank");
    add_address(&connection, normal, "to", "me@a.test", "我");
    let junk = insert_message(&connection, 1, 2, 2, "垃圾一封", "2026-11-05T09:00:00.000Z", 0);
    add_address(&connection, junk, "from", "frank@corp.test", "Frank");
    add_address(&connection, junk, "to", "me@a.test", "我");
    let reply = insert_message(&connection, 1, 3, 3, "Re: 第一封", "2026-11-02T09:00:00.000Z", 1);
    add_address(&connection, reply, "from", "me@a.test", "我");
    add_address(&connection, reply, "to", "frank@corp.test", "Frank");

    rebuild_all(&connection).expect("rebuild index");
    let frank = find(&list(&connection, GROUP_NORMAL, None, None, 50, None).unwrap(), "frank@corp.test");
    let correspondent_id = frank["correspondentId"].as_i64().expect("id");

    let thread = thread(&connection, correspondent_id, None, 50).expect("thread");
    let messages = thread["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 3, "中栏应包含全部往来（含垃圾与我的去信）");
    assert!(messages.iter().any(|message| message["isJunk"] == true));
    assert!(messages.iter().any(|message| message["direction"] == "out"));
    // 倒序返回，最新在前
    assert_eq!(messages[0]["subject"], "垃圾一封");
}

#[test]
fn trash_and_drafts_do_not_create_correspondence() {
    let connection = setup();
    let trashed = insert_message(&connection, 1, 4, 1, "已删除邮件", "2026-11-01T09:00:00.000Z", 1);
    add_address(&connection, trashed, "from", "gina@corp.test", "Gina");
    add_address(&connection, trashed, "to", "me@a.test", "我");

    rebuild_all(&connection).expect("rebuild index");
    let items = list(&connection, GROUP_NORMAL, None, None, 50, None).expect("list normal");
    assert!(items["items"].as_array().unwrap().is_empty(), "回收站邮件不参与往来");
}

#[test]
fn junk_override_moves_a_message_between_groups() {
    let connection = setup();
    let message_id = insert_message(&connection, 1, 1, 1, "其实是垃圾", "2026-11-01T09:00:00.000Z", 1);
    add_address(&connection, message_id, "from", "hank@corp.test", "Hank");
    add_address(&connection, message_id, "to", "me@a.test", "我");
    rebuild_all(&connection).expect("rebuild index");

    // 收件箱邮件默认属普通组
    assert!(
        list(&connection, GROUP_JUNK, None, None, 50, None).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "普通邮件不应出现在垃圾组"
    );

    // 标为垃圾 → 只留垃圾组
    connection
        .execute(
            "UPDATE onemail_mail_messages SET junk_override=1 WHERE message_id=?1",
            [message_id],
        )
        .expect("mark junk");
    rebuild_all(&connection).expect("rebuild index");
    let junk = list(&connection, GROUP_JUNK, None, None, 50, None).unwrap();
    let hank = find(&junk, "hank@corp.test");
    assert_eq!(hank["junkUnreadCount"], 0, "该邮件已读，垃圾未读应为 0");
    assert!(hank["junkLastAt"].as_str().is_some());
    assert!(
        list(&connection, GROUP_NORMAL, None, None, 50, None).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "标为垃圾后不应留在普通组"
    );

    // 再标回“不是垃圾”
    connection
        .execute(
            "UPDATE onemail_mail_messages SET junk_override=0 WHERE message_id=?1",
            [message_id],
        )
        .expect("unmark junk");
    rebuild_all(&connection).expect("rebuild index");
    assert!(list(&connection, GROUP_JUNK, None, None, 50, None).unwrap()["items"]
        .as_array()
        .unwrap()
        .is_empty());
    let normal = list(&connection, GROUP_NORMAL, None, None, 50, None).unwrap();
    assert_eq!(find(&normal, "hank@corp.test")["inboundCount"], 1);
}

#[test]
fn user_deleted_messages_leave_the_index() {
    let connection = setup();
    let message_id = insert_message(&connection, 1, 1, 1, "待删除", "2026-11-01T09:00:00.000Z", 1);
    add_address(&connection, message_id, "from", "iris@corp.test", "Iris");
    add_address(&connection, message_id, "to", "me@a.test", "我");
    rebuild_all(&connection).expect("rebuild index");
    assert_eq!(
        find(&list(&connection, GROUP_NORMAL, None, None, 50, None).unwrap(), "iris@corp.test")
            ["inboundCount"],
        1
    );

    connection
        .execute(
            "UPDATE onemail_mail_messages SET user_deleted=1 WHERE message_id=?1",
            [message_id],
        )
        .expect("mark deleted");
    rebuild_all(&connection).expect("rebuild index");
    assert!(
        list(&connection, GROUP_NORMAL, None, None, 50, None).unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty(),
        "删除后联系人应离开索引"
    );
}
