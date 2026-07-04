pub mod asr;
pub mod audio;
pub mod commands;
pub mod config;
pub mod db;
pub mod events;
pub mod logging;
pub mod state;
pub mod vad;

use config::{AppConfig, Appearance, WindowRect};
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

            let monitor_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                device_monitor(monitor_handle).await;
            });

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
    let api_key = match get_config_value(pool, "api_key").await? {
        Some(s) => serde_json::from_str::<String>(&s).unwrap_or_default(),
        None => String::new(),
    };
    let appearance = match get_config_value(pool, "appearance").await? {
        Some(s) => serde_json::from_str::<Appearance>(&s).unwrap_or_default(),
        None => Appearance::default(),
    };
    let window = match get_config_value(pool, "window").await? {
        Some(s) => serde_json::from_str::<WindowRect>(&s).ok(),
        None => None,
    };
    Ok(AppConfig {
        api_key,
        appearance,
        window,
    })
}

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
