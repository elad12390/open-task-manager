//! AppKit icons: an app bundle's Finder icon, rendered once by `osascript` (see `app_icon.js`).

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const SCRIPT: &str = include_str!("app_icon.js");
const CORE_TYPES: &str = "/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources";

/// The icon for a user account (its picture is not read).
pub const USER_ICON: &str =
    "/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/GuestUserIcon.icns";

/// The app bundle a program belongs to: the outermost `….app`, or for bundles with another
/// extension (e.g. `Steam.AppBundle`) the folder that holds `Contents/MacOS`.
pub fn bundle_of(path: &str) -> Option<String> {
    if let Some(i) = path.find(".app/") {
        return Some(path[..i + 4].to_string());
    }
    if path.ends_with(".app") {
        return Some(path.to_string());
    }
    // App-like bundles with another extension (Steam.AppBundle); not system plug-ins and services,
    // whose generic icons would only add noise.
    path.find("/Contents/MacOS/")
        .map(|i| path[..i].to_string())
        .filter(|b| {
            ![
                ".xpc",
                ".appex",
                ".bundle",
                ".framework",
                ".plugin",
                ".kext",
            ]
            .iter()
            .any(|ext| b.ends_with(ext))
        })
}

/// A well-known system process's icon file, for the processes that have no executable to show.
pub fn system_icon(name: &str) -> Option<String> {
    match name {
        "kernel_task" | "launchd" => Some(format!("{CORE_TYPES}/ToolbarAdvanced.icns")),
        _ => None,
    }
}

/// Render the icons of `(key, png)` jobs with one `osascript` run. The script is kept in `dir`.
/// Returns the `(key, png)` pairs that were written.
pub fn render_icons(dir: &Path, jobs: &[(String, PathBuf)]) -> Vec<(String, PathBuf)> {
    let script = dir.join("app_icon.js");
    if fs::read_to_string(&script).ok().as_deref() != Some(SCRIPT)
        && fs::write(&script, SCRIPT).is_err()
    {
        return Vec::new();
    }
    let args: Vec<String> = jobs
        .iter()
        .flat_map(|(k, png)| [k.clone(), png.to_string_lossy().into_owned()])
        .collect();
    let Ok(o) = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg(&script)
        .args(&args)
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter_map(|line| {
            let (key, png) = line.split_once('\t')?;
            Some((key.to_string(), PathBuf::from(png)))
        })
        .collect()
}
