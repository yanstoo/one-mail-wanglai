//! 邮件翻译。
//!
//! 不依赖任何浏览器扩展（本客户端是 WebKitGTK，装不了 Firefox 扩展），
//! 也不要求配置 AI：默认走 Google 的公开翻译端点，可在设置里换成
//! LibreTranslate（自托管/公共实例）、Azure Translator、DeepL 或内置 AI。
//!
//! 交互参考沉浸式翻译的做法：段落级、双语对照、结果缓存（缓存由前端负责）。

use std::time::Duration;

use reqwest::{redirect::Policy, Client};
use url::Url;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::{
    db,
    state::AppState,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_TEXT_CHARS: usize = 20_000;

pub const BACKEND_GOOGLE: &str = "google";
pub const BACKEND_LIBRE: &str = "libretranslate";
pub const BACKEND_AZURE: &str = "azure";
pub const BACKEND_DEEPL: &str = "deepl";

fn setting_key(name: &str) -> String {
    format!("translate.{name}")
}

fn read_setting(connection: &Connection, name: &str) -> Option<String> {
    connection
        .query_row(
            "SELECT setting_value FROM onemail_app_settings WHERE setting_key=?1",
            [setting_key(name)],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()
        .flatten()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn write_setting(connection: &Connection, name: &str, value: Option<&str>) -> Result<(), String> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(text) => connection
            .execute(
                "INSERT INTO onemail_app_settings (setting_key,setting_value,value_type)
                 VALUES (?1,?2,'string')
                 ON CONFLICT(setting_key) DO UPDATE SET setting_value=excluded.setting_value,
                   value_type=excluded.value_type,
                   updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
                params![setting_key(name), text],
            )
            .map(|_| ())
            .map_err(|error| format!("保存翻译设置失败：{error}")),
        None => connection
            .execute(
                "DELETE FROM onemail_app_settings WHERE setting_key=?1",
                [setting_key(name)],
            )
            .map(|_| ())
            .map_err(|error| format!("清除翻译设置失败：{error}")),
    }
}

#[derive(Clone)]
pub struct TranslateSettings {
    pub backend: String,
    /// 目标语言；空表示中文
    pub target_lang: String,
    pub libretranslate_url: Option<String>,
    pub api_key: Option<String>,
    pub api_region: Option<String>,
}

impl TranslateSettings {
    fn target(&self) -> String {
        match self.target_lang.trim() {
            "" => "zh-CN".to_string(),
            other => other.to_string(),
        }
    }
}

pub fn load_settings(connection: &Connection) -> TranslateSettings {
    TranslateSettings {
        backend: read_setting(connection, "backend").unwrap_or_else(|| BACKEND_GOOGLE.to_string()),
        target_lang: read_setting(connection, "target_lang").unwrap_or_default(),
        libretranslate_url: read_setting(connection, "libretranslate_url"),
        api_key: read_setting(connection, "api_key"),
        api_region: read_setting(connection, "api_region"),
    }
}

fn build_client() -> Result<Client, String> {
    Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(Policy::none())
        .user_agent("OneMail/1.0 (mail client translation)")
        .build()
        .map_err(|_| "无法创建翻译请求客户端。".to_string())
}

/// 把 Google 免费端点的返回结构拍平成纯文本。
///
/// 两种形状都要支持：
/// - `clients5`（默认）：`[["你好","en"]]`，多段时是 `[["甲","en"],["乙","en"]]`
/// - `gtx`（回退）：`[[["甲","a",null,null,10],["乙","b",null,null,10]],null,"en"]`
fn parse_google_response(body: &Value) -> Option<String> {
    let rows = body.as_array()?;
    let mut parts: Vec<String> = Vec::new();

    for row in rows {
        match row {
            // gtx 顶层还会带检测语言（如 "en"），不能当译文
            Value::Array(items) => {
                if let Some(first) = items.first() {
                    match first {
                        // clients5：每行第一项就是译文
                        Value::String(text) => parts.push(text.clone()),
                        // gtx：整行是一个段落数组
                        Value::Array(_) => {
                            for segment in items {
                                if let Some(text) = segment.get(0).and_then(Value::as_str) {
                                    parts.push(text.to_string());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    let out = parts.join("").trim().to_string();
    (!out.is_empty()).then_some(out)
}

async fn translate_google(client: &Client, text: &str, target: &str) -> Result<String, String> {
    // 首选：Chrome 词典客户端端点（社区长期在用，返回干净）
    let mut primary = Url::parse("https://clients5.google.com/translate_a/t")
        .map_err(|error| format!("构造 Google 翻译地址失败：{error}"))?;
    {
        let mut pairs = primary.query_pairs_mut();
        pairs.append_pair("client", "dict-chrome-ex");
        pairs.append_pair("sl", "auto");
        pairs.append_pair("tl", target);
        pairs.append_pair("q", text);
    }
    match request_google(client, primary.clone()).await {
        Ok(translated) => return Ok(translated),
        Err(primary_error) => {
            // 回退：gtx 端点（部分网络环境下可行）
            let mut fallback = Url::parse("https://translate.googleapis.com/translate_a/single")
                .map_err(|error| format!("构造 Google 翻译地址失败：{error}"))?;
            {
                let mut pairs = fallback.query_pairs_mut();
                pairs.append_pair("client", "gtx");
                pairs.append_pair("sl", "auto");
                pairs.append_pair("tl", target);
                pairs.append_pair("dt", "t");
                pairs.append_pair("q", text);
            }
            request_google(client, fallback).await.map_err(|fallback_error| {
                format!("Google 翻译失败（两个端点均不可用）：{primary_error} / {fallback_error}")
            })
        }
    }
}

async fn request_google(client: &Client, url: Url) -> Result<String, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("请求 Google 翻译失败：{error}"))?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status.as_u16()));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("解析 Google 翻译结果失败：{error}"))?;
    parse_google_response(&body).ok_or_else(|| "没有返回译文".to_string())
}

async fn translate_libretranslate(
    client: &Client,
    settings: &TranslateSettings,
    text: &str,
) -> Result<String, String> {
    let base = settings
        .libretranslate_url
        .as_deref()
        .ok_or_else(|| "还没有填写 LibreTranslate 服务地址。".to_string())?;
    let url = format!("{}/translate", base.trim_end_matches('/'));
    let mut payload = json!({
        "q": text,
        "source": "auto",
        "target": settings.target(),
        "format": "text"
    });
    if let Some(key) = settings.api_key.as_deref() {
        payload["api_key"] = Value::String(key.to_string());
    }
    let response = client
        .post(url)
        .json(&payload)
        .send()
        .await
        .map_err(|error| format!("请求 LibreTranslate 失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("LibreTranslate 返回 HTTP {}", response.status().as_u16()));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("解析 LibreTranslate 结果失败：{error}"))?;
    body.get("translatedText")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "LibreTranslate 没有返回译文。".to_string())
}

async fn translate_azure(
    client: &Client,
    settings: &TranslateSettings,
    text: &str,
) -> Result<String, String> {
    let key = settings
        .api_key
        .as_deref()
        .ok_or_else(|| "还没有填写 Azure Translator 密钥。".to_string())?;
    let region = settings
        .api_region
        .as_deref()
        .ok_or_else(|| "还没有填写 Azure 资源区域（region）。".to_string())?;
    let mut url = Url::parse("https://api.cognitive.microsofttranslator.com/translate")
        .map_err(|error| format!("构造 Azure 翻译地址失败：{error}"))?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("api-version", "3.0");
        pairs.append_pair("to", &settings.target());
    }
    let response = client
        .post(url)
        .header("Ocp-Apim-Subscription-Key", key)
        .header("Ocp-Apim-Subscription-Region", region)
        .json(&json!([{ "Text": text }]))
        .send()
        .await
        .map_err(|error| format!("请求 Azure 翻译失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Azure 翻译返回 HTTP {}（检查密钥与区域）",
            response.status().as_u16()
        ));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("解析 Azure 翻译结果失败：{error}"))?;
    body.get(0)
        .and_then(|item| item.get("translations"))
        .and_then(|items| items.get(0))
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "Azure 没有返回译文。".to_string())
}

async fn translate_deepl(
    client: &Client,
    settings: &TranslateSettings,
    text: &str,
) -> Result<String, String> {
    let key = settings
        .api_key
        .as_deref()
        .ok_or_else(|| "还没有填写 DeepL API 密钥。".to_string())?;
    // Free 版端点是 api-free.deepl.com；带 :fx 后缀的密钥走免费端点
    let endpoint = if key.ends_with(":fx") {
        "https://api-free.deepl.com/v2/translate"
    } else {
        "https://api.deepl.com/v2/translate"
    };
    let target = settings.target().to_uppercase();
    let response = client
        .post(endpoint)
        .header("Authorization", format!("DeepL-Auth-Key {key}"))
        .form(&[("text", text), ("target_lang", target.as_str())])
        .send()
        .await
        .map_err(|error| format!("请求 DeepL 失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("DeepL 返回 HTTP {}（检查密钥）", response.status().as_u16()));
    }
    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("解析 DeepL 结果失败：{error}"))?;
    body.get("translations")
        .and_then(|items| items.get(0))
        .and_then(|item| item.get("text"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "DeepL 没有返回译文。".to_string())
}

/// 翻译一段文本。`backend` 为空时用设置里的默认后端。
pub async fn translate(state: &AppState, text: &str, backend_override: Option<&str>) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("没有可翻译的内容。".to_string());
    }
    if trimmed.chars().count() > MAX_TEXT_CHARS {
        return Err(format!("文本过长（超过 {MAX_TEXT_CHARS} 字），请分段翻译。"));
    }

    let (settings, backend) = {
        let connection = db::open(state)?;
        let settings = load_settings(&connection);
        let backend = backend_override
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| settings.backend.clone());
        (settings, backend)
    };

    if backend == "ai" {
        return Err("AI 翻译请使用阅读面板的 AI 入口（本功能不经过内置 AI）。".to_string());
    }

    let client = build_client()?;
    match backend.as_str() {
        BACKEND_GOOGLE => translate_google(&client, trimmed, &settings.target()).await,
        BACKEND_LIBRE => translate_libretranslate(&client, &settings, trimmed).await,
        BACKEND_AZURE => translate_azure(&client, &settings, trimmed).await,
        BACKEND_DEEPL => translate_deepl(&client, &settings, trimmed).await,
        other => Err(format!("未知的翻译后端：{other}")),
    }
}

