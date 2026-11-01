//! Linux demo data: a fictional Ubuntu desktop ("Demo PC", user "alex") with systemd, a GNOME session,
//! Chrome, Firefox (Flatpak), Spotify (Snap), VS Code, Files and a terminal. Each process carries the
//! cgroup systemd gives it, so the real Linux classification files it under the same app or service.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::{
    platform::{Captions, Machine, Seed},
    sysops::{Connection, InstalledApp, Power, Service, StartupItem, SystemInfo, now_secs},
};

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;
const NAME_PREFIX: &str = "icon-name:";

static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Turns on the fictional desktop's identities (cgroups, desktop names and icons) for this process.
pub fn activate() {
    ACTIVE.store(true, Ordering::Relaxed);
}

/// The cgroup of a fictional process, while demo mode is on.
pub fn cgroup(pid: u32) -> Option<&'static str> {
    ACTIVE.load(Ordering::Relaxed).then(|| {
        CGROUPS
            .iter()
            .find(|(p, _)| *p == pid)
            .map_or("", |(_, c)| c)
    })
}

/// The name of a fictional app's desktop entry, while demo mode is on.
pub fn desktop_name(id: &str) -> Option<String> {
    ACTIVE
        .load(Ordering::Relaxed)
        .then(|| {
            NAMES
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, name)| name.to_string())
        })
        .flatten()
}

/// The theme icon of a fictional app or folder, as an icon-name key, while demo mode is on.
pub fn icon_of(path: &str) -> Option<String> {
    if !ACTIVE.load(Ordering::Relaxed) {
        return None;
    }
    if let Some((_, icon)) = ICONS.iter().find(|(p, _)| *p == path) {
        return Some(format!("{NAME_PREFIX}{icon}"));
    }
    let name = path.strip_prefix("/home/alex/")?;
    if name.contains('/') {
        return None;
    }
    let icon = match name {
        "Desktop" => "user-desktop",
        "Documents" => "folder-documents",
        "Downloads" => "folder-download",
        "Music" => "folder-music",
        "Pictures" => "folder-pictures",
        "Videos" => "folder-videos",
        _ => "folder",
    };
    Some(format!("{NAME_PREFIX}{icon}"))
}

const CHROME: &str = "/opt/google/chrome/chrome";
const FIREFOX: &str = "/app/lib/firefox/firefox";
const SPOTIFY: &str = "/snap/spotify/current/usr/share/spotify/spotify";
const CODE: &str = "/usr/share/code/code";

const fn p(
    pid: u32,
    ppid: u32,
    name: &'static str,
    user: &'static str,
    cpu: f32,
    mem_mb: u64,
    path: &'static str,
) -> Seed {
    Seed {
        pid,
        ppid,
        name,
        user,
        cpu,
        mem_mb,
        path,
    }
}

