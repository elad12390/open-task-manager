//! The macOS menu bar (Open Task Manager, File, Edit, View, Update, Window, Help) and its shortcuts.
//! GPUI menu items have no checkmark state, so the bar is rebuilt with "✓" whenever a choice changes.

use std::path::PathBuf;

use gpui::{
    App, ClipboardItem, Context, Div, KeyBinding, Menu, MenuItem, PromptLevel, Stateful,
    SystemMenuType, actions, prelude::*,
};

use crate::{
    app::{OtmApp, Page},
    columns::GroupBy,
    recorder,
    settings::Appearance,
    sysops,
    windows::{Popout, Saver},
};

actions!(
    otm,
    [
        Quit,
        About,
        Hide,
        HideOthers,
        ShowAll,
        OpenSettings,
        ToggleRecording,
        ExportProcesses,
        ShowRecordingsFolder,
        CloseWindow,
        CopySelected,
        Find,
        ClearFilter,
        ShowOverview,
        ShowProcesses,
        ShowUsers,
        ShowConnections,
        ShowPower,
        ShowStartup,
        ShowServices,
        ShowApps,
        ShowDisk,
        ShowBenchmarks,
        ShowRecorder,
        GroupNone,
        GroupOwner,
        GroupUser,
        GroupParent,
        GroupCategory,
        ToggleColumnsMenu,
        AppearanceSystem,
        AppearanceLight,
        AppearanceDark,
        TextBigger,
        TextSmaller,
        TextActual,
        ToggleHighContrast,
        Every1s,
        Every2s,
        Every5s,
        Every10s,
        TogglePause,
        UpdateNow,
        Minimize,
        Zoom,
        OpenPopout,
        OpenScreensaver,
        ShowDataFolder,
    ]
);

/// The parts of app state the menu bar shows; the bar is rebuilt only when this changes.
#[derive(Clone, PartialEq)]
pub struct MenuState {
    pub refresh_secs: u64,
    pub paused: bool,
    pub recording: bool,
    pub group_by: GroupBy,
    pub appearance: Appearance,
    pub high_contrast: bool,
    pub page: Page,
}

impl MenuState {
    pub fn of(app: &OtmApp) -> Self {
        Self {
            refresh_secs: app.settings.refresh_secs,
            paused: app.paused,
            recording: app.recording.is_some(),
            group_by: app.layout.group_by,
            appearance: app.settings.appearance,
            high_contrast: app.settings.high_contrast,
            page: app.page,
        }
    }
}

fn check(on: bool, label: &str) -> String {
    if on {
        format!("✓ {label}")
    } else {
        format!("    {label}")
    }
}

