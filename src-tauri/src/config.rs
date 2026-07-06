use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Appearance {
    pub font_family: String,
    pub font_size: u32,
    pub text_color: String,
    pub bg_opacity: f32,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            font_family: "Microsoft YaHei".to_string(),
            font_size: 24,
            text_color: "#FFFFFF".to_string(),
            bg_opacity: 0.5,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AsrProvider {
    Stepfun,
    Volc,
}

impl Default for AsrProvider {
    fn default() -> Self {
        Self::Stepfun
    }
}

pub const DEFAULT_VOLC_URL: &str =
    "wss://openspeech.bytedance.com/api/v3/plan/sauc/bigmodel_async";
pub const DEFAULT_VOLC_RESOURCE_ID: &str = "volc.seedasr.sauc.duration";
pub const DEFAULT_VOLC_MODEL: &str = "bigmodel";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub provider: AsrProvider,
    pub api_key: String,
    pub appearance: Appearance,
    pub window: Option<WindowRect>,
    pub volc_api_key: String,
    pub volc_resource_id: String,
    pub volc_url: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: AsrProvider::default(),
            api_key: String::new(),
            appearance: Appearance::default(),
            window: None,
            volc_api_key: String::new(),
            volc_resource_id: DEFAULT_VOLC_RESOURCE_ID.into(),
            volc_url: DEFAULT_VOLC_URL.into(),
        }
    }
}

pub fn default_window_rect() -> WindowRect {
    WindowRect {
        x: 240,
        y: 60,
        width: 800,
        height: 120,
    }
}
