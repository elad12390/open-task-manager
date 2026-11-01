//! macOS: launchd, Finder, AppKit icons, `ps`, `lsof`, `pmset` and `ioreg`.

mod classify;
pub mod icons;
mod launchd;
pub mod selftest;
mod system;

use std::{collections::HashMap, fs, os::unix::io::AsRawFd, path::Path, process::Command};

use gpui::{TitlebarOptions, WindowBackgroundAppearance, point, px};

use super::{Caps, Chrome, WindowRole};
use crate::{metrics::PsRow, sysops::home};

pub use classify::classify;
#[cfg(test)]
pub use launchd::plist_program;
pub use launchd::{control_service, services, startup_items, toggle_startup};
pub use system::{connections, installed_apps, power, system_info, trash_app};

pub const APP_ID: &str = "org.opentaskmanager.native";

pub const CAPS: Caps = Caps {
    system_group: "macOS",
    apps_group: "Apps",
    groups: &["macOS", "Apps", "Command-line & Other"],
    mod_key: "cmd",
    mod_label: "⌘",
    reveal_label: "Show in Finder",
    reveal_app_label: "Show App in Finder",
    can_uninstall: true,
    glass_blur: true,
    in_window_menu: false,
    show_app_source: false,
};

pub fn data_root() -> std::path::PathBuf {
    home().join("Library/Application Support/OpenTaskManager")
}

/// Open a folder in Finder.
pub fn open_path(path: &Path) {
    let _ = Command::new("open").arg(path).spawn();
}

/// Select a file in its Finder window.
pub fn reveal(path: &str) {
    let _ = Command::new("open").args(["-R", path]).spawn();
}

/// Whether a mounted volume appears in the Disk list: the boot volume and the external volumes.
pub fn shows_volume(mount: &Path, _fs_type: &str) -> bool {
    mount == Path::new("/") || mount.starts_with("/Volumes/")
}

/// macOS refuses to signal only launchd and the kernel (pid 0 and 1), which `send_signal` already checks.
pub fn is_kernel_process(_pid: u32) -> bool {
    false
}

/// Signals go straight to the process: the app is not sandboxed from its processes.
pub fn signal(pid: u32, sig: i32) -> Result<(), String> {
    if unsafe { libc::kill(pid as i32, sig) } == 0 {
        Ok(())
    } else {
        Err(format!("PID {pid}: {}", std::io::Error::last_os_error()))
    }
}

/// The process table comes from `ps` and sysinfo here, so there is no host table to read.
pub fn host_processes() -> Option<Vec<crate::metrics::ProcessInfo>> {
    None
}

/// Turn off the unified buffer cache for a file, so the benchmark measures the disk.
pub fn bypass_cache(file: &fs::File) {
    unsafe { libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1) };
}

pub fn window_chrome(role: WindowRole) -> Chrome {
    let (title, traffic_light) = match role {
        WindowRole::Main => ("Open Task Manager", point(px(26.), px(26.))),
        WindowRole::Popout => ("Popout", point(px(-40.), px(-40.))),
    };
    Chrome {
        titlebar: Some(TitlebarOptions {
            title: Some(title.into()),
            appears_transparent: true,
            traffic_light_position: Some(traffic_light),
        }),
        background: WindowBackgroundAppearance::Blurred,
    }
}

/// Owner, CPU and resident memory for every process from `ps`, which (unlike sysinfo's task APIs)
/// can read processes owned by other users without root.
pub fn process_table() -> HashMap<u32, PsRow> {
    let out = Command::new("ps")
        .args(["-axo", "pid=,ppid=,user=,pcpu=,rss=,comm="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    out.lines()
        .filter_map(|l| {
            // Five space-separated fields, then the executable path, which may contain spaces.
            let mut rest = l.trim_start();
            let mut field = || {
                let (f, r) = rest.split_once(char::is_whitespace)?;
                rest = r.trim_start();
                Some(f)
            };
            let pid = field()?.parse().ok()?;
            let ppid = field()?.parse().ok()?;
            let user = field()?.to_string();
            let cpu = field()?.parse().ok()?;
            let rss: u64 = field()?.parse().ok()?;
            // Login shells are listed as "-/bin/zsh".
            let path = rest.trim_end().trim_start_matches('-').to_string();
            Some((
                pid,
                PsRow {
                    ppid,
                    user,
                    cpu,
                    memory: rss * 1024,
                    path,
                },
            ))
        })
        .collect()
}

/// `ps` state letters for a process ("S", "R", "T" = stopped…), or None if it is gone.
pub fn process_state(pid: u32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}
