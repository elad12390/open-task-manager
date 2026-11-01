//! `otm --demo <scene>`: a fictional machine (each OS has its own, under `platform::demo_data`) and
//! scripted tours of each feature, used to record the README and website GIFs. In demo mode nothing
//! is read from the real system and no real process is ever signalled.

use std::time::Duration;

use gpui::{Context, point, px};

use crate::{
    app::{OtmApp, Page},
    columns::{ColumnId, GroupBy},
    filter::Chip,
    metrics::{DiskInfo, ProcessInfo, Snapshot},
    platform,
    processes::{Nav, ROW_HEIGHT, Row, RowId},
    sysops::{BenchResult, FolderSize, Scan},
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

/// A frame of the fictional machine at time `tick` (seconds); CPU moves smoothly, memory drifts.
pub fn snapshot(tick: u64, removed: &[u32]) -> Snapshot {
    let machine = platform::demo_data::machine();
    let t = tick as f32;
    let mut processes: Vec<ProcessInfo> = machine
        .seeds
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
        memory_used: machine.memory_base + ((t * 0.2).sin() * 0.4 * GB as f32) as u64,
        memory_total: machine.memory_total,
        swap_used: 512 * MB,
        net_down: (((t * 0.8).sin() * 0.5 + 0.6) * 2.4 * MB as f32) as u64,
        net_up: (((t * 1.1).cos() * 0.5 + 0.6) * 0.4 * MB as f32) as u64,
        disks: vec![DiskInfo {
            mount: "/".into(),
            total: machine.disk_total,
            available: machine.disk_free,
        }],
        processes,
        uptime: 3 * 86_400 + 4 * 3600 + tick,
        host: machine.host.into(),
        os: machine.os.into(),
        cpu_brand: machine.cpu_brand.into(),
    }
}

impl OtmApp {
    /// Fill every page with fictional data and start the scene's script.
    pub fn start_demo(&mut self, scene: String, cx: &mut Context<Self>) {
        platform::demo_data::activate();
        let machine = platform::demo_data::machine();
        self.system = machine.system;
        self.startup = machine.startup;
        self.services = machine.services;
        self.connections = machine.connections;
        self.apps = machine.apps;
        self.power = Some(machine.power);
        let folders = |at: u64, scale: u64| Scan {
            at,
            items: machine
                .folders
                .iter()
                .map(|&(name, gb)| FolderSize {
                    path: format!("{}/{name}", machine.home),
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
                        if app.page == crate::app::Page::Users {
                            app.rebuild_listing();
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        // The scene script. `OTM_DEMO_CAPTIONS=0` hides the captions (for store screenshots).
        let steps = script(&scene);
        let hide_captions = std::env::var("OTM_DEMO_CAPTIONS").is_ok_and(|v| v == "0");
        cx.spawn(async move |this, cx| {
            // Let the window, icons and first frames settle before the recording-worthy part.
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            for (wait, step) in steps {
                if this
                    .update(cx, |app, cx| {
                        step(app, cx);
                        if hide_captions {
                            app.demo_caption = None;
                        }
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
    let c = platform::demo_data::machine().captions;
    match scene {
        "overview" => vec![
            step(2600, page(Page::Overview, c.intro)),
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
            step(2600, group(GroupBy::Category, c.category)),
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
            step(2000, page(Page::Settings, c.settings)),
        ],
        "process-tree" => vec![
            step(2600, group(GroupBy::Parent, c.root)),
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
            step(2600, move |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.toggle_group("split:0", false);
                app.set_cursor(Some(RowId::Group("split:0".into())));
                app.demo_caption = Some(c.system.into());
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
            step(2800, move |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                let id = RowId::Pid(1604);
                app.set_cursor(Some(id.clone()));
                open_menu_at(app, id);
                app.demo_caption = Some(c.menu.into());
            }),
            step(2400, |app: &mut OtmApp, _: &mut Context<OtmApp>| {
                app.context_menu = None;
                app.demo_kill(&[1604]);
                app.notice = Some(("Force quit 1 process".into(), false));
                app.demo_caption = Some("Force quit — gone".into());
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
            step(1900, page(Page::Startup, c.startup)),
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
/// The row's window position follows the list's measured bounds, clamped the way the scroll will clamp.
fn open_menu_at(app: &mut OtmApp, id: RowId) {
    app.set_cursor(Some(id.clone()));
    let ix = app.row_index(&id);
    let list = app.scroll.0.borrow().base_handle.clone();
    let offset = ix as f32 * ROW_HEIGHT;
    let scrolled = offset.min(f32::from(list.max_offset().height));
    let row_top = f32::from(list.bounds().origin.y) + offset - scrolled;
    app.scroll.scroll_to_item(ix, gpui::ScrollStrategy::Top);
    app.context_menu = Some((id, point(px(420.), px(row_top + ROW_HEIGHT))));
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
