//! Linux: `/proc` and `/sys` for processes and hardware, systemd for services and user units, `ss` for
//! sockets, XDG autostart entries for startup items, UPower-style reads from `/sys` for power, and
//! desktop entries for installed apps.

pub mod apps;
pub mod demo_data;
pub mod host;
mod hostproc;
pub mod icons;
mod net;
mod owners;
mod power;
mod proc;
pub mod selftest;
mod services;
mod startup;

use std::{
    fs,
    path::{Path, PathBuf},
};

use gpui::{TitlebarOptions, WindowBackgroundAppearance};

use super::{Caps, Chrome, WindowRole};
use crate::metrics::ProcessInfo;

pub use apps::{installed_apps, trash_app};
pub use net::connections;
pub use power::power;
pub use proc::{is_kernel_process, own_pid, process_state, process_table, system_info};
pub use services::{control_service, services};
pub use startup::{startup_items, toggle_startup};

/// Your own command-line tools and shells, each filed under the one you ran them from.
pub const TOOLS: &str = "Command-line & Other";

/// The Flatpak application ID: the window's Wayland app id must equal the desktop file's name to get its icon.
pub const APP_ID: &str = "io.github.elad12390.open_task_manager.OpenTaskManager";

pub const CAPS: Caps = Caps {
    system_group: crate::categories::OS_OWNER,
    apps_group: "Apps",
    groups: &[crate::categories::OS_OWNER, "Apps", TOOLS],
    mod_key: "ctrl",
    mod_label: "Ctrl+",
    reveal_label: "Show in Files",
    reveal_app_label: "Show App Folder",
    can_uninstall: true,
    glass_blur: false,
    in_window_menu: true,
    show_app_source: true,
};

/// Runs a command on the host and returns its output (see `host::run`).
fn host(cmd: &str, args: &[&str]) -> Result<String, String> {
    let argv: Vec<&str> = std::iter::once(cmd).chain(args.iter().copied()).collect();
    host::run(&argv).map_err(|e| e.to_string())
}

/// The process table of a sandboxed app, read from the host in one command (None outside a sandbox,
/// or when the host cannot be asked: the shared code then reads what the process can see).
pub fn host_processes() -> Option<Vec<ProcessInfo>> {
    host::sandboxed().then(hostproc::sample).flatten()
}

/// Sends a signal to a process of the host's table, through the host inside a sandbox.
pub fn signal(pid: u32, sig: i32) -> Result<(), String> {
    if host::sandboxed() {
        return host("kill", &[&format!("-{sig}"), &pid.to_string()]).map(|_| ());
    }
    if unsafe { libc::kill(pid as i32, sig) } == 0 {
        Ok(())
    } else {
        Err(format!("PID {pid}: {}", std::io::Error::last_os_error()))
    }
}

pub fn data_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::sysops::home().join(".local/share"))
        .join("OpenTaskManager")
}

/// Open a folder in the file manager.
pub fn open_path(path: &Path) {
    let path = path.to_string_lossy().into_owned();
    std::thread::spawn(move || {
        let _ = host("xdg-open", &[&path]);
    });
}

/// Select a file in the file manager (through the FileManager1 D-Bus interface, which highlights
/// the file), or open its folder when no file manager answers.
pub fn reveal(path: &str) {
    let path = path.to_string();
    std::thread::spawn(move || {
        let uri = format!("'file://{}'", percent_encode(&path));
        let shown = host(
            "gdbus",
            &[
                "call",
                "--session",
                "--dest",
                "org.freedesktop.FileManager1",
                "--object-path",
                "/org/freedesktop/FileManager1",
                "--method",
                "org.freedesktop.FileManager1.ShowItems",
                &format!("[{uri}]"),
                "''",
            ],
        )
        .is_ok();
        if !shown {
            let parent = Path::new(&path).parent().unwrap_or(Path::new("/"));
            let _ = host("xdg-open", &[&parent.to_string_lossy()]);
        }
    });
}

