use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager, State};

use crate::audio::start_audio_source;
use crate::audio::ring;
use crate::asr::pipeline;
use crate::db::repository::{
    clear_history, delete_session, finalize_session, get_session_with_transcriptions,
    insert_session, list_sessions, SessionListItem, SessionRow, TranscriptionRow,
};
use crate::events::{emit_session_meta, SessionMeta};
use crate::state::{AppState, RunningHandle};

#[derive(Debug, Serialize)]
pub struct SessionStartInfo {
    pub session_guid: String,
    pub started_at: i64,
    pub device_name: String,
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[tauri::command]
pub async fn session_start(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<SessionStartInfo, String> {
    {
        let running = state.running.lock();
        if running.is_some() {
            return Err("AlreadyRunning".into());
        }
    }

    let asr_cfg = {
        let cfg = state.config.lock();
        match cfg.provider {
            crate::config::AsrProvider::Stepfun => {
                if cfg.api_key.is_empty() {
                    return Err("ApiKeyMissing".into());
                }
                crate::asr::provider::AsrConfig::Stepfun {
                    api_key: cfg.api_key.clone(),
                }
            }
            crate::config::AsrProvider::Volc => {
                if cfg.volc_api_key.is_empty() {
                    return Err("VolcApiKeyMissing".into());
                }
                crate::asr::provider::AsrConfig::Volc {
                    api_key: cfg.volc_api_key.clone(),
                    resource_id: cfg.volc_resource_id.clone(),
                    url: cfg.volc_url.clone(),
                }
            }
        }
    };

    let (producer, consumer) = ring::new();
    let (capture_thread, capture_stop, info_rx) = start_audio_source(producer);
    let capture_info = match info_rx.recv() {
        Ok(Ok(info)) => info,
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err("capture thread exited before sending info".into()),
    };
    let device_name = capture_info.device_name.clone();

    let guid = uuid::Uuid::new_v4().to_string();
    let started_at = now_secs();
    insert_session(&state.pool, &guid, started_at, &device_name)
        .await
        .map_err(|e| e.to_string())?;

    let pipeline_handle = pipeline::spawn(
        app.clone(),
        consumer,
        capture_info.sample_rate as usize,
        asr_cfg,
        guid.clone(),
    )
    .map_err(|e| e.to_string())?;

    {
        let mut running = state.running.lock();
        *running = Some(RunningHandle {
            session_guid: guid.clone(),
            stop_tx: pipeline_handle.stop_tx,
            pipeline_tasks: pipeline_handle.tasks,
            capture_stop,
            capture_thread: Some(capture_thread),
        });
    }

    emit_session_meta(
        &app,
        SessionMeta {
            session_guid: guid.clone(),
            started_at,
            device_name: device_name.clone(),
        },
    );

    #[cfg(not(target_os = "android"))]
    {
        if let Some(win) = app.get_webview_window("subtitle") {
            let _ = win.show();
            // Dev: keep cursor events on so DevTools (right-click) works.
            // Production: set_ignore_cursor_events(true).
            let _ = win.set_ignore_cursor_events(false);
            if cfg!(debug_assertions) {
                let _ = win.open_devtools();
            }
        }
    }

    Ok(SessionStartInfo {
        session_guid: guid,
        started_at,
        device_name,
    })
}

#[tauri::command]
pub async fn session_stop(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let handle = {
        let mut running = state.running.lock();
        running.take()
    };
    if let Some(h) = handle {
        let _ = h.stop_tx.send(()).await;
        // Wait for pipeline tasks to finish in-flight SSE submissions (5s timeout).
        for task in h.pipeline_tasks {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), task).await;
        }
        let _ = h.capture_stop.send(());
        if let Some(t) = h.capture_thread {
            let _ = t.join();
        }
        finalize_session(&state.pool, &h.session_guid, now_secs())
            .await
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "android"))]
    {
        if let Some(win) = app.get_webview_window("subtitle") {
            let _ = win.hide();
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn session_list(
    limit: Option<i64>,
    offset: Option<i64>,
    state: State<'_, AppState>,
) -> Result<(i64, Vec<SessionListItem>), String> {
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);
    list_sessions(&state.pool, limit, offset)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn session_get(
    guid: String,
    state: State<'_, AppState>,
) -> Result<(Option<SessionRow>, Vec<TranscriptionRow>), String> {
    get_session_with_transcriptions(&state.pool, &guid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn session_delete(guid: String, state: State<'_, AppState>) -> Result<(), String> {
    delete_session(&state.pool, &guid)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn history_clear(state: State<'_, AppState>) -> Result<(), String> {
    clear_history(&state.pool)
        .await
        .map_err(|e| e.to_string())
}
