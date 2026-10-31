//! `otm --demo <scene>`: a fictional Mac ("Demo Mac", user "alex") and scripted tours of each
//! feature, used to record the README and website GIFs. In demo mode nothing is read from the real
//! system and no real process is ever signalled.

use std::{path::PathBuf, time::Duration};

use gpui::{Context, point, px};

use crate::{
    app::{OtmApp, Page},
    columns::{ColumnId, GroupBy},
    filter::Chip,
    metrics::{DiskInfo, ProcessInfo, Snapshot},
    processes::{Nav, Row, RowId},
    sysops::{
        BenchResult, Connection, FolderSize, InstalledApp, Power, Scan, Service, StartupItem,
        SystemInfo,
    },
};

pub const SCENES: [&str; 8] = [
    "overview",
    "process-tree",
    "memory",
    "columns",
    "filters",
    "context-menu",
    "keyboard",
    "pages",
];

const GB: u64 = 1024 * 1024 * 1024;
const MB: u64 = 1024 * 1024;

struct Seed {
    pid: u32,
    ppid: u32,
    name: &'static str,
    user: &'static str,
    cpu: f32,
    mem_mb: u64,
    path: &'static str,
}

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

const CHROME: &str = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const CHROME_HELPER: &str = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/A/Helpers/Google Chrome Helper (Renderer).app/Contents/MacOS/Google Chrome Helper (Renderer)";
const CHROME_GPU: &str = "/Applications/Google Chrome.app/Contents/Frameworks/Google Chrome Framework.framework/Versions/A/Helpers/Google Chrome Helper (GPU).app/Contents/MacOS/Google Chrome Helper (GPU)";
const FIREFOX: &str = "/Applications/Firefox.app/Contents/MacOS/firefox";
const FIREFOX_CHILD: &str =
    "/Applications/Firefox.app/Contents/MacOS/plugin-container.app/Contents/MacOS/plugin-container";
const SPOTIFY: &str = "/Applications/Spotify.app/Contents/MacOS/Spotify";
const SPOTIFY_HELPER: &str = "/Applications/Spotify.app/Contents/Frameworks/Spotify Helper (Renderer).app/Contents/MacOS/Spotify Helper (Renderer)";