/// 读取翻译设置（不回传密钥本身，只标记是否已配置）。
pub fn settings_to_json(connection: &Connection) -> Value {
    let settings = load_settings(connection);
    json!({
        "backend": settings.backend,
        "targetLang": settings.target_lang,
        "libretranslateUrl": settings.libretranslate_url,
        "apiKeyConfigured": settings.api_key.is_some(),
        "apiRegion": settings.api_region,
        "googleUnofficial": settings.backend == BACKEND_GOOGLE,
    })
}

pub fn save_settings(connection: &Connection, input: &Value) -> Result<Value, String> {
    if let Some(backend) = input.get("backend").and_then(Value::as_str) {
        let backend = backend.trim();
        if !matches!(
            backend,
            BACKEND_GOOGLE | BACKEND_LIBRE | BACKEND_AZURE | BACKEND_DEEPL
        ) {
            return Err(format!("不支持的翻译后端：{backend}"));
        }
        write_setting(connection, "backend", Some(backend))?;
    }
    if let Some(target) = input.get("targetLang").and_then(Value::as_str) {
        write_setting(connection, "target_lang", Some(target))?;
    }
    if input.get("libretranslateUrl").is_some() {
        write_setting(
            connection,
            "libretranslate_url",
            input.get("libretranslateUrl").and_then(Value::as_str),
        )?;
    }
    if input.get("apiRegion").is_some() {
        write_setting(
            connection,
            "api_region",
            input.get("apiRegion").and_then(Value::as_str),
        )?;
    }
    // 传空字符串表示清除密钥
    if input.get("apiKey").is_some() {
        let key = input.get("apiKey").and_then(Value::as_str);
        write_setting(connection, "api_key", key)?;
    }
    Ok(settings_to_json(connection))
}

#[cfg(test)]
mod tests {
    use super::parse_google_response;
    use serde_json::json;

    #[test]
    fn parses_google_multisegment_response() {
        let body = json!([
            [["第一段", "first", null, null, 10], ["第二段", "second", null, null, 10]],
            null,
            "en"
        ]);
        assert_eq!(parse_google_response(&body).as_deref(), Some("第一段第二段"));
    }

    #[test]
    fn parses_clients5_shape() {
        // clients5.google.com 的真实返回：单段 / 多段
        assert_eq!(
            parse_google_response(&json!([["你好", "en"]])).as_deref(),
            Some("你好")
        );
        assert_eq!(
            parse_google_response(&json!([["你好", "en"], ["世界", "en"]])).as_deref(),
            Some("你好世界")
        );
    }

    #[test]
    fn rejects_empty_google_response() {
        assert_eq!(parse_google_response(&json!([[], null, "en"])), None);
        assert_eq!(parse_google_response(&json!({})), None);
    }
}
