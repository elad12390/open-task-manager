//! Linux: `/proc` for processes and hardware, desktop entries for installed apps. Services, startup
//! items, sockets and power are placeholders for now (empty lists, or "Not available yet" for actions).

pub mod apps;
pub mod icons;
mod proc;
pub mod selftest;

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use gpui::{TitlebarOptions, WindowBackgroundAppearance};

use super::{Caps, Chrome, WindowRole};
use crate::{
    metrics::ProcessInfo,
    sysops::{Connection, Power, Service, ServiceAction, StartupItem},
};

pub use apps::{installed_apps, trash_app};
pub use proc::{process_state, process_table, system_info};

const NOT_YET: &str = "Not available yet";

pub const CAPS: Caps = Caps {
    system_group: crate::categories::OS_OWNER,
    apps_group: "Your Processes",
    groups: &[crate::categories::OS_OWNER, "Your Processes"],
    mod_key: "ctrl",
    mod_label: "Ctrl+",
    reveal_label: "Show in Files",
    reveal_app_label: "Show App Folder",
    can_uninstall: true,
    glass_blur: false,
    in_window_menu: true,
    show_app_source: true,
};

pub fn data_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::sysops::home().join(".local/share"))
        .join("OpenTaskManager")
}

/// Open a folder in the file manager.
pub fn open_path(path: &Path) {
    let _ = Command::new("xdg-open").arg(path).spawn();
}

/// Open the folder that holds a file (Linux file managers have no "select this file" call).
pub fn reveal(path: &str) {
    let parent = Path::new(path).parent().unwrap_or(Path::new("/"));
    let _ = Command::new("xdg-open").arg(parent).spawn();
}

/// Drop a file's cached pages, so a read after the write comes from the disk (called after fsync).
pub fn bypass_cache(file: &fs::File) {
    use std::os::unix::io::AsRawFd;
    unsafe { libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED) };
}

/// The main window is a normal window with the system's own title bar. The popout has no title bar:
/// it is dragged by its body. Neither can blur what is behind it, so both get an opaque background.
pub fn window_chrome(role: WindowRole) -> Chrome {
    let titlebar = match role {
        WindowRole::Main => Some(TitlebarOptions {
            title: Some("Open Task Manager".into()),
            appears_transparent: false,
            traffic_light_position: None,
        }),
        WindowRole::Popout => None,
    };
    Chrome {
        titlebar,
        background: WindowBackgroundAppearance::Opaque,
    }
}

/// Kernel threads and system services (their executables are root's or the kernel's) are the
/// operating system's; everything else is a process of yours, grouped under its own name.
pub fn classify(p: &ProcessInfo) -> (&'static str, String) {
    let system_dir = [
        "/usr/lib/",
        "/usr/libexec/",
        "/usr/sbin/",
        "/sbin/",
        "/lib/",
        "/lib64/",
        "/usr/lib64/",
    ]
    .iter()
    .any(|dir| p.path.starts_with(dir));
    let system = p.pid == 1 || (p.user == "root" && system_dir);
    if proc::is_kernel_thread(p.pid, p.ppid, p.path.is_empty()) {
        (CAPS.system_group, "Kernel threads".to_string())
    } else if system {
        (CAPS.system_group, "Services".to_string())
    } else {
        (CAPS.apps_group, p.name.clone())
    }
}

pub fn services() -> Vec<Service> {
    Vec::new()
}

pub fn startup_items() -> Vec<StartupItem> {
    Vec::new()
}

pub fn toggle_startup(_file: &Path, _enable: bool) -> Result<(), String> {
    Err(NOT_YET.into())
}

pub fn control_service(_label: &str, _action: ServiceAction) -> Result<(), String> {
    Err(NOT_YET.into())
}

pub fn connections() -> Vec<Connection> {
    Vec::new()
}

pub fn power() -> Power {
    Power::default()
}
