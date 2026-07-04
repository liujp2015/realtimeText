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

#[derive(Debug, Clone, Default)]
pub struct AppConfig {
    pub api_key: String,
    pub appearance: Appearance,
    pub window: Option<WindowRect>,
}

pub fn default_window_rect() -> WindowRect {
    WindowRect {
        x: 240,
        y: 60,
        width: 800,
        height: 120,
    }
}