pub fn build(s: &MenuState) -> Vec<Menu> {
    let page = |p: Page, action: Box<dyn gpui::Action>| MenuItem::Action {
        name: check(s.page == p, p.label()).into(),
        action,
        os_action: None,
    };
    vec![
        Menu {
            name: "Open Task Manager".into(),
            items: vec![
                MenuItem::action("About Open Task Manager", About),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Hide Open Task Manager", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action("Quit Open Task Manager", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action(
                    if s.recording {
                        "Stop and Save Recording"
                    } else {
                        "Start Recording"
                    },
                    ToggleRecording,
                ),
                MenuItem::action("Show Recordings Folder", ShowRecordingsFolder),
                MenuItem::separator(),
                MenuItem::action("Export Process List as CSV", ExportProcesses),
                MenuItem::separator(),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Copy Selected Process", CopySelected),
                MenuItem::separator(),
                MenuItem::action("Find Process", Find),
                MenuItem::action("Clear Filter", ClearFilter),
            ],
        },
        Menu {
            name: "View".into(),
            items: vec![
                page(Page::Overview, Box::new(ShowOverview)),
                page(Page::Processes, Box::new(ShowProcesses)),
                page(Page::Users, Box::new(ShowUsers)),
                page(Page::Connections, Box::new(ShowConnections)),
                page(Page::Power, Box::new(ShowPower)),
                page(Page::Startup, Box::new(ShowStartup)),
                page(Page::Services, Box::new(ShowServices)),
                page(Page::Apps, Box::new(ShowApps)),
                page(Page::Disk, Box::new(ShowDisk)),
                page(Page::Benchmarks, Box::new(ShowBenchmarks)),
                page(Page::Recorder, Box::new(ShowRecorder)),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "Group Processes By".into(),
                    items: vec![
                        MenuItem::action(check(s.group_by == GroupBy::None, "None"), GroupNone),
                        MenuItem::action(
                            check(
                                s.group_by == GroupBy::App,
                                "Owner (app, or Operating System)",
                            ),
                            GroupOwner,
                        ),
                        MenuItem::action(
                            check(s.group_by == GroupBy::User, "User Account"),
                            GroupUser,
                        ),
                        MenuItem::action(
                            check(s.group_by == GroupBy::Parent, "Process Tree"),
                            GroupParent,
                        ),
                        MenuItem::action(
                            check(s.group_by == GroupBy::Category, "Category (macOS / Apps)"),
                            GroupCategory,
                        ),
                    ],
                }),
                MenuItem::action("Choose Columns…", ToggleColumnsMenu),
                MenuItem::separator(),
                MenuItem::submenu(Menu {
                    name: "Appearance".into(),
                    items: vec![
                        MenuItem::action(
                            check(s.appearance == Appearance::System, "Match System"),
                            AppearanceSystem,
                        ),
                        MenuItem::action(
                            check(s.appearance == Appearance::Light, "Light"),
                            AppearanceLight,
                        ),
                        MenuItem::action(
                            check(s.appearance == Appearance::Dark, "Dark"),
                            AppearanceDark,
                        ),
                    ],
                }),
                MenuItem::action(check(s.high_contrast, "High Contrast"), ToggleHighContrast),
                MenuItem::separator(),
                MenuItem::action("Bigger Text", TextBigger),
                MenuItem::action("Smaller Text", TextSmaller),
                MenuItem::action("Actual Size", TextActual),
            ],
        },
        Menu {
            name: "Update".into(),
            items: vec![
                MenuItem::action(
                    check(s.refresh_secs == 1 && !s.paused, "Every Second"),
                    Every1s,
                ),
                MenuItem::action(
                    check(s.refresh_secs == 2 && !s.paused, "Every 2 Seconds"),
                    Every2s,
                ),
                MenuItem::action(
                    check(s.refresh_secs == 5 && !s.paused, "Every 5 Seconds"),
                    Every5s,
                ),
                MenuItem::action(
                    check(s.refresh_secs == 10 && !s.paused, "Every 10 Seconds"),
                    Every10s,
                ),
                MenuItem::separator(),
                MenuItem::action(check(s.paused, "Pause Updates"), TogglePause),
                MenuItem::action("Update Now", UpdateNow),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
                MenuItem::action("Show Popout", OpenPopout),
                MenuItem::action("Start Screensaver", OpenScreensaver),
            ],
        },
        Menu {
            name: "Help".into(),
            items: vec![MenuItem::action("Show Data Folder", ShowDataFolder)],
        },
    ]
}

impl OtmApp {
    /// Rebuild the menu bar when anything it shows (✓ marks, Start/Stop Recording) changed.
    pub fn sync_menus(&mut self, cx: &mut Context<Self>) {
        let state = MenuState::of(self);
        if self.menu_state.as_ref() != Some(&state) {
            cx.set_menus(build(&state));
            self.menu_state = Some(state);
        }
    }

    fn set_refresh(&mut self, secs: u64, cx: &mut Context<Self>) {
        self.settings.refresh_secs = secs;
        self.paused = false;
        self.settings.save();
        cx.notify();
    }

    fn set_appearance(&mut self, a: Appearance, cx: &mut Context<Self>) {
        self.settings.appearance = a;
        self.settings.save();
        cx.notify();
    }

    fn step_text(&mut self, delta: isize, cx: &mut Context<Self>) {
        const SIZES: [u32; 4] = [90, 100, 115, 130];
        let i = SIZES
            .iter()
            .position(|&s| s == self.settings.text_scale)
            .unwrap_or(1) as isize;
        self.settings.text_scale = SIZES[(i + delta).clamp(0, SIZES.len() as isize - 1) as usize];
        self.settings.save();
        cx.notify();
    }

    fn set_group(&mut self, g: GroupBy, cx: &mut Context<Self>) {
        self.layout.group_by = g;
        self.page = Page::Processes;
        self.layout_changed(cx);
    }

