pub mod asr;
pub mod audio;
pub mod commands;
pub mod config;
pub mod db;
pub mod events;
pub mod logging;
pub mod state;
pub mod vad;

use config::{AppConfig, Appearance, AsrProvider, WindowRect};
use db::repository::get_config_value;
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .max_file_size(50000)
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                let pool = match db::pool::init(&handle).await {
                    Ok(p) => p,
                    Err(e) => {
                        log::error!("db init failed: {e}");
                        panic!("db init: {e}");
                    }
                };
                let config = load_config(&pool).await.unwrap_or_default();
                let state = AppState::new(pool, config);
                handle.manage(state);
            });

            #[cfg(not(target_os = "android"))]
            {
                let monitor_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    device_monitor(monitor_handle).await;
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::config::config_get,
            commands::config::config_set,
            commands::config::config_reset_appearance,
            commands::session::session_start,
            commands::session::session_stop,
            commands::session::session_list,
            commands::session::session_get,
            commands::session::session_delete,
            commands::session::history_clear,
            commands::search::search_keywords,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

async fn load_config(pool: &sqlx::SqlitePool) -> anyhow::Result<AppConfig> {
    let mut cfg = AppConfig::default();
    if let Some(s) = get_config_value(pool, "api_key").await? {
        cfg.api_key = serde_json::from_str::<String>(&s).unwrap_or_default();
    }
    if let Some(s) = get_config_value(pool, "appearance").await? {
        cfg.appearance = serde_json::from_str::<Appearance>(&s).unwrap_or_default();
    }
    if let Some(s) = get_config_value(pool, "window").await? {
        if let Ok(w) = serde_json::from_str::<WindowRect>(&s) {
            cfg.window = Some(w);
        }
    }
    if let Some(s) = get_config_value(pool, "provider").await? {
        if let Ok(p) = serde_json::from_str::<AsrProvider>(&s) {
            cfg.provider = p;
        }
    }
    if let Some(s) = get_config_value(pool, "volc_api_key").await? {
        cfg.volc_api_key = serde_json::from_str::<String>(&s).unwrap_or_default();
    }
    if let Some(s) = get_config_value(pool, "volc_resource_id").await? {
        if let Ok(v) = serde_json::from_str::<String>(&s) {
            cfg.volc_resource_id = v;
        }
    }
    if let Some(s) = get_config_value(pool, "volc_url").await? {
        if let Ok(v) = serde_json::from_str::<String>(&s) {
            cfg.volc_url = v;
        }
    }
    Ok(cfg)
}

#[cfg(not(target_os = "android"))]
async fn device_monitor(app: tauri::AppHandle) {
    use crate::audio::capture::current_default_output_name;
    use std::time::Duration;
    let mut last = current_default_output_name();
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        let cur = current_default_output_name();
        if cur != last {
            log::info!("default output device changed: {:?} -> {:?}", last, cur);
            last = cur;
        }
        let _ = &app;
    }
}
