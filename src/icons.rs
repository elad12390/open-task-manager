//! Icons for the process and listing tables: an app bundle's icon, an executable's or folder's icon,
//! or a system icon file. Each is rendered once to a small PNG by the platform and cached under the
//! data folder. An icon is named by its key: a path (or a system icon file).

use std::{collections::HashMap, fs, path::PathBuf};

use crate::{platform::icons, sysops};

/// The icon for a user account (its picture is not read).
pub const USER_ICON: &str = icons::USER_ICON;

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

/// The icon of a path: its app bundle when it has one, else the file or folder itself.
pub fn path_key(path: &str) -> String {
    icons::bundle_of(path).unwrap_or_else(|| path.to_string())
}

/// The icon key of a process: its own app bundle, else its owner app's bundle, else a well-known
/// system icon, else the executable file itself (the platform draws its generic icon).
pub fn process_key(
    name: &str,
    path: &str,
    owner: &str,
    owner_bundles: &HashMap<String, String>,
) -> Option<String> {
    icons::bundle_of(path)
        .or_else(|| owner_bundles.get(owner).cloned())
        .or_else(|| icons::system_icon(name))
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

/// Render the icons of these keys that are not cached yet. Returns key -> PNG.
pub fn render(keys: &[String]) -> HashMap<String, PathBuf> {
    let mut out = cached(keys);
    let jobs: Vec<(String, PathBuf)> = keys
        .iter()
        .filter(|k| !out.contains_key(*k))
        .map(|k| (k.clone(), cache_file(k)))
        .collect();
    if jobs.is_empty() {
        return out;
    }
    let dir = cache_dir();
    if fs::create_dir_all(&dir).is_err() {
        return out;
    }
    out.extend(icons::render_icons(&dir, &jobs));
    out
}
