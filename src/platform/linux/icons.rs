//! Icons: Linux app bundles and their icon themes are not read yet, so every process falls back to
//! its executable (which has no icon) and the tables draw their generic glyphs.

use std::path::{Path, PathBuf};

/// Linux has no per-user icon file; an empty key draws no icon.
pub const USER_ICON: &str = "";

pub fn bundle_of(_path: &str) -> Option<String> {
    None
}

pub fn system_icon(_name: &str) -> Option<String> {
    None
}

pub fn render_icons(_dir: &Path, _jobs: &[(String, PathBuf)]) -> Vec<(String, PathBuf)> {
    Vec::new()
}