/// The fictional Mac's processes: (pid, parent, name, user, base CPU %, memory MB, path).
const SEEDS: &[Seed] = &[
    p(1, 0, "launchd", "root", 0.4, 14, "/sbin/launchd"),
    p(310, 1, "logd", "root", 0.6, 22, "/usr/libexec/logd"),
    p(312, 1, "configd", "root", 0.2, 9, "/usr/libexec/configd"),
    p(
        318,
        1,
        "UserEventAgent",
        "root",
        0.1,
        11,
        "/usr/libexec/UserEventAgent",
    ),
    p(
        320,
        1,
        "fseventsd",
        "root",
        0.3,
        8,
        "/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/FSEvents.framework/Versions/A/Support/fseventsd",
    ),
    p(
        330,
        1,
        "WindowServer",
        "_windowserver",
        9.8,
        610,
        "/System/Library/PrivateFrameworks/SkyLight.framework/Versions/A/Resources/WindowServer",
    ),
    p(
        340,
        1,
        "mds",
        "root",
        1.9,
        96,
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/Metadata.framework/Support/mds",
    ),
    p(
        341,
        1,
        "mds_stores",
        "_mds_stores",
        2.4,
        182,
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/Metadata.framework/Support/mds_stores",
    ),
    p(
        342,
        341,
        "mdworker_shared",
        "alex",
        0.8,
        31,
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/Metadata.framework/Versions/A/Support/mdworker_shared",
    ),
    p(
        343,
        341,
        "mdworker_shared",
        "alex",
        0.5,
        28,
        "/System/Library/Frameworks/CoreServices.framework/Frameworks/Metadata.framework/Versions/A/Support/mdworker_shared",
    ),
    p(
        350,
        1,
        "coreaudiod",
        "_coreaudiod",
        1.2,
        44,
        "/usr/sbin/coreaudiod",
    ),
    p(
        352,
        1,
        "bluetoothd",
        "root",
        0.4,
        19,
        "/usr/sbin/bluetoothd",
    ),
    p(354, 1, "airportd", "root", 0.3, 15, "/usr/libexec/airportd"),
    p(
        356,
        1,
        "mDNSResponder",
        "_mdnsresponder",
        0.2,
        12,
        "/usr/sbin/mDNSResponder",
    ),
    p(360, 1, "securityd", "root", 0.1, 21, "/usr/sbin/securityd"),
    p(361, 1, "trustd", "root", 0.2, 17, "/usr/libexec/trustd"),
    p(
        362,
        1,
        "syspolicyd",
        "root",
        0.1,
        23,
        "/usr/libexec/syspolicyd",
    ),
    p(
        363,
        1,
        "tccd",
        "root",
        0.0,
        9,
        "/System/Library/PrivateFrameworks/TCC.framework/Support/tccd",
    ),
    p(
        370,
        1,
        "powerd",
        "root",
        0.1,
        7,
        "/System/Library/CoreServices/powerd.bundle/powerd",
    ),
    p(
        372,
        1,
        "thermalmonitord",
        "root",
        0.1,
        6,
        "/usr/libexec/thermalmonitord",
    ),
    p(
        380,
        1,
        "locationd",
        "_locationd",
        0.2,
        18,
        "/usr/libexec/locationd",
    ),
    p(
        382,
        1,
        "softwareupdated",
        "root",
        0.0,
        27,
        "/System/Library/PrivateFrameworks/MobileSoftwareUpdate.framework/Support/softwareupdated",
    ),
    p(
        390,
        1,
        "loginwindow",
        "alex",
        0.3,
        64,
        "/System/Library/CoreServices/loginwindow.app/Contents/MacOS/loginwindow",
    ),
    p(
        400,
        1,
        "Dock",
        "alex",
        0.6,
        88,
        "/System/Library/CoreServices/Dock.app/Contents/MacOS/Dock",
    ),
    p(
        401,
        1,
        "Finder",
        "alex",
        0.4,
        142,
        "/System/Library/CoreServices/Finder.app/Contents/MacOS/Finder",
    ),
    p(
        402,
        1,
        "ControlCenter",
        "alex",
        0.5,
        71,
        "/System/Library/CoreServices/ControlCenter.app/Contents/MacOS/ControlCenter",
    ),
    p(
        403,
        1,
        "NotificationCenter",
        "alex",
        0.2,
        58,
        "/System/Library/CoreServices/NotificationCenter.app/Contents/MacOS/NotificationCenter",
    ),
    p(
        404,
        1,
        "Spotlight",
        "alex",
        0.1,
        49,
        "/System/Library/CoreServices/Spotlight.app/Contents/MacOS/Spotlight",
    ),
    p(
        405,
        1,
        "SystemUIServer",
        "alex",
        0.1,
        33,
        "/System/Library/CoreServices/SystemUIServer.app/Contents/MacOS/SystemUIServer",
    ),
    p(
        410,
        1,
        "cloudd",
        "alex",
        0.7,
        52,
        "/System/Library/PrivateFrameworks/CloudKitDaemon.framework/Support/cloudd",
    ),
    p(
        411,
        1,
        "bird",
        "alex",
        0.3,
        26,
        "/System/Library/PrivateFrameworks/CloudDocsDaemon.framework/Versions/A/Support/bird",
    ),
    p(
        420,
        1,
        "siriactionsd",
        "alex",
        0.0,
        24,
        "/System/Library/PrivateFrameworks/LinkServices.framework/Versions/A/Support/siriactionsd",
    ),
    p(
        421,
        1,
        "photoanalysisd",
        "alex",
        3.1,
        210,
        "/System/Library/PrivateFrameworks/PhotoAnalysis.framework/Versions/A/Support/photoanalysisd",
    ),
    p(430, 1, "sharingd", "alex", 0.2, 37, "/usr/libexec/sharingd"),
    p(431, 1, "rapportd", "alex", 0.1, 16, "/usr/libexec/rapportd"),
    // Google Chrome and its helpers
    p(1200, 1, "Google Chrome", "alex", 6.5, 520, CHROME),
    p(
        1201,
        1200,
        "Google Chrome Helper (GPU)",
        "alex",
        4.2,
        380,
        CHROME_GPU,
    ),
    p(
        1202,
        1200,
        "Google Chrome Helper (Renderer)",
        "alex",
        3.8,
        690,
        CHROME_HELPER,
    ),
    p(
        1203,
        1200,
        "Google Chrome Helper (Renderer)",
        "alex",
        1.1,
        410,
        CHROME_HELPER,
    ),
    p(
        1204,
        1200,
        "Google Chrome Helper (Renderer)",
        "alex",
        0.4,
        360,
        CHROME_HELPER,
    ),
    p(
        1205,
        1200,
        "Google Chrome Helper (Renderer)",
        "alex",
        0.2,
        280,
        CHROME_HELPER,
    ),
    p(
        1206,
        1200,
        "Google Chrome Helper (Renderer)",
        "alex",
        0.1,
        240,
        CHROME_HELPER,
    ),
    p(
        1207,
        1200,
        "Google Chrome Helper (Renderer)",
        "alex",
        0.0,
        190,
        CHROME_HELPER,
    ),
    // Firefox
    p(1300, 1, "firefox", "alex", 3.2, 610, FIREFOX),
    p(
        1301,
        1300,
        "plugin-container",
        "alex",
        2.1,
        330,
        FIREFOX_CHILD,
    ),
    p(
        1302,
        1300,
        "plugin-container",
        "alex",
        0.6,
        270,
        FIREFOX_CHILD,
    ),
    p(
        1303,
        1300,
        "plugin-container",
        "alex",
        0.2,
        150,
        FIREFOX_CHILD,
    ),
    // Spotify
    p(1400, 1, "Spotify", "alex", 1.4, 240, SPOTIFY),
    p(
        1401,
        1400,
        "Spotify Helper (Renderer)",
        "alex",
        0.9,
        180,
        SPOTIFY_HELPER,
    ),
    // Apple apps
    p(
        1500,
        1,
        "Mail",
        "alex",
        0.5,
        210,
        "/System/Applications/Mail.app/Contents/MacOS/Mail",
    ),
    p(
        1510,
        1,
        "Music",
        "alex",
        0.8,
        160,
        "/System/Applications/Music.app/Contents/MacOS/Music",
    ),
    p(
        1520,
        1,
        "Notes",
        "alex",
        0.1,
        95,
        "/System/Applications/Notes.app/Contents/MacOS/Notes",
    ),
    // Terminal with a shell and some tools
    p(
        1600,
        1,
        "Terminal",
        "alex",
        0.6,
        120,
        "/System/Applications/Utilities/Terminal.app/Contents/MacOS/Terminal",
    ),
    p(1601, 1600, "login", "root", 0.0, 3, "/usr/bin/login"),
    p(1602, 1601, "zsh", "alex", 0.0, 6, "/bin/zsh"),
    p(
        1603,
        1602,
        "node",
        "alex",
        7.4,
        420,
        "/opt/homebrew/bin/node",
    ),
    p(
        1604,
        1602,
        "python3",
        "alex",
        24.0,
        260,
        "/opt/homebrew/bin/python3",
    ),
    p(
        1605,
        1603,
        "esbuild",
        "alex",
        1.2,
        48,
        "/opt/homebrew/lib/node_modules/esbuild/bin/esbuild",
    ),
    // This app
    p(
        1700,
        1,
        "otm",
        "alex",
        1.8,
        92,
        "/Applications/Open Task Manager.app/Contents/MacOS/otm",
    ),
];

