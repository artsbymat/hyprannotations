pub mod palette;
use crate::canvas::Tool;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub default_tool: Tool,
    pub color: [u8; 4],
    pub stroke_width: f32,
    pub highlighter_opacity: f32,
    pub font_size: f32,
    pub toolbar_position: [f32; 2],
    pub shortcuts: BTreeMap<String, String>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            default_tool: Tool::Pen,
            color: [187, 255, 139, 255],
            stroke_width: 4.0,
            highlighter_opacity: 0.35,
            font_size: 28.0,
            toolbar_position: [20.0, 20.0],
            shortcuts: [
                ("p", "pen"),
                ("h", "highlighter"),
                ("e", "eraser"),
                ("l", "line"),
                ("a", "arrow"),
                ("r", "rectangle"),
                ("o", "ellipse"),
                ("t", "text"),
                ("q", "pointer"),
                ("v", "select"),
                ("ctrl+z", "undo"),
                ("ctrl+shift+z", "redo"),
                ("Tab", "desktop"),
                ("Escape", "exit"),
                ("Delete", "delete"),
            ]
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect(),
        }
    }
}
impl Settings {
    pub fn path() -> PathBuf {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            })
            .join("screen-annotation/config.toml")
    }
    pub fn load() -> Self {
        match std::fs::read_to_string(Self::path()) {
            Ok(text) => match toml::from_str::<Self>(&text) {
                Ok(mut settings) => {
                    for (key, action) in Self::default().shortcuts {
                        settings.shortcuts.entry(key).or_insert(action);
                    }
                    if !settings.stroke_width.is_finite() {
                        settings.stroke_width = 4.0;
                    }
                    settings.stroke_width = settings.stroke_width.clamp(1.0, 80.0);
                    if !settings.font_size.is_finite() {
                        settings.font_size = 28.0;
                    }
                    settings.font_size = settings.font_size.clamp(8.0, 200.0);
                    if !settings.highlighter_opacity.is_finite() {
                        settings.highlighter_opacity = 0.35;
                    }
                    settings.highlighter_opacity = settings.highlighter_opacity.clamp(0.0, 1.0);
                    if settings.toolbar_position.iter().any(|v| !v.is_finite()) {
                        settings.toolbar_position = [20.0, 20.0];
                    }
                    settings
                }
                Err(error) => {
                    eprintln!("Invalid config, using defaults: {error}");
                    Self::default()
                }
            },
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("Cannot read config: {error}");
                }
                Self::default()
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let settings = Settings::default();
        let text = toml::to_string(&settings).unwrap();
        let decoded: Settings = toml::from_str(&text).unwrap();
        assert_eq!(decoded.color, settings.color);
        assert_eq!(decoded.shortcuts, settings.shortcuts);
    }
}
