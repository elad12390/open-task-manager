//! The operating-system boundary. Shared code and the UI call these functions and read `CAPS`; only
//! this folder names an OS. `macos/` holds the launchd, Finder, AppKit and `ps` code; `linux/` reads
//! `/proc` and `/sys` and shells out to `systemctl`, `xdg-open` and friends.

#[cfg(target_os = "linux")]
pub(crate) mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("Open Task Manager builds for macOS and Linux only");

pub use imp::{
    APP_ID, CAPS, bypass_cache, classify, connections, control_service, data_root, host_processes,
    installed_apps, is_kernel_process, open_path, own_pid, power, process_state, process_table,
    reveal, services, shows_volume, signal, startup_items, system_info, toggle_startup, trash_app,
};

#[cfg(all(target_os = "macos", test))]
pub use imp::plist_program;

#[cfg(all(target_os = "linux", test))]
pub use linux::host;

/// The fictional machine of `otm --demo`: the data each OS shows, moved here from the shared scripts.
pub mod demo_data {
    pub use super::imp::demo_data::{activate, machine};
}

/// One fictional process of demo mode: its parent, owner, load, memory and executable.
pub struct Seed {
    pub pid: u32,
    pub ppid: u32,
    pub name: &'static str,
    pub user: &'static str,
    pub cpu: f32,
    pub mem_mb: u64,
    pub path: &'static str,
}

/// The scene captions that name the OS's own things (launch agents, the menu's reveal action).
pub struct Captions {
    pub intro: &'static str,
    pub category: &'static str,
    pub root: &'static str,
    pub system: &'static str,
    pub menu: &'static str,
    pub startup: &'static str,
    pub settings: &'static str,
}

/// A fictional machine for demo mode: its processes, fixed system facts and the lists the pages show.
pub struct Machine {
    pub seeds: &'static [Seed],
    pub host: &'static str,
    pub os: &'static str,
    pub cpu_brand: &'static str,
    pub memory_base: u64,
    pub memory_total: u64,
    pub disk_total: u64,
    pub disk_free: u64,
    pub system: crate::sysops::SystemInfo,
    pub startup: Vec<crate::sysops::StartupItem>,
    pub services: Vec<crate::sysops::Service>,
    pub connections: Vec<crate::sysops::Connection>,
    pub apps: Vec<crate::sysops::InstalledApp>,
    pub power: crate::sysops::Power,
    pub home: &'static str,
    pub folders: &'static [(&'static str, u64)],
    pub captions: Captions,
}

pub enum WindowRole {
    Main,
    Popout,
}

/// The titlebar and background of a window, as GPUI's window options take them.
pub struct Chrome {
    pub titlebar: Option<gpui::TitlebarOptions>,
    pub background: gpui::WindowBackgroundAppearance,
}

pub fn window_chrome(role: WindowRole) -> Chrome {
    imp::window_chrome(role)
}

/// The icon functions of the platform (app bundles and system icon files).
pub mod icons {
    pub use super::imp::icons::{USER_ICON, bundle_of, render_icons, system_icon};
}

/// Platform checks run by `otm --selftest`, after the shared ones.
pub mod selftest {
    pub use super::imp::selftest::{Throwaway, app_checks, init_holds_tree, unmeasured};
}

/// What differs between the platforms and the UI needs to know: group names, shortcut
/// spelling, window chrome and which actions exist.
pub struct Caps {
    /// The top-level group of the operating system's own processes.
    pub system_group: &'static str,
    /// The top-level group of apps: one section per app bundle, or per process on Linux.
    pub apps_group: &'static str,
    /// Top-level groups in display order.
    pub groups: &'static [&'static str],
    /// The modifier of shortcuts in GPUI keystroke syntax ("cmd" or "ctrl").
    pub mod_key: &'static str,
    /// The modifier as shown in text ("⌘" or "Ctrl+").
    pub mod_label: &'static str,
    pub reveal_label: &'static str,
    /// The same, for an app's bundle or install folder.
    pub reveal_app_label: &'static str,
    /// Whether applications can be moved to the trash from the app list.
    pub can_uninstall: bool,
    /// Whether the window background can be blurred behind the glass panels.
    pub glass_blur: bool,
    /// Whether the menu bar is drawn inside the window (no global menu bar on this platform).
    pub in_window_menu: bool,
    /// Whether the apps list shows where each app came from (Flatpak, Snap, packages…).
    pub show_app_source: bool,
}