/// The fictional desktop's processes: (pid, parent, name, user, base CPU %, memory MB, path).
const SEEDS: &[Seed] = &[
    p(1, 0, "systemd", "root", 0.4, 18, "/usr/lib/systemd/systemd"),
    p(2, 0, "kthreadd", "root", 0.0, 0, ""),
    p(4, 2, "rcu_gp", "root", 0.0, 0, ""),
    p(5, 2, "kworker/0:1", "root", 0.3, 0, ""),
    p(6, 2, "kworker/u32:0", "root", 0.2, 0, ""),
    p(7, 2, "ksoftirqd/0", "root", 0.0, 0, ""),
    p(8, 2, "migration/0", "root", 0.0, 0, ""),
    p(9, 2, "kswapd0", "root", 0.1, 0, ""),
    p(10, 2, "kcompactd0", "root", 0.0, 0, ""),
    p(11, 2, "jbd2/nvme0n1p2-", "root", 0.0, 0, ""),
    p(
        300,
        1,
        "systemd-journal",
        "root",
        0.6,
        36,
        "/usr/lib/systemd/systemd-journald",
    ),
    p(
        302,
        1,
        "systemd-udevd",
        "root",
        0.2,
        9,
        "/usr/lib/systemd/systemd-udevd",
    ),
    p(
        304,
        1,
        "systemd-logind",
        "root",
        0.1,
        8,
        "/usr/lib/systemd/systemd-logind",
    ),
    p(
        306,
        1,
        "systemd-resolve",
        "systemd-resolve",
        0.0,
        10,
        "/usr/lib/systemd/systemd-resolved",
    ),
    p(
        310,
        1,
        "NetworkManager",
        "root",
        0.5,
        24,
        "/usr/sbin/NetworkManager",
    ),
    p(
        312,
        1,
        "dbus-daemon",
        "messagebus",
        0.3,
        6,
        "/usr/bin/dbus-daemon",
    ),
    p(
        316,
        1,
        "polkitd",
        "polkitd",
        0.0,
        14,
        "/usr/lib/polkit-1/polkitd",
    ),
    p(
        320,
        1,
        "bluetoothd",
        "root",
        0.1,
        7,
        "/usr/libexec/bluetooth/bluetoothd",
    ),
    p(324, 1, "cupsd", "root", 0.1, 18, "/usr/sbin/cupsd"),
    p(328, 1, "snapd", "root", 0.4, 58, "/usr/lib/snapd/snapd"),
    p(332, 1, "cron", "root", 0.0, 2, "/usr/sbin/cron"),
    p(336, 1, "gdm3", "root", 0.2, 22, "/usr/sbin/gdm3"),
    p(
        800,
        1,
        "gnome-session-b",
        "alex",
        0.2,
        42,
        "/usr/libexec/gnome-session-binary",
    ),
    p(
        802,
        800,
        "gnome-shell",
        "alex",
        6.2,
        610,
        "/usr/bin/gnome-shell",
    ),
    p(804, 802, "Xwayland", "alex", 0.9, 96, "/usr/bin/Xwayland"),
    p(
        806,
        800,
        "gsd-power",
        "alex",
        0.0,
        18,
        "/usr/libexec/gsd-power",
    ),
    p(
        808,
        800,
        "gsd-color",
        "alex",
        0.0,
        14,
        "/usr/libexec/gsd-color",
    ),
    p(810, 800, "pipewire", "alex", 0.7, 22, "/usr/bin/pipewire"),
    p(
        812,
        810,
        "wireplumber",
        "alex",
        0.5,
        30,
        "/usr/bin/wireplumber",
    ),
    p(
        814,
        800,
        "xdg-desktop-por",
        "alex",
        0.2,
        34,
        "/usr/libexec/xdg-desktop-portal",
    ),
    p(
        816,
        800,
        "gnome-keyring-d",
        "alex",
        0.0,
        11,
        "/usr/bin/gnome-keyring-daemon",
    ),
    p(1200, 1, "chrome", "alex", 6.5, 520, CHROME),
    p(1201, 1200, "chrome", "alex", 4.2, 380, CHROME),
    p(1202, 1200, "chrome", "alex", 3.8, 690, CHROME),
    p(1203, 1200, "chrome", "alex", 1.1, 410, CHROME),
    p(1204, 1200, "chrome", "alex", 0.4, 360, CHROME),
    p(1205, 1200, "chrome", "alex", 0.2, 280, CHROME),
    p(1206, 1200, "chrome", "alex", 0.1, 240, CHROME),
    p(1207, 1200, "chrome", "alex", 0.0, 190, CHROME),
    p(1300, 1, "firefox", "alex", 3.2, 610, FIREFOX),
    p(1301, 1300, "Web Content", "alex", 2.1, 330, FIREFOX),
    p(1302, 1300, "Web Content", "alex", 0.6, 270, FIREFOX),
    p(1303, 1300, "RDD Process", "alex", 0.2, 150, FIREFOX),
    p(1400, 1, "spotify", "alex", 1.4, 240, SPOTIFY),
    p(1401, 1400, "spotify", "alex", 0.9, 180, SPOTIFY),
    p(1510, 1, "code", "alex", 2.1, 380, CODE),
    p(1511, 1510, "code", "alex", 1.2, 260, CODE),
    p(1512, 1510, "code", "alex", 0.8, 210, CODE),
    p(1520, 1, "nautilus", "alex", 0.2, 64, "/usr/bin/nautilus"),
    p(
        1600,
        1,
        "gnome-terminal-",
        "alex",
        0.3,
        52,
        "/usr/libexec/gnome-terminal-server",
    ),
    p(1602, 1600, "bash", "alex", 0.0, 8, "/usr/bin/bash"),
    p(1603, 1602, "node", "alex", 7.4, 420, "/usr/bin/node"),
    p(1604, 1602, "python3", "alex", 24.0, 260, "/usr/bin/python3"),
    p(
        1605,
        1603,
        "esbuild",
        "alex",
        1.2,
        48,
        "/usr/lib/node_modules/esbuild/bin/esbuild",
    ),
    p(1700, 1, "otm", "alex", 1.8, 92, "/app/bin/otm"),
];

