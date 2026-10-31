//! Icons for the process and listing tables: an app bundle's Finder icon, an executable's or folder's
//! icon, or a system icon file. Each is rendered once to a small PNG by AppKit (through `osascript`)
//! and cached under the data folder. An icon is named by its key: a path (or a system icon file).

use std::{collections::HashMap, fs, path::PathBuf, process::Command};

use crate::sysops;

const SCRIPT: &str = include_str!("app_icon.js");
const CORE_TYPES: &str = "/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources";

/// The icon for a user account (its picture is not read).
pub const USER_ICON: &str =
    "/System/Library/CoreServices/CoreTypes.bundle/Contents/Resources/GuestUserIcon.icns";

fn cache_dir() -> PathBuf {
    sysops::data_dir().join("IconCache")
}

/// The cached PNG of an icon key: a 64-bit FNV-1a hash of the key, so any key maps to a short,
/// distinct file name.
pub fn cache_file(key: &str) -> PathBuf {
    let hash = key.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    cache_dir().join(format!("{hash:016x}.png"))
}

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

/// The icon of a path: its app bundle when it has one, else the file or folder itself.
pub fn path_key(path: &str) -> String {
    bundle_of(path).unwrap_or_else(|| path.to_string())
}

/// A well-known system process's icon file, for the processes that have no executable to show.
fn system_icon(name: &str) -> Option<String> {
    match name {
        "kernel_task" | "launchd" => Some(format!("{CORE_TYPES}/ToolbarAdvanced.icns")),
        _ => None,
    }
}

/// The icon key of a process: its own app bundle, else its owner app's bundle, else a well-known
/// system icon, else the executable file itself (macOS draws its generic icon).
pub fn process_key(
    name: &str,
    path: &str,
    owner: &str,
    owner_bundles: &HashMap<String, String>,
) -> Option<String> {
    bundle_of(path)
        .or_else(|| owner_bundles.get(owner).cloned())
        .or_else(|| system_icon(name))
        .or_else(|| path.starts_with('/').then(|| path.to_string()))
}

/// Icons already on disk for these keys (no rendering).
pub fn cached(keys: &[String]) -> HashMap<String, PathBuf> {
    keys.iter()
        .filter_map(|k| {
            let file = cache_file(k);
            file.exists().then(|| (k.clone(), file))
        })
        .collect()
}

/// Render the icons of these keys (one `osascript` run for all of them). Returns key -> PNG.
pub fn render(keys: &[String]) -> HashMap<String, PathBuf> {
    let mut out = cached(keys);
    let missing: Vec<&String> = keys.iter().filter(|k| !out.contains_key(*k)).collect();
    if missing.is_empty() {
        return out;
    }
    let dir = cache_dir();
    if fs::create_dir_all(&dir).is_err() {
        return out;
    }
    let script = dir.join("app_icon.js");
    if fs::read_to_string(&script).ok().as_deref() != Some(SCRIPT)
        && fs::write(&script, SCRIPT).is_err()
    {
        return out;
    }
    let args: Vec<String> = missing
        .iter()
        .flat_map(|k| [k.to_string(), cache_file(k).to_string_lossy().into_owned()])
        .collect();
    let result = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg(&script)
        .args(&args)
        .output();
    if let Ok(o) = result {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            if let Some((key, png)) = line.split_once('\t') {
                out.insert(key.to_string(), PathBuf::from(png));
            }
        }
    }
    out
}
