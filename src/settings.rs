use crate::theme::Appearance;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Write,
    Split,
    Read,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Appearance,
    pub layout: Layout,
    pub sidebar: bool,
    pub status_bar: bool,
    pub line_numbers: bool,
    pub wrap: bool,
    pub font_size: f32,
    pub workspace: Option<PathBuf>,
    pub open_files: Vec<PathBuf>,
    pub window: Option<[f32; 4]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Appearance::Dark,
            layout: Layout::Write,
            sidebar: false,
            status_bar: true,
            line_numbers: true,
            wrap: true,
            font_size: 14.5,
            workspace: None,
            open_files: Vec::new(),
            window: None,
        }
    }
}

fn settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "Glyph", "Glyph").map(|dirs| dirs.config_dir().join("settings.json"))
}

impl Settings {
    pub fn load() -> Self {
        settings_path()
            .and_then(|path| fs::read_to_string(path).ok())
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}
