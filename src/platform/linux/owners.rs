//! Which app or unit a Linux process belongs to, read from its systemd cgroup (`/proc/<pid>/cgroup`):
//! Flatpak and Snap apps, GNOME and KDE app scopes, user and system services, and the `Name` of the
//! app's desktop entry when one is installed.

use std::{collections::HashMap, path::PathBuf, sync::Mutex};

use super::host;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    /// Started by systemd as a system service (system.slice) or the init scope (pid 1).
    System,
    /// A service of your own user manager (user@<uid>.service).
    UserService,
    /// Anything else: a login session, a terminal, or a command you started yourself.
    Session,
}

fn segments(cgroup: &str) -> impl Iterator<Item = &str> {
    cgroup
        .lines()
        .filter_map(|line| line.splitn(3, ':').nth(2))
        .flat_map(|path| path.split('/'))
}

/// Drops the instance number systemd appends to scope names (`firefox-1234` → `firefox`).
fn without_instance(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((head, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => name,
    }
}

/// The app id of a cgroup segment: `app-flatpak-org.mozilla.firefox-1234.scope` → `org.mozilla.firefox`,
/// `app-gnome-firefox-1234.scope` → `firefox`, `app-org.gnome.Terminal.slice` → `org.gnome.Terminal`,
/// `snap.firefox.firefox-1234.scope` → `firefox`.
pub fn cgroup_app(cgroup: &str) -> Option<String> {
    segments(cgroup).find_map(|seg| {
        if let Some(rest) = seg.strip_prefix("app-flatpak-") {
            let id = rest.strip_suffix(".scope")?;
            return Some(without_instance(id).to_string());
        }
        if let Some(rest) = seg
            .strip_prefix("snap.")
            .filter(|_| seg.ends_with(".scope"))
        {
            return rest.split('.').next().map(str::to_string);
        }
        let rest = seg.strip_prefix("app-")?;
        if let Some(scope) = rest.strip_suffix(".scope") {
            let id = ["gnome-", "kde-", "xfce-"]
                .iter()
                .find_map(|p| scope.strip_prefix(p))
                .map_or(scope, without_instance);
            return Some(id.to_string());
        }
        rest.strip_suffix(".slice").map(str::to_string)
    })
}

pub fn unit_kind(cgroup: &str) -> Unit {
    let segs: Vec<&str> = segments(cgroup).collect();
    if segs
        .iter()
        .any(|s| *s == "system.slice" || *s == "init.scope")
    {
        Unit::System
    } else if segs.iter().any(|s| s.starts_with("user@"))
        && segs.last().is_some_and(|s| s.ends_with(".service"))
    {
        Unit::UserService
    } else {
        Unit::Session
    }
}

/// The display name of an app: its desktop entry's `Name`, else the app id itself.
pub fn owner_name(cgroup: &str, desktop: impl Fn(&str) -> Option<String>) -> Option<String> {
    let id = cgroup_app(cgroup)?;
    Some(desktop(&id).unwrap_or(id))
}

/// Desktop-session programs (the shell, compositor, audio and portal services). They belong to the
/// operating system's desktop, not to any app you started.
const DESKTOP_SESSION: &[&str] = &[
    "gnome-shell",
    "gnome-session",
    "gnome-keyring",
    "gsd-",
    "kwin",
    "plasmashell",
    "Xwayland",
    "Xorg",
    "mutter",
    "sway",
    "Hyprland",
    "pipewire",
    "wireplumber",
    "pulseaudio",
    "gvfs",
    "xdg-desktop-portal",
    "at-spi",
    "ibus",
    "xdg-permission-store",
];

pub fn is_desktop_session(name: &str) -> bool {
    DESKTOP_SESSION.iter().any(|p| name.starts_with(p))
}

fn desktop_dirs() -> Vec<PathBuf> {
    let home = crate::sysops::home();
    let data = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    vec![
        data.join("applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ]
}

/// The unlocalized `Name=` of a desktop entry.
pub fn desktop_entry_name(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.strip_prefix("Name=").map(|n| n.trim().to_string()))
        .filter(|n| !n.is_empty())
}

static NAMES: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

/// The `Name` of the desktop entry `<id>.desktop`, read once per id.
pub fn desktop_name(id: &str) -> Option<String> {
    let mut cache = NAMES.lock().ok()?;
    let names = cache.get_or_insert_with(HashMap::new);
    if let Some(name) = names.get(id) {
        return name.clone();
    }
    let name = desktop_dirs().iter().find_map(|dir| {
        let text = host::read(dir.join(format!("{id}.desktop"))).ok()?;
        desktop_entry_name(&text)
    });
    names.insert(id.to_string(), name.clone());
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cg(path: &str) -> String {
        format!("0::{path}\n")
    }

    #[test]
    fn gnome_and_kde_app_scopes_name_the_app() {
        let firefox = cg(
            "/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-firefox-1234.scope",
        );
        assert_eq!(cgroup_app(&firefox).as_deref(), Some("firefox"));
        let kde =
            cg("/user.slice/user-1000.slice/user@1000.service/app.slice/app-kde-konsole-88.scope");
        assert_eq!(cgroup_app(&kde).as_deref(), Some("konsole"));
    }

    #[test]
    fn app_slices_keep_their_desktop_id() {
        let terminal = cg(
            "/user.slice/user-1000.slice/user@1000.service/app.slice/app-org.gnome.Terminal.slice/vte-spawn-1.scope",
        );
        assert_eq!(cgroup_app(&terminal).as_deref(), Some("org.gnome.Terminal"));
    }

    #[test]
    fn flatpak_and_snap_apps_are_named_by_their_id() {
        let flatpak = cg(
            "/user.slice/user-1000.slice/user@1000.service/app.slice/app-flatpak-org.mozilla.firefox-4455.scope",
        );
        assert_eq!(cgroup_app(&flatpak).as_deref(), Some("org.mozilla.firefox"));
        let snap = cg(
            "/user.slice/user-1000.slice/user@1000.service/app.slice/snap.firefox.firefox-991.scope",
        );
        assert_eq!(cgroup_app(&snap).as_deref(), Some("firefox"));
    }

    #[test]
    fn plain_sessions_and_services_are_not_apps() {
        let shell = cg("/user.slice/user-1000.slice/session-3.scope");
        assert_eq!(cgroup_app(&shell), None);
        assert_eq!(unit_kind(&shell), Unit::Session);
        let user_unit =
            cg("/user.slice/user-1000.slice/user@1000.service/app.slice/syncthing.service");
        assert_eq!(cgroup_app(&user_unit), None);
        assert_eq!(unit_kind(&user_unit), Unit::UserService);
        let system = cg("/system.slice/cron.service");
        assert_eq!(unit_kind(&system), Unit::System);
        assert_eq!(unit_kind(&cg("/init.scope")), Unit::System);
    }

    #[test]
    fn owner_uses_the_desktop_name_when_installed() {
        let firefox = cg(
            "/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-firefox-1234.scope",
        );
        let named = owner_name(&firefox, |id| {
            (id == "firefox").then(|| "Firefox".to_string())
        });
        assert_eq!(named.as_deref(), Some("Firefox"));
        assert_eq!(owner_name(&firefox, |_| None).as_deref(), Some("firefox"));
    }

    #[test]
    fn desktop_entries_give_their_name() {
        let entry = "[Desktop Entry]\nType=Application\nName=Terminal\nName[de]=Terminal\nExec=gnome-terminal\n";
        assert_eq!(desktop_entry_name(entry).as_deref(), Some("Terminal"));
        assert_eq!(desktop_entry_name("[Desktop Entry]\nExec=x\n"), None);
    }

    #[test]
    fn desktop_session_programs_are_recognised() {
        assert!(is_desktop_session("gnome-shell"));
        assert!(is_desktop_session("xdg-desktop-portal-gnome"));
        assert!(!is_desktop_session("bash"));
    }
}
