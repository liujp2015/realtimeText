use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleUpdate {
    pub state: String,        // "partial" | "final"
    pub text: String,
    pub start_ts: i64,        // ms
    pub end_ts: Option<i64>,  // present on final
    pub paralinguistic: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_guid: String,
    pub started_at: i64,       // seconds
    pub device_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrStatus {
    pub connected: bool,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

pub fn emit_subtitle_update(app: &AppHandle, upd: SubtitleUpdate) {
    // Emit to subtitle window specifically (fallback to broadcast if window missing).
    if let Some(win) = app.get_webview_window("subtitle") {
        if let Err(e) = win.emit("subtitle-update", &upd) {
            log::warn!("emit subtitle-update to window: {e}");
        }
    } else if let Err(e) = app.emit("subtitle-update", &upd) {
        log::warn!("emit subtitle-update broadcast: {e}");
    }
}

pub fn emit_session_meta(app: &AppHandle, meta: SessionMeta) {
    if let Err(e) = app.emit("session-meta", &meta) {
        log::warn!("emit session-meta: {e}");
    }
}

pub fn emit_asr_status(app: &AppHandle, connected: bool, retry_count: u32, last_error: Option<&str>) {
    let s = AsrStatus {
        connected,
        retry_count,
        last_error: last_error.map(|s| s.to_string()),
    };
    if let Err(e) = app.emit("asr-status", &s) {
        log::warn!("emit asr-status: {e}");
    }
}
