//! App icons for the process list: each app bundle's Finder icon, rendered once to a small PNG by
//! AppKit (through `osascript`) and cached under the data folder.

use std::{collections::HashMap, fs, path::PathBuf, process::Command};

use crate::sysops;

const SCRIPT: &str = include_str!("app_icon.js");

fn cache_dir() -> PathBuf {
    sysops::data_dir().join("IconCache")
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

/// Icons already on disk for these bundles (no rendering).
pub fn cached(bundles: &[String]) -> HashMap<String, PathBuf> {
    bundles
        .iter()
        .filter_map(|b| {
            let file = cache_dir().join(format!(
                "{}.png",
                b.replace(|c: char| !c.is_ascii_alphanumeric(), "_")
            ));
            file.exists().then(|| (b.clone(), file))
        })
        .collect()
}

/// Render the icons of these bundles (one `osascript` run for all of them). Returns bundle -> PNG.
pub fn render(bundles: &[String]) -> HashMap<String, PathBuf> {
    let mut out = cached(bundles);
    let missing: Vec<&String> = bundles.iter().filter(|b| !out.contains_key(*b)).collect();
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
    let result = Command::new("osascript")
        .arg("-l")
        .arg("JavaScript")
        .arg(&script)
        .arg(&dir)
        .args(missing.iter().map(|b| b.as_str()))
        .output();
    if let Ok(o) = result {
        for line in String::from_utf8_lossy(&o.stdout).lines() {
            if let Some((bundle, png)) = line.split_once('\t') {
                out.insert(bundle.to_string(), PathBuf::from(png));
            }
        }
    }
    out
}
