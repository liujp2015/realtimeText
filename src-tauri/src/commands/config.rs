use serde_json::Value;
use tauri::State;

use crate::config::{Appearance, WindowRect};
use crate::db::repository::{get_config_value, set_config_value};
use crate::state::AppState;

#[tauri::command]
pub async fn config_get(key: String, state: State<'_, AppState>) -> Result<Option<Value>, String> {
    let v = get_config_value(&state.pool, &key)
        .await
        .map_err(|e| e.to_string())?;
    Ok(v.and_then(|s| serde_json::from_str(&s).ok()))
}

#[tauri::command]
pub async fn config_set(key: String, value: Value, state: State<'_, AppState>) -> Result<(), String> {
    if key == "appearance" {
        if let Some(obj) = value.as_object() {
            if let Some(fs) = obj.get("font_size").and_then(|v| v.as_u64()) {
                if !(12..=72).contains(&fs) {
                    return Err("font_size must be in [12, 72]".into());
                }
            }
            if let Some(op) = obj.get("bg_opacity").and_then(|v| v.as_f64()) {
                if !(0.0..=1.0).contains(&op) {
                    return Err("bg_opacity must be in [0.0, 1.0]".into());
                }
            }
        }
    }
    if key == "api_key" {
        if let Some(s) = value.as_str() {
            if s.is_empty() {
                // allow clearing; session_start will re-validate
            }
        }
    }
    let s = serde_json::to_string(&value).map_err(|e| e.to_string())?;
    set_config_value(&state.pool, &key, &s)
        .await
        .map_err(|e| e.to_string())?;

    // Mirror to in-memory config for hot access
    {
        let mut cfg = state.config.lock();
        match key.as_str() {
            "api_key" => {
                if let Some(s) = value.as_str() {
                    cfg.api_key = s.to_string();
                }
            }
            "appearance" => {
                if let Ok(a) = serde_json::from_value::<Appearance>(value.clone()) {
                    cfg.appearance = a;
                }
            }
            "window" => {
                if let Ok(w) = serde_json::from_value::<WindowRect>(value.clone()) {
                    cfg.window = Some(w);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn config_reset_appearance(state: State<'_, AppState>) -> Result<Appearance, String> {
    let default = Appearance::default();
    let v = serde_json::to_value(&default).map_err(|e| e.to_string())?;
    let s = serde_json::to_string(&v).map_err(|e| e.to_string())?;
    set_config_value(&state.pool, "appearance", &s)
        .await
        .map_err(|e| e.to_string())?;
    {
        let mut cfg = state.config.lock();
        cfg.appearance = default.clone();
    }
    Ok(default)
}
