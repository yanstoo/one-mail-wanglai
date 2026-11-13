mod folders;

pub use folders::{decode_modified_utf7, folder_role, is_folder_selectable};
use folders::name_attribute_value;

use std::time::Duration;

use aes_gcm::{aead::Aead, KeyInit};
use async_imap::{Authenticator, Client};
use async_native_tls::TlsConnector;
use base64::Engine;
use futures_util::TryStreamExt;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

use crate::{db, oauth, state::AppState};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const DISCOVER_TIMEOUT: Duration = Duration::from_secs(30);
const IMAP_CLIENT_NAME: &str = "OneMail";
const IMAP_CLIENT_VENDOR: &str = "Huzhihui";
const IMAP_CLIENT_SUPPORT_URL: &str = "https://github.com/zhihui-hu/one-mail";

pub(crate) trait ImapStream:
    AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug
{
}

impl<T> ImapStream for T where T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug {}

pub(crate) type ImapSession = async_imap::Session<Box<dyn ImapStream>>;

#[derive(Clone, Debug)]
pub struct ImapConnectionConfig {
    pub email: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImapFolder {
    pub path: String,
    pub name: String,
    pub delimiter: Option<String>,
    pub role: String,
    pub attributes: Vec<String>,
    pub is_selectable: bool,
}

#[derive(Clone, Debug)]
pub struct MailAccount {
    pub account_id: i64,
    pub provider_key: String,
    pub email: String,
    pub auth_type: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_security: String,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_security: Option<String>,
    pub smtp_auth_type: Option<String>,
    pub smtp_enabled: bool,
    pub encrypted_password: Option<String>,
}

pub fn load_account(state: &AppState, account_id: i64) -> Result<MailAccount, String> {
    let connection = db::open(state)?;
    connection
        .query_row(
            "SELECT account_id,provider_key,email,auth_type,imap_host,imap_port,imap_security,
                    smtp_host,smtp_port,smtp_security,smtp_auth_type,smtp_enabled,encrypted_password
             FROM onemail_mail_accounts WHERE account_id=?1",
            [account_id],
            |row| {
                let imap_port = row.get::<_, i64>(5)?;
                let smtp_port = row.get::<_, Option<i64>>(8)?;
                Ok(MailAccount {
                    account_id: row.get(0)?,
                    provider_key: row.get(1)?,
                    email: row.get(2)?,
                    auth_type: row.get(3)?,
                    imap_host: row.get(4)?,
                    imap_port: u16::try_from(imap_port).unwrap_or(0),
                    imap_security: row.get(6)?,
                    smtp_host: row.get(7)?,
                    smtp_port: smtp_port.and_then(|port| u16::try_from(port).ok()),
                    smtp_security: row.get(9)?,
                    smtp_auth_type: row.get(10)?,
                    smtp_enabled: row.get::<_, i64>(11)? != 0,
                    encrypted_password: row.get(12)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("读取邮箱账号失败：{error}"))?
        .ok_or_else(|| format!("账号不存在：{account_id}"))
}

pub async fn connect_authenticated(
    state: &AppState,
    account: &MailAccount,
) -> Result<ImapSession, String> {
    let config = ImapConnectionConfig::from(account);
    if account.auth_type != "oauth2" {
        let password = password(state, account)?;
        return connect_with_password(&config, &password).await;
    }

    let client = connect_client(&config).await?;
    let token = oauth::access_token(state, account.account_id, &account.provider_key).await?;
    let login_hints = vec![account.email.clone()];
    match authenticate_xoauth2(client, &login_hints, &token.access_token).await {
        Ok(session) => {
            let _ =
                oauth::set_connection_state(state, account.account_id, "connected", false, None);
            Ok(session)
        }
        Err(first_error) if is_outlook_mailbox_access_error(account, &first_error) => {
            Err(outlook_mailbox_access_error(state, account, &first_error))
        }
        Err(first_error) if is_xoauth2_auth_error(&first_error) => {
            let refreshed = oauth::force_refresh_access_token(
                state,
                account.account_id,
                &account.provider_key,
                Some(&token.access_token),
            )
            .await?;
            let client = connect_client(&config).await?;
            match authenticate_xoauth2(client, &login_hints, &refreshed.access_token).await {
                Ok(session) => {
                    let _ = oauth::set_connection_state(
                        state,
                        account.account_id,
                        "connected",
                        false,
                        None,
                    );
                    Ok(session)
                }
                Err(second_error) => {
                    if is_outlook_mailbox_access_error(account, &second_error) {
                        return Err(outlook_mailbox_access_error(state, account, &second_error));
                    }
                    let message = format!("IMAP OAuth 登录认证失败，请重新授权：{second_error}");
                    let _ = oauth::set_connection_state(
                        state,
                        account.account_id,
                        "reauthorize",
                        true,
                        Some(&message),
                    );
                    Err(message)
                }
            }
        }
        Err(error) => Err(format!("IMAP OAuth 登录认证失败：{error}")),
    }
}

pub async fn discover_folders(
    config: &ImapConnectionConfig,
    password: &str,
) -> Result<Vec<ImapFolder>, String> {
    tokio::time::timeout(DISCOVER_TIMEOUT, discover_folders_inner(config, password))
        .await
        .map_err(|_| "列举 IMAP 文件夹超时。".to_string())?
}

async fn discover_folders_inner(
    config: &ImapConnectionConfig,
    password: &str,
) -> Result<Vec<ImapFolder>, String> {
    let mut session = connect_with_password(config, password).await?;
    let mut folders = Vec::new();
    let mut stream = session
        .list(Some(""), Some("*"))
        .await
        .map_err(|error| format!("列举 IMAP 文件夹失败：{error}"))?;
    while let Some(folder) = stream
        .try_next()
        .await
        .map_err(|error| format!("读取 IMAP 文件夹失败：{error}"))?
    {
        let path = folder.name().to_string();
        let attributes = folder
            .attributes()
            .iter()
            .map(name_attribute_value)
            .collect::<Vec<_>>();
        folders.push(ImapFolder {
            name: decode_modified_utf7(&path),
            role: folder_role(&path, &attributes).to_string(),
            is_selectable: is_folder_selectable(&attributes),
            path,
            delimiter: folder.delimiter().map(str::to_string),
            attributes,
        });
    }
    drop(stream);
    session
        .logout()
        .await
        .map_err(|error| format!("关闭 IMAP 连接失败：{error}"))?;
    Ok(folders)
}

async fn connect_with_password(
    config: &ImapConnectionConfig,
    password: &str,
) -> Result<ImapSession, String> {
    let mut session = connect_client(config)
        .await?
        .login(&config.email, password)
        .await
        .map_err(|(error, _)| format!("IMAP 登录认证失败：{error}"))?;
    announce_imap_client_id(&mut session).await;
    Ok(session)
}



/// 直接向服务器枚举全部可选文件夹：(path, role)。
///
/// 抓正文时不能只依赖本地已同步的文件夹——邮件可能被服务端规则移到一个
/// 从未同步过的文件夹，那就永远找不到。
pub async fn list_all_folders(
    state: &AppState,
    account: &MailAccount,
) -> Result<Vec<(String, String)>, String> {
    let mut session = connect_authenticated(state, account).await?;
    let mut folders = Vec::new();
    {
        let mut stream = session
            .list(Some(""), Some("*"))
            .await
            .map_err(|error| format!("列举 IMAP 文件夹失败：{error}"))?;
        while let Some(folder) = stream
            .try_next()
            .await
            .map_err(|error| format!("读取 IMAP 文件夹失败：{error}"))?
        {
            let path = folder.name().to_string();
            let attributes = folder
                .attributes()
                .iter()
                .map(name_attribute_value)
                .collect::<Vec<_>>();
            if !is_folder_selectable(&attributes) {
                continue;
            }
            let role = folder_role(&path, &attributes).to_string();
            folders.push((path, role));
        }
    }
    let _ = session.logout().await;
    Ok(folders)
}

/// 在指定文件夹里按 RFC822 Message-ID 搜索 UID。
///
/// 注意：IMAP 的 `HEADER` 搜索键后跟的是 astring，Message-ID 里的 `<>@`
/// 必须整体加引号，否则服务器可能解析失败或匹配不到。
pub async fn search_uid_by_message_id(
    state: &AppState,
    account: &MailAccount,
    folder_path: &str,
    message_id: &str,
) -> Result<Option<u32>, String> {
    let trimmed = message_id.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let mut session = connect_authenticated(state, account).await?;
    session
        .select(folder_path)
        .await
        .map_err(|error| format!("打开邮箱文件夹失败：{error}"))?;
    let query = format!("HEADER Message-ID \"{trimmed}\"");
    let found = session
        .uid_search(query)
        .await
        .map_err(|error| format!("按 Message-ID 查找邮件失败：{error}"))?
        .into_iter()
        .next();
    let _ = session.logout().await;
    Ok(found)
}


/// 分段抓取：部分服务器（大小限制/实现缺陷）会拒绝整体 `BODY.PEEK[]`，
/// 但对 `BODY.PEEK[HEADER]` 与带区间的 `BODY.PEEK[TEXT]<off.len>` 正常响应。
/// 拼成完整 RFC822 消息返回（头部 + 正文）。
async fn fetch_partial_message(session: &mut ImapSession, uid: u32) -> Result<Vec<u8>, String> {
    const CHUNK: usize = 60_000;
    const MAX_CHUNKS: usize = 200;

    let mut header = Vec::new();
    {
        let mut stream = session
            .uid_fetch(uid.to_string(), "BODY.PEEK[HEADER]")
            .await
            .map_err(|error| format!("读取邮件头部失败：{error}"))?;
        while let Some(fetch) = stream
            .try_next()
            .await
            .map_err(|error| format!("读取邮件头部失败：{error}"))?
        {
            if let Some(value) = fetch.body() {
                header = value.to_vec();
                break;
            }
        }
    }
    if header.is_empty() {
        return Err("服务器连邮件头部都没有返回".to_string());
    }

    let mut text: Vec<u8> = Vec::new();
    for index in 0..MAX_CHUNKS {
        let start = index * CHUNK;
        let request = format!("BODY.PEEK[TEXT]<{start}.{CHUNK}>");
        let mut stream = session
            .uid_fetch(uid.to_string(), request)
            .await
            .map_err(|error| format!("分段读取邮件正文失败：{error}"))?;
        let mut received = 0usize;
        while let Some(fetch) = stream
            .try_next()
            .await
            .map_err(|error| format!("分段读取邮件正文失败：{error}"))?
        {
            if let Some(value) = fetch.body() {
                received = value.len();
                text.extend_from_slice(value);
                break;
            }
        }
        if received < CHUNK {
            break;
        }
    }
    if text.is_empty() {
        return Err("服务器分段返回的正文也是空的".to_string());
    }

    let mut combined = header;
    if !combined.ends_with(b"\r\n") {
        combined.extend_from_slice(b"\r\n");
    }
    combined.extend_from_slice(b"\r\n");
    combined.extend_from_slice(&text);
    Ok(combined)
}

pub async fn fetch_raw_message(
    state: &AppState,
    account: &MailAccount,
    folder_path: &str,
    uid: u32,
    message_id: Option<&str>,
) -> Result<(Vec<u8>, Option<u32>), String> {
    if uid == 0 {
        return Err("邮件 UID 无效。".to_string());
    }
    let _network_request = state.network_activity.begin_for_account("body", account.account_id);
    let mut session = connect_authenticated(state, account).await?;
    session
        .select(folder_path)
        .await
        .map_err(|error| format!("打开邮箱文件夹失败：{error}"))?;
    // 不同服务器对 BODY.PEEK[] 的支持程度不同（部分国内服务商只给 RFC822 或 TEXT），
    // 依次尝试，取第一个拿到的完整内容
    let mut body: Option<Vec<u8>> = None;
    let mut attempts: Vec<String> = Vec::new();
    for spec in ["BODY.PEEK[]", "RFC822.PEEK"] {
        let mut stream = match session.uid_fetch(uid.to_string(), spec).await {
            Ok(stream) => stream,
            Err(error) => {
                attempts.push(format!("{spec}: {error}"));
                continue;
            }
        };
        while let Some(fetch) = stream.try_next().await.map_err(|error| format!("读取邮件正文失败：{error}"))? {
            if let Some(value) = fetch.body() {
                body = Some(value.to_vec());
                break;
            }
            if fetch.header().is_none() && fetch.body().is_none() {
                attempts.push(format!("{spec}: 服务器返回空内容"));
            }
        }
        drop(stream);
        if body.is_some() {
            break;
        }
    }

    // 最后兜底：只要正文文本（无头），用合成头包一层让解析器能识别编码
    if body.is_none() {
        let mut stream = session
            .uid_fetch(uid.to_string(), "BODY.PEEK[TEXT]")
            .await
            .map_err(|error| format!("读取邮件正文失败：{error}"))?;
        while let Some(fetch) = stream
            .try_next()
            .await
            .map_err(|error| format!("读取邮件正文失败：{error}"))?
        {
            if let Some(value) = fetch.body() {
                let mut wrapped = b"Content-Type: text/plain; charset=utf-8\r\n\r\n".to_vec();
                wrapped.extend_from_slice(value);
                body = Some(wrapped);
                break;
            }
        }
        drop(stream);
        if body.is_none() {
            attempts.push("BODY.PEEK[TEXT]: 服务器返回空内容".to_string());
        }
    }

    session
        .logout()
        .await
        .ok();

    // UID 可能失效：服务器改了 UIDVALIDITY，或这封邮件被服务端规则移动到别的文件夹。
    // 用 RFC822 Message-ID 在同一个文件夹里重新定位一次。
    let mut resolved_uid = None;
    if body.is_none() {
        if let Some(message_id) = message_id.map(str::trim).filter(|value| !value.is_empty()) {
            let search = session
                .uid_search(format!("HEADER Message-ID \"{message_id}\""))
                .await
                .map_err(|error| format!("按 Message-ID 查找邮件失败：{error}"))?;
            if let Some(found) = search.iter().copied().next() {
                let mut stream = session
                    .uid_fetch(found.to_string(), "BODY.PEEK[]")
                    .await
                    .map_err(|error| format!("读取邮件正文失败：{error}"))?;
                while let Some(fetch) = stream
                    .try_next()
                    .await
                    .map_err(|error| format!("读取邮件正文失败：{error}"))?
                {
                    if let Some(value) = fetch.body() {
                        body = Some(value.to_vec());
                        resolved_uid = Some(found);
                        break;
                    }
                }
                drop(stream);
                if body.is_none() {
                    attempts.push(format!("按 Message-ID 命中 uid={found} 但正文为空"));
                }
            } else {
                attempts.push("按 Message-ID 未在该文件夹找到该邮件（可能已被移动或删除）".to_string());
            }
        }
    }

    // 最后兜底：分段抓取（应对服务器对整封邮件的大小限制）
    if body.is_none() {
        match fetch_partial_message(&mut session, uid).await {
            Ok(value) => body = Some(value),
            Err(error) => attempts.push(format!("分段抓取: {error}")),
        }
    }

    body.map(|value| (value, resolved_uid)).ok_or_else(|| {
        format!(
            "IMAP 没有返回邮件正文（uid={uid}，文件夹 {folder_path}）：{}",
            attempts.join("; ")
        )
    })
}

async fn connect_client(
    config: &ImapConnectionConfig,
) -> Result<Client<Box<dyn ImapStream>>, String> {
    if config.imap_port == 0 {
        return Err("IMAP 端口无效。".to_string());
    }
    if !matches!(
        config.imap_security.as_str(),
        "ssl_tls" | "starttls" | "none"
    ) {
        return Err("不支持的 IMAP 加密方式。".to_string());
    }
    let tcp = tokio::time::timeout(
        CONNECT_TIMEOUT,
        TcpStream::connect((config.imap_host.as_str(), config.imap_port)),
    )
    .await
    .map_err(|_| "连接 IMAP 服务器超时。".to_string())?
    .map_err(|error| format!("连接 IMAP 服务器失败：{error}"))?;

    if config.imap_security == "none" {
        let stream: Box<dyn ImapStream> = Box::new(tcp);
        let mut client = Client::new(stream);
        client
            .read_response()
            .await
            .map_err(|error| format!("读取 IMAP 欢迎语失败：{error}"))?;
        return Ok(client);
    }

    if config.imap_security == "starttls" {
        let mut client = Client::new(tcp);
        client
            .read_response()
            .await
            .map_err(|error| format!("读取 IMAP 欢迎语失败：{error}"))?;
        client
            .run_command_and_check_ok("STARTTLS", None)
            .await
            .map_err(|error| format!("IMAP STARTTLS 失败：{error}"))?;
        let stream = client.into_inner();
        let tls = TlsConnector::new()
            .connect(&config.imap_host, stream)
            .await
            .map_err(|error| format!("IMAP TLS 握手失败：{error}"))?;
        let stream: Box<dyn ImapStream> = Box::new(tls);
        return Ok(Client::new(stream));
    }

    let tls = TlsConnector::new()
        .connect(&config.imap_host, tcp)
        .await
        .map_err(|error| format!("IMAP TLS 握手失败：{error}"))?;
    let stream: Box<dyn ImapStream> = Box::new(tls);
    let mut client = Client::new(stream);
    client
        .read_response()
        .await
        .map_err(|error| format!("读取 IMAP 欢迎语失败：{error}"))?;
    Ok(client)
}

impl From<&MailAccount> for ImapConnectionConfig {
    fn from(account: &MailAccount) -> Self {
        Self {
            email: account.email.clone(),
            imap_host: account.imap_host.clone(),
            imap_port: account.imap_port,
            imap_security: account.imap_security.clone(),
        }
    }
}

async fn authenticate_xoauth2(
    mut client: Client<Box<dyn ImapStream>>,
    login_hints: &[String],
    access_token: &str,
) -> Result<ImapSession, String> {
    let mut last_error = None;
    for username in login_hints {
        let auth = XOAuth2 {
            username,
            access_token,
        };
        match client.authenticate("XOAUTH2", auth).await {
            Ok(mut session) => {
                announce_imap_client_id(&mut session).await;
                return Ok(session);
            }
            Err((error, next_client)) => {
                last_error = Some(error.to_string());
                client = next_client;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| "XOAUTH2 认证失败。".to_string()))
}

async fn announce_imap_client_id(session: &mut ImapSession) {
    let _ = session.id(imap_client_identification()).await;
}

fn imap_client_identification() -> [(&'static str, Option<&'static str>); 4] {
    [
        ("name", Some(IMAP_CLIENT_NAME)),
        ("version", Some(env!("CARGO_PKG_VERSION"))),
        ("vendor", Some(IMAP_CLIENT_VENDOR)),
        ("support-url", Some(IMAP_CLIENT_SUPPORT_URL)),
    ]
}

fn is_xoauth2_auth_error(error: &str) -> bool {
    if is_authenticated_but_not_connected(error) {
        return false;
    }
    let message = error.to_ascii_lowercase();
    message.contains("auth")
        || message.contains("oauth")
        || message.contains("invalid credentials")
        || message.contains("not authenticated")
}

fn is_authenticated_but_not_connected(error: &str) -> bool {
    error.to_ascii_lowercase().contains("authenticated but not connected")
}

fn is_outlook_mailbox_access_error(account: &MailAccount, error: &str) -> bool {
    matches!(account.provider_key.to_ascii_lowercase().as_str(), "outlook" | "microsoft")
        && is_authenticated_but_not_connected(error)
}

fn outlook_mailbox_access_error(state: &AppState, account: &MailAccount, error: &str) -> String {
    let message = format!("Outlook IMAP 连接被拒绝：{error}");
    let _ = oauth::set_connection_state(state, account.account_id, "connected", false, Some(&message));
    message
}

pub fn password(state: &AppState, account: &MailAccount) -> Result<String, String> {
    let encrypted = account
        .encrypted_password
        .as_deref()
        .ok_or_else(|| "账号凭据不存在，请编辑账号并重新保存密码。".to_string())?;
    decrypt_secret(&state.database_key()?, encrypted)
}

#[derive(serde::Deserialize)]
struct EncryptedPayload {
    version: u8,
    alg: String,
    iv: String,
    #[serde(rename = "authTag")]
    auth_tag: String,
    ciphertext: String,
}

fn decrypt_secret(database_key: &str, value: &str) -> Result<String, String> {
    let payload: EncryptedPayload = serde_json::from_slice(
        &base64::engine::general_purpose::STANDARD
            .decode(value)
            .map_err(|_| "账号凭据格式无效，请编辑账号并重新保存密码。".to_string())?,
    )
    .map_err(|_| "账号凭据格式无效，请编辑账号并重新保存密码。".to_string())?;
    if payload.version != 1 || payload.alg != "aes-256-gcm" {
        return Err("账号凭据格式不支持，请编辑账号并重新保存密码。".to_string());
    }
    let iv = base64::engine::general_purpose::STANDARD
        .decode(payload.iv)
        .map_err(|_| "账号凭据格式无效，请编辑账号并重新保存密码。".to_string())?;
    let mut encrypted = base64::engine::general_purpose::STANDARD
        .decode(payload.ciphertext)
        .map_err(|_| "账号凭据格式无效，请编辑账号并重新保存密码。".to_string())?;
    encrypted.extend(
        base64::engine::general_purpose::STANDARD
            .decode(payload.auth_tag)
            .map_err(|_| "账号凭据格式无效，请编辑账号并重新保存密码。".to_string())?,
    );
    let key = sha2::Sha256::digest(database_key.as_bytes());
    let cipher = aes_gcm::Aes256Gcm::new_from_slice(&key)
        .map_err(|_| "账号凭据解密失败，请编辑账号并重新保存密码。".to_string())?;
    let plaintext = cipher
        .decrypt(aes_gcm::Nonce::from_slice(&iv), encrypted.as_ref())
        .map_err(|_| "账号凭据解密失败，请编辑账号并重新保存密码。".to_string())?;
    String::from_utf8(plaintext)
        .map_err(|_| "账号凭据内容无效，请编辑账号并重新保存密码。".to_string())
}

struct XOAuth2<'a> {
    username: &'a str,
    access_token: &'a str,
}

impl Authenticator for XOAuth2<'_> {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.username, self.access_token
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        decode_modified_utf7, folder_role, imap_client_identification, is_folder_selectable,
        is_xoauth2_auth_error,
        IMAP_CLIENT_NAME, IMAP_CLIENT_SUPPORT_URL, IMAP_CLIENT_VENDOR,
    };

    #[test]
    fn authenticated_but_not_connected_is_not_a_token_error() {
        assert!(!is_xoauth2_auth_error("AUTHENTICATE failed: User is authenticated but not connected"));
        assert!(is_xoauth2_auth_error("AUTHENTICATE failed: invalid credentials"));
    }

    #[test]
    fn decodes_modified_utf7_mailbox_names_and_literal_ampersands() {
        assert_eq!(
            decode_modified_utf7("~peter/mail/&U,BTFw-/&ZeVnLIqe-"),
            "~peter/mail/台北/日本語"
        );
        assert_eq!(decode_modified_utf7("R&-D &- Sales"), "R&D & Sales");
    }

    #[test]
    fn preserves_invalid_modified_utf7_segments() {
        assert_eq!(decode_modified_utf7("Folder/&bad!-"), "Folder/&bad!-");
        assert_eq!(decode_modified_utf7("Folder/&broken"), "Folder/&broken");
    }

    #[test]
    fn maps_special_use_attributes_without_guessing_custom_names() {
        assert_eq!(folder_role("Inbox", &[]), "inbox");
        assert_eq!(folder_role("已发送", &["\\Sent".to_string()]), "sent");
        assert_eq!(
            folder_role("Everything", &["\\ALL".to_string()]),
            "all_mail"
        );
        assert_eq!(folder_role("Sent Items", &[]), "custom");
    }

    #[test]
    fn keeps_non_selectable_folders_but_marks_them_unselectable() {
        assert!(!is_folder_selectable(&["\\Noselect".to_string()]));
        assert!(!is_folder_selectable(&["\\nonexistent".to_string()]));
        assert!(is_folder_selectable(&["\\HasChildren".to_string()]));
    }

    #[test]
    fn builds_imap_client_identification_for_id_command() {
        assert_eq!(
            imap_client_identification(),
            [
                ("name", Some(IMAP_CLIENT_NAME)),
                ("version", Some(env!("CARGO_PKG_VERSION"))),
                ("vendor", Some(IMAP_CLIENT_VENDOR)),
                ("support-url", Some(IMAP_CLIENT_SUPPORT_URL)),
            ]
        );
    }
}
