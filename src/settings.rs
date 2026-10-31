use std::fs;

use serde::{Deserialize, Serialize};

use crate::sysops;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Glass {
    Clear,
    Tinted,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub appearance: Appearance,
    pub glass: Glass,
    pub refresh_secs: u64,
    pub mark_self: bool,
    pub text_scale: u32,
    pub high_contrast: bool,
    pub popout_skin: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            appearance: Appearance::System,
            glass: Glass::Clear,
            refresh_secs: 1,
            mark_self: true,
            text_scale: 100,
            high_contrast: false,
            popout_skin: GLASS_SKIN.into(),
        }
    }
}

impl Settings {
    fn path() -> std::path::PathBuf {
        sysops::data_dir().join("native-settings.json")
    }

    pub fn load() -> Self {
        fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(sysops::data_dir());
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(Self::path(), json);
        }
    }
}

pub const GLASS_SKIN: &str = "Glass";

/// A popout skin: the same JSON files the earlier Swift popout read from `Skins/`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    pub name: String,
    pub background: String,
    pub ink: String,
    pub accent: String,
    #[serde(default = "default_radius")]
    pub corner_radius: f32,
}

fn default_radius() -> f32 {
    18.
}

impl Skin {
    pub fn color(hex: &str, alpha: u8) -> u32 {
        (u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0) << 8) | alpha as u32
    }
}

pub fn skins() -> Vec<Skin> {
    let mut skins: Vec<Skin> = fs::read_dir(sysops::data_dir().join("Skins"))
        .map(|r| {
            r.flatten()
                .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("json"))
                .filter_map(|e| serde_json::from_slice(&fs::read(e.path()).ok()?).ok())
                .collect()
        })
        .unwrap_or_default();
    skins.sort_by(|a, b| a.name.cmp(&b.name));
    skins
}
