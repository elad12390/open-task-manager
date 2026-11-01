//! The operating-system boundary. Shared code and the UI call these functions and read `CAPS`; only
//! this folder names an OS. `macos/` holds the launchd, Finder, AppKit and `ps` code; `linux/` reads
//! `/proc` and `/sys` and shells out to `systemctl`, `xdg-open` and friends.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("Open Task Manager builds for macOS and Linux only");

pub use imp::{
    CAPS, bypass_cache, classify, connections, control_service, data_root, installed_apps,
    is_kernel_process, open_path, power, process_state, process_table, reveal, services,
    shows_volume, startup_items, system_info, toggle_startup, trash_app,
};

#[cfg(all(target_os = "macos", test))]
pub use imp::plist_program;

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
    pub use super::imp::selftest::{app_checks, init_holds_tree, unmeasured};
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
}