    fn toggle_recording(&mut self, cx: &mut Context<Self>) {
        match self.recording.take() {
            None => self.recording = Some(Vec::new()),
            Some(frames) => {
                self.notice = Some(match recorder::save(frames) {
                    Ok(p) => (
                        format!(
                            "Saved {}",
                            p.file_name().unwrap_or_default().to_string_lossy()
                        ),
                        false,
                    ),
                    Err(e) => (e, true),
                });
                self.recordings = recorder::list();
            }
        }
        cx.notify();
    }

    fn export_csv(&mut self, cx: &mut Context<Self>) {
        let cols: Vec<_> = self.layout.visible().map(|(_, c)| c.id).collect();
        let quote = |s: String| format!("\"{}\"", s.replace('"', "\"\""));
        let mut out = cols
            .iter()
            .map(|c| quote(c.title().into()))
            .collect::<Vec<_>>()
            .join(",")
            + "\n";
        for p in &self.snapshot.processes {
            out += &(cols
                .iter()
                .map(|c| quote(c.cell(p)))
                .collect::<Vec<_>>()
                .join(",")
                + "\n");
        }
        let home = PathBuf::from(std::env::var("HOME").unwrap_or_default());
        let path = home
            .join("Downloads")
            .join(format!("processes-{}.csv", sysops::now_secs()));
        self.notice = Some(match std::fs::write(&path, out) {
            Ok(()) => {
                sysops::reveal_in_finder(&path.to_string_lossy());
                (
                    format!(
                        "Exported {} processes to {}",
                        self.snapshot.processes.len(),
                        path.display()
                    ),
                    false,
                )
            }
            Err(e) => (e.to_string(), true),
        });
        cx.notify();
    }