/// A frame of the fictional Mac at time `tick` (seconds); CPU moves smoothly, memory drifts.
pub fn snapshot(tick: u64, removed: &[u32]) -> Snapshot {
    let t = tick as f32;
    let mut processes: Vec<ProcessInfo> = SEEDS
        .iter()
        .filter(|s| !removed.contains(&s.pid))
        .map(|s| {
            let wave = ((t * 0.7 + s.pid as f32 * 0.37).sin() * 0.35 + 1.0).max(0.2);
            ProcessInfo {
                pid: s.pid,
                ppid: (s.ppid != 0).then_some(s.ppid),
                name: s.name.to_string(),
                user: s.user.to_string(),
                cpu: s.cpu * wave,
                memory: s.mem_mb * MB + ((t * 0.3 + s.pid as f32).sin().abs() * 4.0) as u64 * MB,
                disk_read: if s.pid % 3 == 0 {
                    (wave * 40.0) as u64 * 1024
                } else {
                    0
                },
                disk_write: if s.pid % 5 == 0 {
                    (wave * 120.0) as u64 * 1024
                } else {
                    0
                },
                state: if s.cpu > 2.0 {
                    "Runnable".into()
                } else {
                    "Sleeping".into()
                },
                run_time: 3600 * 5 + s.pid as u64 * 7 + tick,
                path: s.path.to_string(),
                is_self: s.name == "otm",
                owner_app: String::new(),
            }
        })
        .collect();
    let owners = crate::categories::owners(&processes);
    for pr in &mut processes {
        pr.owner_app = owners.get(&pr.pid).cloned().unwrap_or_default();
    }
    let cores: Vec<f32> = (0..12)
        .map(|i| {
            (((t * 0.9 + i as f32 * 1.3).sin() * 0.5 + 0.5) * if i < 8 { 46.0 } else { 18.0 })
                .max(2.0)
        })
        .collect();
    Snapshot {
        cpu: cores.iter().sum::<f32>() / cores.len() as f32,
        cores,
        memory_used: 11 * GB + ((t * 0.2).sin() * 0.4 * GB as f32) as u64,
        memory_total: 18 * GB,
        swap_used: 512 * MB,
        net_down: (((t * 0.8).sin() * 0.5 + 0.6) * 2.4 * MB as f32) as u64,
        net_up: (((t * 1.1).cos() * 0.5 + 0.6) * 0.4 * MB as f32) as u64,
        disks: vec![DiskInfo {
            mount: "/".into(),
            total: 994 * GB,
            available: 412 * GB,
        }],
        processes,
        uptime: 3 * 86_400 + 4 * 3600 + tick,
        host: "Demo Mac".into(),
        os: "macOS 26.1".into(),
        cpu_brand: "Apple M4 Pro".into(),
    }
}