/// A path as a file URI body: everything but unreserved characters and `/` is percent-encoded.
fn percent_encode(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
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

/// Filesystems that hold no disk space of their own; they stay out of the Disk list.
const PSEUDO_FS: &[&str] = &[
    "proc",
    "sysfs",
    "tmpfs",
    "devtmpfs",
    "devpts",
    "cgroup",
    "cgroup2",
    "overlay",
    "squashfs",
    "autofs",
    "mqueue",
    "hugetlbfs",
    "debugfs",
    "tracefs",
    "securityfs",
    "pstore",
    "bpf",
    "fusectl",
    "configfs",
    "binfmt_misc",
    "nsfs",
    "ramfs",
    "efivarfs",
    "rpc_pipefs",
    "fuse.snapfuse",
];

/// Mount points that are kernel or snap plumbing rather than volumes.
const PSEUDO_DIRS: &[&str] = &["/proc", "/sys", "/dev", "/run", "/snap"];

pub fn shows_volume(mount: &Path, fs_type: &str) -> bool {
    !PSEUDO_FS.contains(&fs_type) && !PSEUDO_DIRS.iter().any(|d| mount.starts_with(d))
}

/// Kernel threads and system services (their executables are root's or the kernel's) are the
/// operating system's. Your apps, their helpers and your command-line tools are filed under the app
/// or tool they belong to.
pub fn classify(p: &ProcessInfo) -> (&'static str, String) {
    match demo_data::cgroup(p.pid) {
        Some(cgroup) => classify_with(p, cgroup, demo_data::desktop_name),
        None => {
            let cgroup = proc::cgroup(p.pid).unwrap_or_default();
            classify_with(p, &cgroup, owners::desktop_name)
        }
    }
}

fn classify_with(
    p: &ProcessInfo,
    cgroup: &str,
    desktop: impl Fn(&str) -> Option<String>,
) -> (&'static str, String) {
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
    let unit = owners::unit_kind(cgroup);
    if proc::is_kernel_thread(p.pid, p.ppid, p.path.is_empty()) {
        (CAPS.system_group, "Kernel threads".to_string())
    } else if p.pid == 1 || unit == owners::Unit::System || (p.user == "root" && system_dir) {
        (CAPS.system_group, "Services".to_string())
    } else if owners::is_desktop_session(&p.name) {
        (CAPS.system_group, "Desktop session".to_string())
    } else if let Some(app) = owners::owner_name(cgroup, desktop) {
        (CAPS.apps_group, app)
    } else if unit == owners::Unit::UserService {
        (CAPS.system_group, "Services".to_string())
    } else {
        (TOOLS, p.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_named(name: &str, path: &str) -> ProcessInfo {
        ProcessInfo {
            pid: 4_000_000,
            ppid: Some(4_000_001),
            name: name.into(),
            user: "elad".into(),
            path: path.into(),
            ..Default::default()
        }
    }

    const FIREFOX: &str =
        "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-firefox-1234.scope\n";

    #[test]
    fn an_app_scope_makes_an_app_named_after_its_desktop_entry() {
        let p = proc_named("Web Content", "/usr/lib/firefox/firefox");
        let got = classify_with(&p, FIREFOX, |id| {
            (id == "firefox").then(|| "Firefox".into())
        });
        assert_eq!(got, (CAPS.apps_group, "Firefox".to_string()));
    }

    #[test]
    fn a_plain_command_is_a_tool() {
        let p = proc_named("grep", "/usr/bin/grep");
        assert_eq!(
            classify_with(&p, "0::/user.slice/session-2.scope\n", |_| None),
            (TOOLS, "grep".to_string())
        );
    }

    #[test]
    fn system_services_and_desktop_pieces_are_the_operating_systems() {
        let cron = proc_named("cron", "/usr/sbin/cron");
        assert_eq!(
            classify_with(&cron, "0::/system.slice/cron.service\n", |_| None),
            (CAPS.system_group, "Services".to_string())
        );
        let shell = proc_named("gnome-shell", "/usr/bin/gnome-shell");
        assert_eq!(
            classify_with(&shell, FIREFOX, |_| None),
            (CAPS.system_group, "Desktop session".to_string())
        );
    }

    #[test]
    fn volumes_skip_pseudo_filesystems() {
        assert!(shows_volume(Path::new("/"), "ext4"));
        assert!(shows_volume(Path::new("/data"), "btrfs"));
        assert!(!shows_volume(Path::new("/proc"), "proc"));
        assert!(!shows_volume(Path::new("/snap/core/1"), "squashfs"));
        assert!(!shows_volume(Path::new("/run/user/1000"), "tmpfs"));
    }

    #[test]
    fn file_uris_encode_spaces_and_quotes() {
        assert_eq!(
            percent_encode("/home/elad/My Files/it's.txt"),
            "/home/elad/My%20Files/it%27s.txt"
        );
    }
}
