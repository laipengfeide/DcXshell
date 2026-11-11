#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audit;
mod auth;
mod commands;
mod crypto;
mod db;
mod lan;
mod models;
mod net;
mod sftp;
mod ssh_client;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let conn = db::open(&dir.join("dcxshell.db")).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::Other, format!("初始化数据库失败: {e}"))
            })?;
            let secrets = crypto::SecretBox::load_or_create(&dir.join("secret.key")).map_err(
                |e| std::io::Error::new(std::io::ErrorKind::Other, format!("初始化密钥失败: {e}")),
            )?;
            let device_id = db::ensure_device_id(&conn);
            let lan = lan::LanState::start(
                app.handle().clone(),
                device_id,
                net::local_ipv4(),
                String::new(),
            );
            app.manage(lan.clone());
            app.manage(commands::AppState::new(
                conn,
                secrets,
                app.handle().clone(),
                lan,
            ));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::auth_status,
            commands::register,
            commands::login,
            commands::logout,
            commands::guest_enter,
            commands::guest_leave,
            commands::current_ip,
            commands::change_password,
            commands::list_audit_logs,
            commands::log_command,
            commands::list_connections,
            commands::save_connection,
            commands::delete_connection,
            commands::test_connection,
            commands::open_session,
            commands::write_session,
            commands::resize_session,
            commands::close_session,
            commands::ping_session,
            commands::active_session_count,
            commands::notify_ready,
            commands::lan_status,
            commands::lan_peers,
            commands::lan_send_file,
            commands::lan_respond,
            commands::open_path,
            sftp::sftp_list,
            sftp::sftp_upload,
            sftp::sftp_download,
            sftp::sftp_remove,
            sftp::sftp_rename,
            sftp::sftp_copy,
            sftp::sftp_extract,
        ])
        .run(tauri::generate_context!())
        .expect("Dc Xshell 启动失败");
}
