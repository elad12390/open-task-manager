//! Startup items: XDG autostart entries and your systemd user units. Your own entries are switched
//! (an autostart entry gets `Hidden=true`; a unit is enabled or disabled); system-wide ones are
//! listed read-only.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use super::host;
use crate::sysops::{StartupItem, home};

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
}

fn user_autostart() -> PathBuf {
    config_home().join("autostart")
}

fn user_units() -> PathBuf {
    config_home().join("systemd/user")
}

/// The value of `key` in a desktop entry (the first match).
fn entry_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.strip_prefix(key)?.strip_prefix('='))
        .map(str::trim)
}

/// An autostart entry runs at login unless it says it is hidden or disabled for GNOME.
pub fn entry_loaded(text: &str) -> bool {
    entry_value(text, "Hidden") != Some("true")
        && entry_value(text, "X-GNOME-Autostart-enabled") != Some("false")
}

/// The first word of `Exec=`, when it is an absolute path.
fn entry_program(text: &str) -> Option<PathBuf> {
    let first = entry_value(text, "Exec")?.split_whitespace().next()?;
    first.starts_with('/').then(|| PathBuf::from(first))
}

/// `text` with the entry hidden (or shown again), leaving every other line alone.
pub fn set_hidden(text: &str, hidden: bool) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.starts_with("Hidden=") {
            if hidden {
                lines.push("Hidden=true".into());
            }
        } else if line.starts_with("X-GNOME-Autostart-enabled=") {
            lines.push(format!("X-GNOME-Autostart-enabled={}", !hidden));
        } else {
            lines.push(line.to_string());
        }
    }
    if hidden && !text.lines().any(|l| l.starts_with("Hidden=")) {
        lines.push("Hidden=true".into());
    }
    lines.join("\n") + "\n"
}

fn desktop_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("desktop"))
        .collect()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn unit_file(name: &str) -> Option<PathBuf> {
    [
        user_units(),
        PathBuf::from("/etc/systemd/user"),
        PathBuf::from("/usr/lib/systemd/user"),
    ]
    .into_iter()
    .map(|dir| dir.join(name))
    .find(|p| p.exists())
}

fn unit_items() -> Vec<StartupItem> {
    let Ok(out) = host(
        "systemctl",
        &[
            "--user",
            "list-unit-files",
            "--type=service",
            "--no-legend",
            "--plain",
        ],
    ) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next()?;
            let state = fields.next()?;
            if state != "enabled" && state != "disabled" {
                return None;
            }
            let file = unit_file(name).unwrap_or_else(|| PathBuf::from(name));
            let editable = file.parent() == Some(user_units().as_path());
            let program = fs::read_to_string(&file).ok().and_then(|text| {
                let exec = text.lines().find_map(|l| l.strip_prefix("ExecStart="))?;
                let first = exec
                    .trim_start_matches(['-', '@', '+', '!'])
                    .split_whitespace()
                    .next()?;
                first.starts_with('/').then(|| PathBuf::from(first))
            });
            Some(StartupItem {
                label: name.to_string(),
                scope: "User unit",
                file,
                loaded: state == "enabled",
                editable,
                program,
            })
        })
        .collect()
}

pub fn startup_items() -> Vec<StartupItem> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for (dir, scope, editable) in [
        (user_autostart(), "Autostart", true),
        (
            PathBuf::from("/etc/xdg/autostart"),
            "System autostart",
            false,
        ),
    ] {
        for file in desktop_files(&dir) {
            let label = stem(&file);
            if !seen.insert(label.clone()) {
                continue;
            }
            let text = fs::read_to_string(&file).unwrap_or_default();
            items.push(StartupItem {
                loaded: entry_loaded(&text),
                program: entry_program(&text),
                label,
                scope,
                file,
                editable,
            });
        }
    }
    items.extend(unit_items());
    items.sort_by(|a, b| {
        a.scope
            .cmp(b.scope)
            .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
    items
}

/// Enable or disable one of your own startup items: an autostart entry is hidden or shown, a user
/// unit is enabled or disabled with `systemctl --user`. System items are refused.
pub fn toggle_startup(file: &Path, enable: bool) -> Result<(), String> {
    if file.to_string_lossy().contains("..") {
        return Err("Only your own startup items can be changed".into());
    }
    if !file.exists() {
        return Err("Startup item not found".into());
    }
    match file.extension().and_then(|x| x.to_str()) {
        Some("desktop") if file.parent() == Some(user_autostart().as_path()) => {
            let text = fs::read_to_string(file).map_err(|e| e.to_string())?;
            fs::write(file, set_hidden(&text, !enable)).map_err(|e| e.to_string())
        }
        Some("service") if file.parent() == Some(user_units().as_path()) => {
            let name = file.file_name().unwrap_or_default().to_string_lossy();
            let verb = if enable { "enable" } else { "disable" };
            host("systemctl", &["--user", verb, &name]).map(|_| ())
        }
        _ => Err("Only your own startup items can be changed".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENTRY: &str =
        "[Desktop Entry]\nType=Application\nName=Syncthing\nExec=/usr/bin/syncthing --no-browser\n";

    #[test]
    fn entries_run_unless_hidden_or_gnome_disabled() {
        assert!(entry_loaded(ENTRY));
        assert!(!entry_loaded(&format!("{ENTRY}Hidden=true\n")));
        assert!(!entry_loaded(&format!(
            "{ENTRY}X-GNOME-Autostart-enabled=false\n"
        )));
    }

    #[test]
    fn exec_gives_the_program_when_absolute() {
        assert_eq!(
            entry_program(ENTRY),
            Some(PathBuf::from("/usr/bin/syncthing"))
        );
        assert_eq!(entry_program("[Desktop Entry]\nExec=code %F\n"), None);
    }

    #[test]
    fn hiding_adds_or_rewrites_the_hidden_line() {
        let hidden = set_hidden(ENTRY, true);
        assert!(hidden.ends_with("Hidden=true\n"));
        assert!(!entry_loaded(&hidden));
        let shown = set_hidden(&hidden, false);
        assert!(entry_loaded(&shown));
        assert!(!shown.contains("Hidden"));
        assert_eq!(
            set_hidden(&set_hidden(ENTRY, true), true)
                .matches("Hidden=")
                .count(),
            1
        );
    }

    #[test]
    fn gnome_flag_follows_the_switch() {
        let text = "[Desktop Entry]\nX-GNOME-Autostart-enabled=true\nExec=/bin/x\n";
        let off = set_hidden(text, true);
        assert!(off.contains("X-GNOME-Autostart-enabled=false"));
        assert!(!entry_loaded(&off));
        assert!(entry_loaded(&set_hidden(&off, false)));
    }
}