    fn copy_selected(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self
            .selected
            .and_then(|pid| self.snapshot.processes.iter().find(|p| p.pid == pid))
        else {
            return;
        };
        let text = self
            .layout
            .visible()
            .map(|(_, c)| format!("{}: {}", c.id.title(), c.id.cell(p)))
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    /// Register every menu action on the root element (they reach it through the focused window).
    pub fn menu_actions(&self, root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        macro_rules! show {
            ($root:expr, $action:ty, $page:expr) => {
                $root.on_action(cx.listener(|this, _: &$action, _, cx| {
                    this.open_page($page, cx);
                    cx.notify();
                }))
            };
        }
        let root = show!(root, ShowOverview, Page::Overview);
        let root = show!(root, ShowProcesses, Page::Processes);
        let root = show!(root, ShowUsers, Page::Users);
        let root = show!(root, ShowConnections, Page::Connections);
        let root = show!(root, ShowPower, Page::Power);
        let root = show!(root, ShowStartup, Page::Startup);
        let root = show!(root, ShowServices, Page::Services);
        let root = show!(root, ShowApps, Page::Apps);
        let root = show!(root, ShowDisk, Page::Disk);
        let root = show!(root, ShowBenchmarks, Page::Benchmarks);
        let root = show!(root, ShowRecorder, Page::Recorder);
        let root = show!(root, OpenSettings, Page::Settings);
        root.on_action(cx.listener(|_, _: &About, window, cx| {
            let _answer = window.prompt(
                PromptLevel::Info,
                "Open Task Manager",
                Some("Version 0.1.0 · open source (Apache-2.0) · runs locally, sends nothing anywhere.\nBuilt in Rust with GPUI."),
                &["OK"],
                cx,
            );
        }))
        .on_action(cx.listener(|this, _: &ToggleRecording, _, cx| this.toggle_recording(cx)))
        .on_action(cx.listener(|_, _: &ShowRecordingsFolder, _, _| {
            let _ = std::fs::create_dir_all(recorder::dir());
            let _ = std::process::Command::new("open").arg(recorder::dir()).spawn();
        }))
        .on_action(cx.listener(|this, _: &ExportProcesses, _, cx| this.export_csv(cx)))
        .on_action(cx.listener(|_, _: &CloseWindow, window, _| window.remove_window()))
        .on_action(cx.listener(|this, _: &CopySelected, _, cx| this.copy_selected(cx)))
        .on_action(cx.listener(|this, _: &Find, _, cx| {
            this.open_page(Page::Processes, cx);
            this.notice = Some(("Type to filter processes · Esc clears".into(), false));
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &ClearFilter, _, cx| {
            this.query.clear();
            this.rebuild_rows();
            this.rebuild_listing();
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &GroupNone, _, cx| this.set_group(GroupBy::None, cx)))
        .on_action(cx.listener(|this, _: &GroupOwner, _, cx| this.set_group(GroupBy::App, cx)))
        .on_action(cx.listener(|this, _: &GroupUser, _, cx| this.set_group(GroupBy::User, cx)))
        .on_action(cx.listener(|this, _: &GroupParent, _, cx| this.set_group(GroupBy::Parent, cx)))
        .on_action(cx.listener(|this, _: &GroupCategory, _, cx| this.set_group(GroupBy::Category, cx)))
        .on_action(cx.listener(|this, _: &ToggleColumnsMenu, _, cx| {
            this.page = Page::Processes;
            this.column_menu = !this.column_menu;
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &AppearanceSystem, _, cx| this.set_appearance(Appearance::System, cx)))
        .on_action(cx.listener(|this, _: &AppearanceLight, _, cx| this.set_appearance(Appearance::Light, cx)))
        .on_action(cx.listener(|this, _: &AppearanceDark, _, cx| this.set_appearance(Appearance::Dark, cx)))
        .on_action(cx.listener(|this, _: &ToggleHighContrast, _, cx| {
            this.settings.high_contrast = !this.settings.high_contrast;
            this.settings.save();
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &TextBigger, _, cx| this.step_text(1, cx)))
        .on_action(cx.listener(|this, _: &TextSmaller, _, cx| this.step_text(-1, cx)))
        .on_action(cx.listener(|this, _: &TextActual, _, cx| {
            this.settings.text_scale = 100;
            this.settings.save();
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &Every1s, _, cx| this.set_refresh(1, cx)))
        .on_action(cx.listener(|this, _: &Every2s, _, cx| this.set_refresh(2, cx)))
        .on_action(cx.listener(|this, _: &Every5s, _, cx| this.set_refresh(5, cx)))
        .on_action(cx.listener(|this, _: &Every10s, _, cx| this.set_refresh(10, cx)))
        .on_action(cx.listener(|this, _: &TogglePause, _, cx| {
            this.paused = !this.paused;
            cx.notify();
        }))
        .on_action(cx.listener(|this, _: &UpdateNow, _, cx| {
            this.update_now = true;
            cx.notify();
        }))
        .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
        .on_action(cx.listener(|_, _: &Zoom, window, _| window.zoom_window()))
        .on_action(cx.listener(|_, _: &OpenPopout, _, cx| Popout::open(cx.entity(), cx)))
        .on_action(cx.listener(|_, _: &OpenScreensaver, _, cx| Saver::open(cx.entity(), cx)))
        .on_action(cx.listener(|_, _: &ShowDataFolder, _, _| {
            let _ = std::fs::create_dir_all(sysops::data_dir());
            let _ = std::process::Command::new("open").arg(sysops::data_dir()).spawn();
        }))
    }
}

/// App-level actions and keyboard shortcuts. View-level actions are handled in `OtmApp::render`.
pub fn install(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-r", ToggleRecording, None),
        KeyBinding::new("cmd-e", ExportProcesses, None),
        KeyBinding::new("cmd-w", CloseWindow, None),
        KeyBinding::new("cmd-c", CopySelected, None),
        KeyBinding::new("cmd-f", Find, None),
        KeyBinding::new("cmd-1", ShowOverview, None),
        KeyBinding::new("cmd-2", ShowProcesses, None),
        KeyBinding::new("cmd-3", ShowUsers, None),
        KeyBinding::new("cmd-4", ShowConnections, None),
        KeyBinding::new("cmd-5", ShowPower, None),
        KeyBinding::new("cmd-6", ShowStartup, None),
        KeyBinding::new("cmd-7", ShowServices, None),
        KeyBinding::new("cmd-8", ShowApps, None),
        KeyBinding::new("cmd-9", ShowDisk, None),
        KeyBinding::new("cmd-=", TextBigger, None),
        KeyBinding::new("cmd-+", TextBigger, None),
        KeyBinding::new("cmd--", TextSmaller, None),
        KeyBinding::new("cmd-0", TextActual, None),
        KeyBinding::new("cmd-p", TogglePause, None),
        KeyBinding::new("cmd-u", UpdateNow, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("shift-cmd-p", OpenPopout, None),
    ]);
}