const ROOT: &str = "0::/";
const INIT: &str = "0::/init.scope";
const SESSION: &str = "0::/user.slice/user-1000.slice/session-2.scope";
const SHELL: &str =
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/gnome-shell-wayland.service";
const PIPEWIRE: &str =
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/pipewire.service";
const PORTAL: &str =
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/xdg-desktop-portal.service";
const KEYRING: &str =
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/gnome-keyring-daemon.service";
const CHROME_CG: &str =
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-google-chrome-1200.scope";
const FIREFOX_CG: &str = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-flatpak-org.mozilla.firefox-1300.scope";
const SPOTIFY_CG: &str =
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/snap.spotify.spotify-1400.scope";
const CODE_CG: &str =
    "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-code-1510.scope";
const FILES_CG: &str = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-gnome-org.gnome.Nautilus-1520.scope";
const TERMINAL_CG: &str = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-org.gnome.Terminal.slice/gnome-terminal-server.service";
const VTE_CG: &str = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-org.gnome.Terminal.slice/vte-spawn-3a1e.scope";
const OTM_CG: &str = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-flatpak-io.github.elad12390.open_task_manager-1700.scope";

/// The cgroup of each fictional process (pid, cgroup as `/proc/<pid>/cgroup` reads it).
const CGROUPS: &[(u32, &str)] = &[
    (1, INIT),
    (2, ROOT),
    (4, ROOT),
    (5, ROOT),
    (6, ROOT),
    (7, ROOT),
    (8, ROOT),
    (9, ROOT),
    (10, ROOT),
    (11, ROOT),
    (300, "0::/system.slice/systemd-journald.service"),
    (302, "0::/system.slice/systemd-udevd.service"),
    (304, "0::/system.slice/systemd-logind.service"),
    (306, "0::/system.slice/systemd-resolved.service"),
    (310, "0::/system.slice/NetworkManager.service"),
    (312, "0::/system.slice/dbus.service"),
    (316, "0::/system.slice/polkit.service"),
    (320, "0::/system.slice/bluetooth.service"),
    (324, "0::/system.slice/cups.service"),
    (328, "0::/system.slice/snapd.service"),
    (332, "0::/system.slice/cron.service"),
    (336, "0::/system.slice/gdm.service"),
    (800, SESSION),
    (802, SHELL),
    (804, SHELL),
    (806, SESSION),
    (808, SESSION),
    (810, PIPEWIRE),
    (812, PIPEWIRE),
    (814, PORTAL),
    (816, KEYRING),
    (1200, CHROME_CG),
    (1201, CHROME_CG),
    (1202, CHROME_CG),
    (1203, CHROME_CG),
    (1204, CHROME_CG),
    (1205, CHROME_CG),
    (1206, CHROME_CG),
    (1207, CHROME_CG),
    (1300, FIREFOX_CG),
    (1301, FIREFOX_CG),
    (1302, FIREFOX_CG),
    (1303, FIREFOX_CG),
    (1400, SPOTIFY_CG),
    (1401, SPOTIFY_CG),
    (1510, CODE_CG),
    (1511, CODE_CG),
    (1512, CODE_CG),
    (1520, FILES_CG),
    (1600, TERMINAL_CG),
    (1602, VTE_CG),
    (1603, VTE_CG),
    (1604, VTE_CG),
    (1605, VTE_CG),
    (1700, OTM_CG),
];