impl OtmApp {
    /// Fill every page with fictional data and start the scene's script.
    pub fn start_demo(&mut self, scene: String, cx: &mut Context<Self>) {
        self.system = SystemInfo {
            model: "Mac16,8".into(),
            chip: "Apple M4 Pro".into(),
            os: "macOS 26.1".into(),
            build: "25B78".into(),
            arch: "arm64".into(),
            memory: 18 * GB,
            physical_cores: 12,
            performance_cores: 8,
            efficiency_cores: 4,
            booted: crate::sysops::now_secs() - (3 * 86_400 + 4 * 3600),
        };
        let agents = |label: &str, scope: &'static str, loaded: bool| StartupItem {
            label: label.to_string(),
            scope,
            file: PathBuf::from(format!(
                "{}/{label}.plist",
                if scope == "User agent" {
                    "/Users/alex/Library/LaunchAgents"
                } else {
                    "/Library/LaunchAgents"
                }
            )),
            loaded,
            editable: scope == "User agent",
        };
        self.startup = vec![
            agents("com.dropbox.dropboxmacupdate.agent", "System agent", true),
            agents("com.google.keystone.agent", "System agent", false),
            agents("us.zoom.updater", "System agent", true),
            agents("com.spotify.webhelper", "User agent", true),
            agents("homebrew.mxcl.postgresql@16", "User agent", true),
            agents("homebrew.mxcl.redis", "User agent", false),
            agents("com.example.backup", "User agent", true),
        ];
        self.services = [
            ("com.apple.Finder", Some(401)),
            ("com.apple.Dock.agent", Some(400)),
            ("com.apple.cloudd", Some(410)),
            ("com.apple.photoanalysisd", Some(421)),
            ("com.apple.quicklook", None),
            ("com.apple.sharingd", Some(430)),
            ("com.spotify.webhelper", None),
            ("homebrew.mxcl.postgresql@16", Some(1800)),
        ]
        .into_iter()
        .map(|(label, pid)| Service {
            label: label.into(),
            pid,
            status: 0,
        })
        .collect();
        self.connections = [
            (
                1200,
                "Google Chrome",
                "TCP",
                "192.168.1.20:54012->142.250.74.14:443",
                "ESTABLISHED",
            ),
            (1200, "Google Chrome", "UDP", "*:5353", ""),
            (
                1300,
                "firefox",
                "TCP",
                "192.168.1.20:54210->151.101.1.140:443",
                "ESTABLISHED",
            ),
            (
                1400,
                "Spotify",
                "TCP",
                "192.168.1.20:54301->35.186.224.25:443",
                "ESTABLISHED",
            ),
            (
                1500,
                "Mail",
                "TCP",
                "192.168.1.20:54402->17.42.251.41:993",
                "ESTABLISHED",
            ),
            (1603, "node", "TCP", "127.0.0.1:5173", "LISTEN"),
            (1604, "python3", "TCP", "127.0.0.1:8000", "LISTEN"),
            (356, "mDNSResponder", "UDP", "*:5353", ""),
            (430, "sharingd", "UDP", "*:0", ""),
        ]
        .into_iter()
        .map(|(pid, process, proto, endpoint, state)| Connection {
            pid,
            process: process.into(),
            proto: proto.into(),
            endpoint: endpoint.into(),
            state: state.into(),
        })
        .collect();
        self.apps = [
            ("Firefox", "135.0", 412),
            ("Google Chrome", "131.0.6778", 1290),
            ("Open Task Manager", "0.1.0", 24),
            ("Spotify", "1.2.52", 386),
            ("Visual Studio Code", "1.96.2", 640),
            ("Zoom", "6.3.0", 290),
        ]
        .into_iter()
        .map(|(name, version, mb)| InstalledApp {
            name: name.into(),
            version: version.into(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bytes: Some(mb * MB),
        })
        .collect();
        self.power = Some(Power {
            has_battery: true,
            source: "Battery Power".into(),
            percent: Some(82),
            status: "discharging".into(),
            watts: Some(-9.6),
            voltage: Some(12.71),
            cycles: Some(143),
            health: Some(97.0),
            temperature: Some(31.2),
            low_power_mode: false,
            thermal: "Nominal".into(),
        });
        let folders = |at: u64, scale: u64| Scan {
            at,
            items: [
                ("Projects", 84),
                ("Library", 61),
                ("Movies", 38),
                ("Downloads", 22),
                ("Pictures", 17),
                ("Documents", 9),
                ("Music", 6),
                ("Desktop", 2),
            ]
            .into_iter()
            .map(|(name, gb)| FolderSize {
                path: format!("/Users/alex/{name}"),
                bytes: gb * GB * scale / 100,
            })
            .collect(),
        };
        let now = crate::sysops::now_secs();
        self.scans = vec![folders(now - 3600, 100), folders(now - 86_400 * 3, 96)];
        self.bench = Some(BenchResult {
            single_core: 148.0,
            multi_core: 1046.0,
            threads: 12,
            memory_gbps: 19.4,
            disk_write_mbps: 1580.0,
            disk_read_mbps: 5810.0,
        });
        // Room for full group names in the Name column.
        if let Some(i) = self
            .layout
            .columns
            .iter()
            .position(|c| c.id == ColumnId::Name)
        {
            self.layout.set_width(i, 300.);
        }
        // Graphs start full.
        for tick in 0..90 {
            self.history.push(&snapshot(tick, &[]));
        }
        self.demo_tick = 90;
        self.snapshot = snapshot(self.demo_tick, &[]);
        self.rebuild_rows();
        self.request_icons(cx);

        // Live updates once a second.
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this
                    .update(cx, |app, cx| {
                        app.demo_tick += 1;
                        app.snapshot = snapshot(app.demo_tick, &app.demo_removed);
                        app.history.push(&app.snapshot);
                        app.rebuild_rows();
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        // The scene script.
        let steps = script(&scene);
        cx.spawn(async move |this, cx| {
            // Let the window, icons and first frames settle before the recording-worthy part.
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            for (wait, step) in steps {
                if this
                    .update(cx, |app, cx| {
                        step(app, cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    return;
                }
                cx.background_executor()
                    .timer(Duration::from_millis(wait))
                    .await;
            }
            let _ = cx.update(|cx| cx.quit());
        })
        .detach();
    }

    /// Demo stand-in for a signal: the fictional process simply disappears.
    pub fn demo_kill(&mut self, pids: &[u32]) {
        self.demo_removed.extend_from_slice(pids);
        self.snapshot = snapshot(self.demo_tick, &self.demo_removed);
        self.rebuild_rows();
    }

    fn row_index(&self, id: &RowId) -> usize {
        self.rows.iter().position(|r| &r.id() == id).unwrap_or(0)
    }

    fn group_id(&self, label: &str) -> Option<RowId> {
        self.rows.iter().find_map(|r| match r {
            Row::Group { label: l, key, .. } if l == label => Some(RowId::Group(key.clone())),
            _ => None,
        })
    }
}

type Step = Box<dyn Fn(&mut OtmApp, &mut Context<OtmApp>)>;

fn step(wait_ms: u64, f: impl Fn(&mut OtmApp, &mut Context<OtmApp>) + 'static) -> (u64, Step) {
    (wait_ms, Box::new(f))
}

fn caption(text: &'static str) -> impl Fn(&mut OtmApp, &mut Context<OtmApp>) {
    move |app, _| app.demo_caption = Some(text.into())
}

fn page(page: Page, text: &'static str) -> impl Fn(&mut OtmApp, &mut Context<OtmApp>) {
    move |app, cx| {
        app.open_page(page, cx);
        app.demo_caption = Some(text.into());
    }
}

fn group(g: GroupBy, text: &'static str) -> impl Fn(&mut OtmApp, &mut Context<OtmApp>) {
    move |app, cx| {
        app.page = Page::Processes;
        app.collapsed.clear();
        app.set_cursor(None);
        app.layout.group_by = g;
        app.demo_caption = Some(text.into());
        app.rebuild_rows();
        app.scroll.scroll_to_item(0, gpui::ScrollStrategy::Top);
        let _ = cx;
    }
}

fn key(nav: Nav, text: &'static str) -> impl Fn(&mut OtmApp, &mut Context<OtmApp>) {
    move |app, _| {
        app.navigate_processes(nav);
        app.demo_caption = Some(text.into());
    }
}

/// The steps of a scene: (milliseconds to hold after the step, step).
fn script(scene: &str) -> Vec<(u64, Step)> {
    match scene {
        "overview" => vec![
            step(
                2600,
                page(
                    Page::Overview,
                    "Open Task Manager — a native, open-source system monitor for macOS",
                ),
            ),
            step(2200, group(GroupBy::None, "Every process, live")),
            step(
                3000,
                group(
                    GroupBy::Parent,
                    "A real process tree: the operating system's processes and yours",
                ),
            ),
            step(
                2800,
                group(
                    GroupBy::App,
                    "Grouped by the app that owns them, with memory summed per app",
                ),
            ),
            step(
                2600,
                group(
                    GroupBy::Category,
                    "Or by category: macOS subsystems, apps and tools",
                ),
            ),
            step(1800, page(Page::Users, "Who is using what")),
            step(
                1800,
                page(Page::Connections, "Open network connections per process"),
            ),
            step(
                1800,
                page(Page::Disk, "Disk space, largest folders and scan history"),
            ),
            step(1800, page(Page::Power, "Battery, power and thermal state")),
            step(
                2000,
                page(
                    Page::Settings,
                    "Light or dark, clear or tinted glass, text size, update speed",
                ),
            ),
        ],
        "process-tree" => vec![
            step(
                2600,
                group(
                    GroupBy::Parent,
                    "launchd at the root: the operating system's processes and yours",
                ),
            ),
            step(1800, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.toggle_branch(1, false);
                app.set_cursor(Some(RowId::Pid(1)));
                app.demo_caption = Some("Folding a branch folds everything inside it".into());
            }),
            step(1800, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.toggle_branch(1, false);
                app.demo_caption = Some("Unfolding opens one level at a time".into());
            }),
            step(2200, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.toggle_group("split:1", false);
                app.set_cursor(Some(RowId::Group("split:1".into())));
                app.demo_caption = Some("Your processes, grouped by the app that owns them".into());
            }),
            step(2600, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.toggle_group("split:1/Google Chrome", true);
                app.set_cursor(Some(RowId::Group("split:1/Google Chrome".into())));
                app.demo_caption = Some("An app and all of its helpers, together".into());
            }),
            step(2600, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.toggle_group("split:0", false);
                app.set_cursor(Some(RowId::Group("split:0".into())));
                app.demo_caption = Some("macOS's own processes, grouped by subsystem".into());
            }),
        ],
        "memory" => vec![
            step(2400, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                group(GroupBy::App, "Sort by memory: what each app really costs")(app, cx);
                app.layout.sort = ColumnId::Memory;
                app.layout.descending = true;
                app.rebuild_rows();
            }),
            step(2600, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                if let Some(RowId::Group(key)) = app.group_id("Operating System") {
                    app.toggle_group(&key, false);
                }
                if let Some(id) = app.group_id("Google Chrome") {
                    let ix = app.row_index(&id);
                    app.scroll.scroll_to_item(ix, gpui::ScrollStrategy::Top);
                    app.set_cursor(Some(id));
                }
                app.demo_caption = Some("Σ adds up the app and every helper it runs".into());
            }),
            step(2600, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.layout.group_by = GroupBy::Parent;
                app.collapsed.clear();
                app.rebuild_rows();
                app.set_cursor(Some(RowId::Pid(1200)));
                app.demo_caption = Some("In the tree, every branch shows its total".into());
            }),
            step(2400, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                let ix = app.row_index(&RowId::Pid(1200));
                app.scroll.scroll_to_item(ix, gpui::ScrollStrategy::Center);
                app.toggle_branch(1200, false);
                app.demo_caption = Some("Fold it, and the total stays".into());
            }),
        ],
        "columns" => vec![
            step(2000, group(GroupBy::None, "Choose your columns")),
            step(1200, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.column_menu = true;
                app.demo_caption = Some("Show and hide any column".into());
            }),
            step(900, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                app.layout.toggle(ColumnId::Path);
                app.layout_changed(cx);
            }),
            step(900, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                app.layout.toggle(ColumnId::DiskRead);
                app.layout_changed(cx);
            }),
            step(1200, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                app.layout.toggle(ColumnId::RunTime);
                app.layout_changed(cx);
            }),
            step(1600, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                app.column_menu = false;
                app.layout.move_column(ColumnId::Memory, ColumnId::Owner);
                app.layout_changed(cx);
                app.demo_caption = Some("Drag headers to reorder".into());
            }),
            step(300, widen(ColumnId::Name, 330.)),
            step(300, widen(ColumnId::Name, 360.)),
            step(1400, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                widen(ColumnId::Name, 400.)(app, cx);
                app.demo_caption = Some("Drag edges to resize".into());
            }),
            step(
                500,
                scroll_x(-120., "Scroll sideways to reach every column"),
            ),
            step(
                500,
                scroll_x(-260., "Scroll sideways to reach every column"),
            ),
            step(
                1600,
                scroll_x(-420., "Scroll sideways to reach every column"),
            ),
            step(1200, scroll_x(0., "Your layout is saved")),
        ],
        "context-menu" => vec![
            step(2000, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                group(GroupBy::Parent, "Right-click any process")(app, cx);
                app.layout.sort = ColumnId::Cpu;
                app.layout.descending = true;
                app.rebuild_rows();
            }),
            step(2800, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                let id = RowId::Pid(1604);
                app.set_cursor(Some(id.clone()));
                open_menu_at(app, id);
                app.demo_caption = Some(
                    "Quit, force quit, suspend, reveal in Finder, show connections, copy…".into(),
                );
            }),
            step(2400, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.context_menu = None;
                app.demo_kill(&[1604]);
                app.notice = Some(("Force quit 1 process".into(), false));
                app.demo_caption = Some("Force Quit — gone".into());
            }),
            step(2600, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                let id = app.group_id("Google Chrome").unwrap_or(RowId::Pid(1200));
                open_menu_at(app, id);
                app.demo_caption =
                    Some("Or act on a whole app: quit all of its processes at once".into());
            }),
            step(800, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.context_menu = None
            }),
        ],
        "keyboard" => vec![
            step(1600, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                group(GroupBy::Parent, "Keyboard first")(app, cx);
                app.set_cursor(Some(RowId::Pid(1)));
            }),
            step(500, key(Nav::Down, "↓  move")),
            step(500, key(Nav::Down, "↓  move")),
            step(900, key(Nav::Down, "↓  move")),
            step(1200, key(Nav::Left, "←  fold")),
            step(1200, key(Nav::Right, "→  unfold one level")),
            step(500, key(Nav::Up, "↑  move")),
            step(1200, key(Nav::Up, "↑  move")),
            step(1300, key(Nav::Toggle, "Space  fold or unfold")),
            step(1300, key(Nav::Toggle, "Space  fold or unfold")),
            step(1500, key(Nav::ExpandAll, "⌥→  open everything inside")),
            step(1200, key(Nav::End, "End  jump to the bottom")),
            step(1200, key(Nav::Home, "Home  back to the top")),
        ],
        "filters" => vec![
            step(1800, |app: &mut OtmApp, cx: &mut Context<OtmApp>| {
                group(GroupBy::None, "Filter with chips: type, pick, combine")(app, cx);
                app.layout.sort = ColumnId::Cpu;
                app.layout.descending = true;
                app.rebuild_rows();
                app.focus_filter();
            }),
            step(
                350,
                typed(
                    "g",
                    "Suggestions as you type: apps, users, categories, states",
                ),
            ),
            step(
                350,
                typed(
                    "go",
                    "Suggestions as you type: apps, users, categories, states",
                ),
            ),
            step(
                1800,
                typed(
                    "goo",
                    "Suggestions as you type: apps, users, categories, states",
                ),
            ),
            step(1000, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.add_chip(Chip::App("Google Chrome".into()));
                app.focus_filter();
                app.demo_caption = Some("⏎ turns it into a chip".into());
            }),
            step(
                350,
                typed("f", "Chips of one kind widen: Chrome or Firefox"),
            ),
            step(
                1300,
                typed("fi", "Chips of one kind widen: Chrome or Firefox"),
            ),
            step(1300, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.add_chip(Chip::App("Firefox".into()));
                app.focus_filter();
            }),
            step(1700, typed("cpu", "cpu> and mem> add thresholds")),
            step(1000, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.suggest_ix = 1
            }),
            step(2800, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.add_chip(Chip::CpuAbove(5.));
                app.blur_filter();
                app.demo_caption =
                    Some("Different kinds narrow: Chrome or Firefox, above 5% CPU".into());
            }),
            step(1800, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.clear_filter();
                app.demo_caption = Some("Esc clears everything".into());
            }),
        ],
        "pages" => vec![
            step(
                1900,
                page(
                    Page::Startup,
                    "Startup items: enable or disable your launch agents",
                ),
            ),
            step(1900, page(Page::Services, "Services: restart or stop")),
            step(
                1900,
                page(Page::Connections, "Connections: who talks to whom"),
            ),
            step(
                1900,
                page(Page::Apps, "Installed apps: uninstall to the Trash"),
            ),
            step(
                1900,
                page(Page::Disk, "Disk space: largest folders and scan history"),
            ),
            step(1900, page(Page::Power, "Power and thermal state")),
            step(
                1900,
                page(
                    Page::Benchmarks,
                    "Benchmarks: CPU, memory and uncached disk",
                ),
            ),
            step(1900, page(Page::Users, "Users and what they run")),
            step(1900, page(Page::Settings, "Settings")),
        ],
        _ => vec![step(500, caption("Unknown demo scene"))],
    }
}

