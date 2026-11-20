mod ai;
mod background;
mod client_log;
mod commands;
mod db;
mod mail;
mod mcp;
mod oauth;
mod state;
mod translate;

use commands::{backup, data, system};
use state::AppState;
use tauri::Manager;

#[cfg(target_os = "macos")]
pub fn send_bundled_notification(
    title: &str,
    body: &str,
    sound: Option<&str>,
    icon_path: Option<&str>,
    interactive: bool,
) -> Result<mac_usernotifications::NotificationHandle, String> {
    let authorized = mac_usernotifications::blocking::request_auth()
        .map_err(|error| error.to_string())?;
    if !authorized {
        return Err("系统通知权限未获批准".to_owned());
    }
    let mut notification = mac_usernotifications::Notification::new()
        .title(title)
        .message(body);
    if sound.is_some() {
        notification = notification.default_sound();
    }
    if let Some(icon_path) = icon_path {
        notification = notification.image_path(icon_path);
    }
    if interactive {
        notification = notification.timeout(std::time::Duration::from_secs(8 * 60 * 60));
    }
    notification
        .send_blocking()
        .map_err(|error| error.to_string())
}

/// 启动后延迟预热设置窗口。
///
/// 设置窗口加载的是同一个 SPA 包，首次创建要付整包解析 + 首屏渲染的成本
/// （dev 下还要走 Vite 逐模块编译）。提前在后台建好并保持隐藏，
/// 用户真正点「设置」时只需要 show，几乎是即时的。
fn prewarm_settings_window(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(2500));
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            if let Err(error) = system::settings_window(&handle, false) {
                eprintln!("OneMail settings prewarm failed: {error}");
            }
        });
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install the rustls crypto provider");
    let builder = tauri::Builder::default();

    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        if let Some(message_id) = system::message_id_from_args(&args) {
            system::open_notification_message(app, message_id);
        }
    }));

    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            if let Some(message_id) = system::message_id_from_args(&std::env::args().collect::<Vec<_>>()) {
                system::queue_notification_message(message_id);
            }
            let state = AppState::initialize(app.handle())?;
            db::initialize(&state)?;
            if let Err(error) = client_log::cleanup(app.handle(), client_log::retention_days(&state)) {
                eprintln!("OneMail log cleanup failed: {error}");
            }
            data::oauth_config::apply_stored_google_client_id(&state);
            let _ = client_log::write(app.handle(), "INFO Client started");
            app.manage(state);
            background::start(app.handle())?;
            // 若上次开启了 MCP 服务，启动即恢复监听（失败只记日志，不影响主流程）
            if let Err(error) = mcp::restart(app.handle()) {
                let _ = client_log::write(app.handle(), &format!("WARN MCP start failed: {error}"));
            }
            prewarm_settings_window(app.handle().clone());
            // 启动后补一轮正文预取，把历史邮件的正文补齐
            std::thread::sleep(std::time::Duration::from_secs(4));
            crate::mail::body::spawn_body_prefetch(app.handle(), None);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::Focused(true) = event {
                    let app = window.app_handle();
                    // 回到窗口即取消任务栏的「有新邮件」提醒
                    commands::system::clear_attention(app);
                    let days = client_log::retention_days(app.state::<AppState>().inner());
                    if let Err(error) = client_log::cleanup(app, days) {
                        eprintln!("OneMail log cleanup failed: {error}");
                    }
                    let _ = client_log::write(app, "INFO Client focused");
                }
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            background::tray_take_compose_request,
            system::network_activity_status,
            system::system_info,
            system::system_process_memory,
            system::system_database_size,
            system::system_set_title_bar_theme,
            system::system_get_theme,
            system::system_reveal_path,
            system::system_reveal_logs,
            system::system_open_database_directory,
            system::system_reload_client,
            system::system_open_devtools,
            system::system_open_external,
            system::system_take_notification_message,
            system::accounts_open_add_window,
            system::settings_open_window,
            system::original_message_open_window,
            system::accounts_close_add_window,
            data::accounts::accounts_list,
            data::accounts::accounts_discover_folders,
            data::accounts::accounts_create,
            data::accounts::accounts_update,
            data::accounts::accounts_reauthorize,
            data::accounts::accounts_remove,
            data::conversations::conversations_list,
            data::conversations::conversations_messages,
            data::conversations::conversations_find_message,
            data::conversations::conversations_search,
            data::correspondents::correspondents_list,
            data::correspondents::correspondents_thread,
            data::correspondents::correspondents_mark_read,
            data::correspondents::correspondents_group_counts,
            data::correspondents::correspondents_set_alias,
            data::messages::messages_stats,
            data::mcp::mcp_settings_get,
            data::mcp::mcp_settings_save,
            data::messages::messages_get,
            data::messages::messages_load_body,
            data::messages::messages_set_read_state,
            data::messages::messages_save_attachment,
            data::messages::messages_attachment_payload,
            data::messages::messages_refresh_attachments,
            data::messages::messages_download_attachment,
            data::messages::messages_save_attachment_as,
            data::messages::messages_set_junk_override,
            data::messages::messages_set_deleted,
            data::oauth_config::oauth_google_client_id,
            data::oauth_config::oauth_set_google_client_id,
            data::translate::translate_text,
            data::translate::translate_settings_get,
            data::translate::translate_settings_save,
            data::compose::compose_create_reply_draft,
            data::compose::compose_create_forward_draft,
            data::compose::compose_send,
            data::compose::compose_select_attachments,
            data::compose::compose_list_outbox,
            data::compose::compose_save_draft,
            data::compose::compose_delete_draft,
            data::compose::compose_retry,
            data::compose::compose_delete_outbox,
            data::sync::sync_start_all,
            data::sync::sync_start_account,
            data::settings::settings_get,
            data::settings::settings_update,
            data::settings::settings_get_backup_sync,
            data::settings::settings_update_backup_sync,
            data::settings::settings_test_backup_sync,
            data::settings::settings_upload_backup_sync,
            data::settings::settings_download_backup_sync,
            data::settings::settings_import_backup_from_remote,
            data::ai::ai_settings_get,
            data::ai::ai_settings_verify_and_save,
            data::ai::ai_settings_clear,
            data::ai::ai_chat,
            backup::settings_export_sql,
            backup::settings_import_sql,
            data::updates::updates_status,
            data::updates::updates_install
        ])
        .build(tauri::generate_context!())
        .expect("error while building OneMail")
        .run(|app, event| {
            match event {
                tauri::RunEvent::Resumed => background::wake(app),
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { has_visible_windows: false, .. } => {
                    background::show_main_window(app);
                }
                _ => {}
            }
        });
}