/// The desktop-entry names of the fictional apps (their desktop IDs as cgroups name them).
const NAMES: &[(&str, &str)] = &[
    ("org.mozilla.firefox", "Firefox"),
    ("google-chrome", "Google Chrome"),
    ("spotify", "Spotify"),
    ("code", "Visual Studio Code"),
    ("org.gnome.Nautilus", "Files"),
    ("org.gnome.Terminal", "Terminal"),
    ("io.github.elad12390.open_task_manager", "Open Task Manager"),
];

/// The theme icons of the fictional apps' executables.
const ICONS: &[(&str, &str)] = &[
    (FIREFOX, "firefox"),
    (CHROME, "google-chrome"),
    (SPOTIFY, "spotify-client"),
    (CODE, "code"),
    ("/usr/bin/nautilus", "org.gnome.Nautilus"),
    ("/usr/libexec/gnome-terminal-server", "org.gnome.Terminal"),
    ("/app/bin/otm", "utilities-system-monitor"),
];

/// The fictional PC's system facts, services, startup items, apps and power, with the lists of its
/// disk scan. Captions name the OS's own things (systemd, autostart, the file manager).
pub fn machine() -> Machine {
    Machine {
        seeds: SEEDS,
        host: "Demo PC",
        os: "Ubuntu 24.04 LTS · Linux 6.8",
        cpu_brand: "AMD Ryzen 7 7840U",
        memory_base: 6 * GB,
        memory_total: 16 * GB,
        disk_total: 512 * GB,
        disk_free: 301 * GB,
        system: SystemInfo {
            model: "ThinkPad X1 Carbon Gen 11".into(),
            chip: "AMD Ryzen 7 7840U".into(),
            os: "Ubuntu 24.04 LTS".into(),
            build: "6.8.0-52-generic".into(),
            arch: "x86_64".into(),
            memory: 16 * GB,
            physical_cores: 8,
            performance_cores: 0,
            efficiency_cores: 0,
            booted: now_secs() - (3 * 86_400 + 4 * 3600),
        },
        startup: vec![
            StartupItem {
                label: "syncthing.service".into(),
                scope: "User unit",
                file: PathBuf::from("/home/alex/.config/systemd/user/syncthing.service"),
                loaded: true,
                editable: true,
                program: Some(PathBuf::from("/usr/bin/syncthing")),
            },
            StartupItem {
                label: "discord".into(),
                scope: "Autostart",
                file: PathBuf::from("/home/alex/.config/autostart/discord.desktop"),
                loaded: true,
                editable: true,
                program: Some(PathBuf::from("/usr/bin/discord")),
            },
            StartupItem {
                label: "steam".into(),
                scope: "Autostart",
                file: PathBuf::from("/home/alex/.config/autostart/steam.desktop"),
                loaded: false,
                editable: true,
                program: Some(PathBuf::from("/usr/games/steam")),
            },
            StartupItem {
                label: "gnome-software-service".into(),
                scope: "System autostart",
                file: PathBuf::from("/etc/xdg/autostart/gnome-software-service.desktop"),
                loaded: true,
                editable: false,
                program: Some(PathBuf::from("/usr/bin/gnome-software")),
            },
            StartupItem {
                label: "backup-home".into(),
                scope: "User unit",
                file: PathBuf::from("/home/alex/.config/systemd/user/backup-home.service"),
                loaded: true,
                editable: true,
                program: Some(PathBuf::from("/home/alex/.local/bin/backup-home")),
            },
        ],
        services: [
            ("NetworkManager.service", Some(310), "System", None),
            ("cups.service", Some(324), "System", None),
            ("gdm.service", Some(336), "System", None),
            ("bluetooth.service", Some(320), "System", None),
            ("snapd.service", Some(328), "System", None),
            ("cron.service", Some(332), "System", None),
            ("ssh.service", None, "System", Some("/usr/sbin/sshd")),
            ("pipewire.service", Some(810), "User", None),
            ("wireplumber.service", Some(812), "User", None),
            (
                "syncthing.service",
                None,
                "User",
                Some("/usr/bin/syncthing"),
            ),
        ]
        .into_iter()
        .map(|(label, pid, scope, program)| Service {
            label: label.into(),
            pid,
            status: 0,
            program: program.map(PathBuf::from),
            scope,
        })
        .collect(),
        connections: [
            (
                1200,
                "chrome",
                "TCP",
                "192.168.1.20:54012->142.250.74.14:443",
                "ESTABLISHED",
            ),
            (1200, "chrome", "UDP", "*:5353", ""),
            (
                1300,
                "firefox",
                "TCP",
                "192.168.1.20:54210->151.101.1.140:443",
                "ESTABLISHED",
            ),
            (
                1400,
                "spotify",
                "TCP",
                "192.168.1.20:54301->35.186.224.25:443",
                "ESTABLISHED",
            ),
            (1603, "node", "TCP", "127.0.0.1:5173", "LISTEN"),
            (1604, "python3", "TCP", "127.0.0.1:8000", "LISTEN"),
            (306, "systemd-resolve", "UDP", "127.0.0.53:53", ""),
            (324, "cupsd", "TCP", "127.0.0.1:631", "LISTEN"),
        ]
        .into_iter()
        .map(|(pid, process, proto, endpoint, state)| Connection {
            pid,
            process: process.into(),
            proto: proto.into(),
            endpoint: endpoint.into(),
            state: state.into(),
        })
        .collect(),
        apps: [
            (
                "Firefox",
                "135.0",
                "/var/lib/flatpak/app/org.mozilla.firefox",
                412,
                "Flatpak",
            ),
            (
                "Google Chrome",
                "131.0.6778",
                "/opt/google/chrome",
                1290,
                "deb",
            ),
            ("Spotify", "1.2.52", "/snap/spotify/current", 386, "Snap"),
            (
                "Visual Studio Code",
                "1.96.2",
                "/usr/share/code",
                640,
                "deb",
            ),
            (
                "Open Task Manager",
                "0.1.0",
                "/var/lib/flatpak/app/io.github.elad12390.open_task_manager",
                24,
                "Flatpak",
            ),
            ("Zoom", "6.3.0", "/opt/zoom", 290, "Manual"),
        ]
        .into_iter()
        .map(|(name, version, path, mb, source)| InstalledApp {
            name: name.into(),
            version: version.into(),
            path: PathBuf::from(path),
            bytes: Some(mb * MB),
            source,
        })
        .collect(),
        power: Power {
            has_battery: true,
            source: "Battery Power".into(),
            percent: Some(64),
            status: "discharging".into(),
            watts: Some(-11.4),
            voltage: Some(12.3),
            cycles: Some(212),
            health: Some(91.0),
            temperature: Some(44.0),
            low_power_mode: false,
            thermal: "58 °C".into(),
        },
        home: "/home/alex",
        folders: &[
            ("Projects", 46),
            ("Downloads", 22),
            ("Videos", 38),
            ("Documents", 9),
            ("Pictures", 17),
            ("Music", 6),
            ("Desktop", 2),
        ],
        captions: Captions {
            intro: "Open Task Manager — a native, open-source system monitor for Linux",
            category: "Or by category: system services, apps and tools",
            root: "systemd at the root: the operating system's processes and yours",
            system: "The system's own processes, grouped by subsystem",
            menu: "Quit, force quit, suspend, show in Files, show connections, copy…",
            startup: "Startup items: enable or disable autostart apps and user services",
            settings: "Light or dark, text size, update speed",
        },
    }
}