/// Scroll the row to the top of the list and open its right-click menu just below it, clear of the caption.
fn open_menu_at(app: &mut OtmApp, id: RowId) {
    app.set_cursor(Some(id.clone()));
    let ix = app.row_index(&id);
    app.scroll.scroll_to_item(ix, gpui::ScrollStrategy::Top);
    app.context_menu = Some((id, point(px(420.), px(172.))));
}

fn typed(text: &'static str, caption: &'static str) -> impl Fn(&mut OtmApp, &mut Context<OtmApp>) {
    move |app, _| {
        app.query = text.into();
        app.suggest_ix = 0;
        app.focus_filter();
        app.rebuild_rows();
        app.demo_caption = Some(caption.into());
    }
}

fn widen(id: ColumnId, width: f32) -> impl Fn(&mut OtmApp, &mut Context<OtmApp>) {
    move |app, cx| {
        if let Some(i) = app.layout.columns.iter().position(|c| c.id == id) {
            app.layout.set_width(i, width);
        }
        cx.notify();
    }
}

fn scroll_x(x: f32, text: &'static str) -> impl Fn(&mut OtmApp, &mut Context<OtmApp>) {
    move |app, _| {
        if x < 0. {
            for c in app.layout.columns.iter_mut() {
                c.visible = true;
            }
        }
        app.hscroll.set_offset(point(px(x), px(0.)));
        app.demo_caption = Some(text.into());
    }
}
