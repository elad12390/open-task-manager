use std::{
    collections::HashSet,
    path::PathBuf,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

use gpui::{
    Context, FocusHandle, FontWeight, KeyDownEvent, MouseButton, MouseMoveEvent, MouseUpEvent,
    Pixels, Render, UniformListScrollHandle, Window, div, prelude::*, px,
};

use crate::{
    columns::TableLayout,
    metrics::{History, Sampler, Snapshot},
    pages::Listing,
    processes::{Nav, Row},
    recorder::{Frame, Recording},
    settings::Settings,
    sysops::{
        BenchResult, Connection, InstalledApp, Power, Scan, Service, StartupItem, SystemInfo,
    },
    theme::Theme,
    ui,
    windows::{Popout, Saver},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Overview,
    Processes,
    Users,
    Startup,
    Services,
    Connections,
    Apps,
    Disk,
    Power,
    Benchmarks,
    Recorder,
    Settings,
}

impl Page {
    const SECTIONS: [(&'static str, &'static [Page]); 3] = [
        (
            "MONITOR",
            &[
                Page::Overview,
                Page::Processes,
                Page::Users,
                Page::Connections,
                Page::Power,
            ],
        ),
        (
            "MANAGE",
            &[Page::Startup, Page::Services, Page::Apps, Page::Disk],
        ),
        ("TOOLS", &[Page::Benchmarks, Page::Recorder, Page::Settings]),
    ];

    pub fn label(self) -> &'static str {
        match self {
            Page::Overview => "Overview",
            Page::Processes => "Processes",
            Page::Users => "Users",
            Page::Startup => "Startup Items",
            Page::Services => "Services",
            Page::Connections => "Connections",
            Page::Apps => "Installed Apps",
            Page::Disk => "Disk Space",
            Page::Power => "Power",
            Page::Benchmarks => "Benchmarks",
            Page::Recorder => "Recorder",
            Page::Settings => "Settings",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Page::Overview => "◎",
            Page::Processes => "☰",
            Page::Users => "◉",
            Page::Startup => "⏻",
            Page::Services => "⚙",
            Page::Connections => "⇅",
            Page::Apps => "▣",
            Page::Disk => "◔",
            Page::Power => "⚡",
            Page::Benchmarks => "⏱",
            Page::Recorder => "●",
            Page::Settings => "✦",
        }
    }

    pub fn filterable(self) -> bool {
        matches!(
            self,
            Page::Processes
                | Page::Users
                | Page::Startup
                | Page::Services
                | Page::Connections
                | Page::Apps
                | Page::Disk
        )
    }
}

/// Icons rendered per background batch, so one batch never holds the list up for long.
const ICON_BATCH: usize = 200;

pub struct Resize {
    pub index: usize,
    pub start_x: Pixels,
    pub start_width: f32,
}

pub struct OtmApp {
    pub snapshot: Snapshot,
    pub history: History,
    pub layout: TableLayout,
    pub settings: Settings,
    pub page: Page,
    pub query: String,
    /// Process filter chips (app, user, category, state, CPU, memory) that apply with `query`.
    pub chips: Vec<crate::filter::Chip>,
    /// Column-value chips of the listing pages and Disk Space; cleared when the page changes.
    pub list_chips: Vec<crate::filter::Chip>,
    /// The filter field has focus: it shows a caret and, while `suggest_open`, its suggestions.
    pub filter_focused: bool,
    pub suggest_open: bool,
    /// Highlighted suggestion.
    pub suggest_ix: usize,
    /// Caret blink phase.
    pub caret_on: bool,
    /// Processes that match the filter (shown in the subtitle while filtering).
    pub match_count: usize,
    /// Caret position in `query`, counted in characters from the end (0 = at the end), so text
    /// set from elsewhere keeps the caret at its end.
    pub caret_back: usize,
    pub collapsed: HashSet<String>,
    pub rows: Rc<Vec<Row>>,
    pub resizing: Option<Resize>,
    pub column_menu: bool,
    /// Sort chosen for each table outside the process list (see `tables`).
    pub table_sorts: std::collections::HashMap<&'static str, (usize, bool)>,
    /// The listing page's data is still being read.
    pub listing_loading: bool,
    /// The listing's list, for its on-screen width (the last column fills what is left).
    pub listing_scroll: UniformListScrollHandle,
    /// The press that is in progress closed the Columns menu (see the Columns button).
    pub column_menu_just_closed: bool,
    pub selected: Option<u32>,
    /// Keyboard cursor: a process or a group row.
    pub cursor: Option<crate::processes::RowId>,
    pub focus: FocusHandle,
    pub scroll: UniformListScrollHandle,
    /// Horizontal scroll of the process table (header and rows together).
    pub hscroll: gpui::ScrollHandle,
    pub listing: Listing,
    pub startup: Vec<StartupItem>,
    pub services: Vec<Service>,
    pub connections: Vec<Connection>,
    pub apps: Vec<InstalledApp>,
    pub power: Option<Power>,
    pub scans: Vec<Scan>,
    pub scanning: bool,
    pub bench: Option<BenchResult>,
    pub benching: bool,
    pub recording: Option<Vec<Frame>>,
    pub recordings: Vec<PathBuf>,
    pub replay: Option<Recording>,
    pub notice: Option<(String, bool)>,
    pub confirm_trash: Option<PathBuf>,
    pub system: SystemInfo,
    pub paused: bool,
    pub update_now: bool,
    pub menu_state: Option<crate::menus::MenuState>,
    /// App bundle path -> cached PNG icon.
    pub icons: std::collections::HashMap<String, PathBuf>,
    /// Icon keys that have been asked for (rendered or failed), so each is rendered once.
    pub icons_requested: HashSet<String>,
    /// Whether an icon render is running; the next batch waits for it.
    pub icons_rendering: bool,
    /// Owning app name -> its bundle path (for icons on owner rows).
    pub owner_bundles: std::collections::HashMap<String, String>,
    /// Right-click menu: the row it is for and where it opened.
    pub context_menu: Option<(crate::processes::RowId, gpui::Point<Pixels>)>,
    /// A Force Quit waiting for confirmation.
    pub confirm: Option<crate::context_menu::Confirm>,
    /// The page the window title was last set for.
    pub titled: Option<Page>,
    /// `--demo`: fictional data and a scripted tour; nothing real is read or signalled.
    pub demo_mode: bool,
    pub demo_tick: u64,
    pub demo_removed: Vec<u32>,
    pub demo_caption: Option<String>,
}

impl OtmApp {
    /// Render (once, in the background, a batch at a time) the icons the tables show: the open
    /// listing's rows, the processes and the folders of the last disk scan.
    pub fn request_icons(&mut self, cx: &mut Context<Self>) {
        if self.icons_rendering {
            return;
        }
        for p in &self.snapshot.processes {
            if p.owner_app != crate::categories::OS_OWNER
                && let Some(b) = crate::icons::bundle_of(&p.path)
            {
                self.owner_bundles.entry(p.owner_app.clone()).or_insert(b);
            }
        }
        let candidates: Vec<String> = self
            .listing
            .rows
            .iter()
            .filter_map(|r| r.icon.clone())
            .chain(self.snapshot.processes.iter().filter_map(|p| {
                crate::icons::process_key(&p.name, &p.path, &p.owner_app, &self.owner_bundles)
            }))
            .chain(
                self.scans
                    .first()
                    .into_iter()
                    .flat_map(|s| s.items.iter())
                    .map(|f| crate::icons::path_key(&f.path)),
            )
            .collect();
        let mut wanted: Vec<String> = Vec::new();
        for key in candidates {
            if wanted.len() == ICON_BATCH {
                break;
            }
            if !self.icons.contains_key(&key) && self.icons_requested.insert(key.clone()) {
                wanted.push(key);
            }
        }
        if wanted.is_empty() {
            return;
        }
        self.icons_rendering = true;
        self.run_bg(
            cx,
            move || crate::icons::render(&wanted),
            |app, found, _| {
                app.icons.extend(found);
                app.icons_rendering = false;
            },
        );
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        if std::env::var_os("OTM_BACKGROUND").is_none() {
            window.focus(&focus);
        }
        let demo = std::env::var("OTM_DEMO").ok();
        let sampler = Arc::new(Mutex::new(Sampler::new()));
        if demo.is_none() {
            cx.spawn(async move |this, cx| {
                loop {
                    let s = sampler.clone();
                    let started = std::time::Instant::now();
                    let snapshot = cx
                        .background_executor()
                        .spawn(async move { s.lock().unwrap().sample() })
                        .await;
                    if std::env::var_os("OTM_DEBUG").is_some() {
                        eprintln!(
                            "sample: {} processes in {:?}",
                            snapshot.processes.len(),
                            started.elapsed()
                        );
                    }
                    if this
                        .update(cx, |app, cx| {
                            app.history.push(&snapshot);
                            if let Some(frames) = &mut app.recording {
                                frames.push(Frame::from_snapshot(&snapshot));
                            }
                            app.snapshot = snapshot;
                            if app.selected.is_some_and(|pid| {
                                !app.snapshot.processes.iter().any(|p| p.pid == pid)
                            }) {
                                app.selected = None;
                            }
                            app.rebuild_rows();
                            if app.page == crate::app::Page::Users {
                                app.rebuild_listing();
                            }
                            app.request_icons(cx);
                            cx.notify();
                        })
                        .is_err()
                    {
                        break;
                    }
                    // Wait out the update interval in short steps so Pause and "Update Now" apply at once.
                    let mut waited = 0u64;
                    loop {
                        cx.background_executor()
                            .timer(Duration::from_millis(100))
                            .await;
                        waited += 100;
                        let Ok((secs, paused, now)) = this.update(cx, |app, _| {
                            (
                                app.settings.refresh_secs,
                                app.paused,
                                std::mem::take(&mut app.update_now),
                            )
                        }) else {
                            return;
                        };
                        if now || (!paused && waited >= secs * 1000) {
                            break;
                        }
                    }
                }
            })
            .detach();
        }

        let mut app = Self {
            snapshot: Snapshot::default(),
            history: History::default(),
            layout: TableLayout::load(),
            settings: Settings::load(),
            page: Page::Processes,
            query: String::new(),
            chips: Vec::new(),
            list_chips: Vec::new(),
            filter_focused: false,
            suggest_open: false,
            suggest_ix: 0,
            caret_on: true,
            match_count: 0,
            caret_back: 0,
            collapsed: HashSet::new(),
            rows: Rc::new(Vec::new()),
            resizing: None,
            column_menu: false,
            table_sorts: Default::default(),
            listing_loading: false,
            listing_scroll: UniformListScrollHandle::new(),
            column_menu_just_closed: false,
            selected: None,
            cursor: None,
            focus,
            scroll: UniformListScrollHandle::new(),
            hscroll: gpui::ScrollHandle::new(),
            listing: Listing::default(),
            startup: Vec::new(),
            services: Vec::new(),
            connections: Vec::new(),
            apps: Vec::new(),
            power: None,
            scans: Vec::new(),
            scanning: false,
            bench: None,
            benching: false,
            recording: None,
            recordings: Vec::new(),
            replay: None,
            notice: None,
            confirm_trash: None,
            system: SystemInfo::default(),
            paused: false,
            update_now: false,
            menu_state: None,
            icons: Default::default(),
            icons_requested: HashSet::new(),
            icons_rendering: false,
            owner_bundles: Default::default(),
            context_menu: None,
            confirm: None,
            titled: None,
            demo_mode: false,
            demo_tick: 0,
            demo_removed: Vec::new(),
            demo_caption: None,
        };
        app.start_caret_blink(cx);
        if let Some(scene) = demo {
            app.demo_mode = true;
            app.start_demo(scene, cx);
        } else {
            app.run_bg(cx, crate::sysops::system_info, |app, info, _| {
                app.system = info
            });
        }
        if let Ok(which) = std::env::var("OTM_OPEN") {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                if let Some(entity) = this.upgrade() {
                    cx.update(|cx| match which.as_str() {
                        "saver" => Saver::open(entity, cx),
                        _ => Popout::open(entity, cx),
                    })
                    .ok();
                }
            })
            .detach();
        }
        if let Ok(name) = std::env::var("OTM_PAGE")
            && let Some(page) = Page::SECTIONS
                .iter()
                .flat_map(|s| s.1.iter())
                .find(|p| format!("{p:?}") == name)
        {
            app.open_page(*page, cx);
        }
        app
    }

    pub fn layout_changed(&mut self, cx: &mut Context<Self>) {
        self.layout.save();
        self.rebuild_rows();
        cx.notify();
    }

    pub fn query_changed(&mut self) {
        if self.page == Page::Processes {
            self.rebuild_rows();
        } else {
            self.rebuild_listing();
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
        if self.confirm.is_some() {
            if matches!(k.key.as_str(), "escape" | "enter") {
                self.confirm = None;
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        // The suggestions panel takes ↑↓, ⏎/⇥ and Esc while it is open.
        if self.page.filterable()
            && self.filter_focused
            && self.suggest_open
            && !k.modifiers.platform
            && !k.modifiers.control
            && !k.modifiers.alt
        {
            let handled = match k.key.as_str() {
                key @ ("down" | "up" | "enter" | "tab") => {
                    let suggestions = self.suggestions();
                    let n = suggestions.len();
                    if n == 0 {
                        // Nothing to pick: Enter just closes the panel to show the results.
                        self.suggest_open = !matches!(key, "enter" | "tab");
                        key == "enter" || key == "tab"
                    } else {
                        match key {
                            "down" => self.suggest_ix = (self.suggest_ix + 1) % n,
                            "up" => self.suggest_ix = (self.suggest_ix + n - 1) % n,
                            _ => {
                                self.add_chip(suggestions[self.suggest_ix.min(n - 1)].chip.clone())
                            }
                        }
                        true
                    }
                }
                "escape" => {
                    self.suggest_open = false;
                    true
                }
                _ => false,
            };
            if handled {
                self.caret_on = true;
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        // With text in the field, ←→/Home/End move the caret, and ↓ reopens the suggestions.
        if self.filter_focused && !self.query.is_empty() && !k.modifiers.control {
            let len = self.query.chars().count();
            let handled = match (k.key.as_str(), k.modifiers.platform) {
                ("left", false) if !k.modifiers.alt => {
                    self.caret_back = (self.caret_back + 1).min(len);
                    true
                }
                ("right", false) if !k.modifiers.alt => {
                    self.caret_back = self.caret_back.saturating_sub(1);
                    true
                }
                ("left", true) | ("home", _) => {
                    self.caret_back = len;
                    true
                }
                ("right", true) | ("end", _) => {
                    self.caret_back = 0;
                    true
                }
                ("down", false) if self.page.filterable() && !self.suggest_open => {
                    self.suggest_open = true;
                    true
                }
                _ => false,
            };
            if handled {
                self.caret_on = true;
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        if self.page == Page::Processes && !k.modifiers.control {
            let nav = match (k.key.as_str(), k.modifiers.platform) {
                ("right", false) if k.modifiers.alt => Some(Nav::ExpandAll),
                _ if k.modifiers.alt => None,
                ("up", true) | ("home", _) => Some(Nav::Home),
                ("down", true) | ("end", _) => Some(Nav::End),
                ("up", false) => Some(Nav::Up),
                ("down", false) => Some(Nav::Down),
                ("pageup", _) => Some(Nav::PageUp),
                ("pagedown", _) => Some(Nav::PageDown),
                ("left", false) => Some(Nav::Left),
                ("right", false) => Some(Nav::Right),
                // Space types into the filter once you have started typing one.
                ("space", false) if self.query.is_empty() && !self.filter_focused => {
                    Some(Nav::Toggle)
                }
                _ => None,
            };
            if let Some(nav) = nav {
                self.column_menu = false;
                self.navigate_processes(nav);
                cx.stop_propagation();
                cx.notify();
                return;
            }
        }
        if k.modifiers.platform || k.modifiers.control {
            return;
        }
        match k.key.as_str() {
            "f12" => {
                Saver::open(cx.entity(), cx);
                return;
            }
            "escape" if self.context_menu.is_some() => {
                self.context_menu = None;
                cx.notify();
                return;
            }
            "escape" => {
                // Esc clears the filter first, and leaves the field once it is empty.
                if self.filter_active() {
                    self.query.clear();
                    self.caret_back = 0;
                    self.active_chips_mut().clear();
                } else {
                    self.blur_filter();
                }
                self.column_menu = false;
                self.confirm_trash = None;
            }
            "backspace" if self.query.is_empty() => {
                if self.active_chips_mut().pop().is_none() {
                    return;
                }
            }
            "backspace" if k.modifiers.alt => {
                // ⌥⌫ deletes the word before the caret.
                let at = self.caret_byte();
                let kept = self.query[..at].trim_end().len();
                let cut = self.query[..kept]
                    .rfind(|c: char| c.is_whitespace() || c == ':' || c == '>')
                    .map_or(0, |i| i + 1);
                self.query.replace_range(cut..at, "");
                self.focus_filter();
            }
            "backspace" => {
                let at = self.caret_byte();
                if let Some((i, _)) = self.query[..at].char_indices().next_back() {
                    self.query.replace_range(i..at, "");
                }
                self.focus_filter();
            }
            _ => match &k.key_char {
                Some(ch) if self.page.filterable() && !ch.chars().any(char::is_control) => {
                    let at = self.caret_byte();
                    self.query.insert_str(at, ch);
                    self.focus_filter();
                }
                _ => return,
            },
        }
        self.suggest_ix = 0;
        self.query_changed();
        cx.notify();
    }

    fn on_mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(r) = &self.resizing else { return };
        if ev.pressed_button != Some(MouseButton::Left) {
            self.resizing = None;
            self.layout_changed(cx);
            return;
        }
        let width = r.start_width + f32::from(ev.position.x - r.start_x);
        self.layout.set_width(r.index, width);
        cx.notify();
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.column_menu_just_closed = false;
        if self.resizing.take().is_some() {
            self.layout_changed(cx);
        }
    }

    fn sidebar(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let s = &self.snapshot;
        let mem = if s.memory_total == 0 {
            0.
        } else {
            s.memory_used as f32 / s.memory_total as f32 * 100.
        };
        ui::glass(t)
            .id("sidebar")
            .w(px(212.))
            .h_full()
            .flex()
            .flex_col()
            .pt(px(50.))
            .px_3()
            .pb_3()
            .gap_0p5()
            .child(
                div()
                    .px_2()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .text_color(t.text)
                            .child("Open Task Manager"),
                    )
                    .child(div().text_xs().text_color(t.text_dim).child(s.host.clone())),
            )
            .children(Page::SECTIONS.iter().map(|(title, pages)| {
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .px_3()
                            .pt_2()
                            .pb_0p5()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(t.text_faint)
                            .child(*title),
                    )
                    .children(pages.iter().map(|&page| {
                        let active = self.page == page;
                        div()
                            .id(page.label())
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py(px(5.))
                            .rounded(px(10.))
                            .cursor_pointer()
                            .text_sm()
                            .when(active, |d| d.bg(t.accent_soft).text_color(t.accent))
                            .when(!active, |d| {
                                d.text_color(t.text).hover(|h| h.bg(t.glass_hover))
                            })
                            .child(div().w(px(16.)).child(page.glyph()))
                            .child(page.label())
                            .when(page == Page::Recorder && self.recording.is_some(), |d| {
                                d.child(div().size(px(7.)).rounded_full().bg(t.hot))
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_page(page, cx);
                                cx.notify();
                            }))
                    }))
            }))
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .px_2()
                    .when(self.paused, |d| {
                        d.child(
                            div()
                                .text_xs()
                                .text_color(t.warn)
                                .child("Updates paused · ⌥⌘P resumes"),
                        )
                    })
                    .child(mini_meter(t, "CPU", s.cpu))
                    .child(mini_meter(t, "Memory", mem))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_1()
                            .pt_1()
                            .child(
                                ui::pill(t, "open-popout", "Popout", false)
                                    .debug_selector(|| "open-popout".into())
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        Popout::open(cx.entity(), cx);
                                    })),
                            )
                            .child(
                                ui::pill(t, "open-saver", "Screensaver", false)
                                    .debug_selector(|| "open-saver".into())
                                    .on_click(cx.listener(|_, _, _, cx| {
                                        Saver::open(cx.entity(), cx);
                                    })),
                            ),
                    ),
            )
    }
}

fn mini_meter(t: &Theme, label: &'static str, pct: f32) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .text_xs()
                .text_color(t.text_dim)
                .child(label)
                .child(crate::metrics::fmt_pct(pct)),
        )
        .child(ui::meter(t, pct, t.load_color(pct)))
}

impl Render for OtmApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::resolve(window.appearance(), &self.settings);
        window.set_rem_size(px(16. * self.settings.text_scale as f32 / 100.));
        if self.titled != Some(self.page) {
            window.set_window_title(&format!("{} — Open Task Manager", self.page.label()));
            self.titled = Some(self.page);
        }
        self.sync_menus(cx);
        let content = match self.page {
            Page::Overview => self.render_overview(&t).into_any_element(),
            Page::Processes => self.render_processes(&t, cx).into_any_element(),
            Page::Users => self
                .render_listing(
                    &t,
                    "Users",
                    "Processes, CPU and memory per account. Group Processes by User for the full list."
                        .into(),
                    cx,
                )
                .into_any_element(),
            Page::Startup => self
                .render_listing(
                    &t,
                    "Startup Items",
                    "Launch agents and daemons. Your own agents can be enabled or disabled.".into(),
                    cx,
                )
                .into_any_element(),
            Page::Services => self
                .render_listing(
                    &t,
                    "Services",
                    "launchd jobs in your session. Restart or stop them.".into(),
                    cx,
                )
                .into_any_element(),
            Page::Connections => self
                .render_listing(
                    &t,
                    "Connections",
                    "Open TCP and UDP sockets per process (lsof)".into(),
                    cx,
                )
                .into_any_element(),
            Page::Apps => self
                .render_listing(
                    &t,
                    "Installed Apps",
                    "Apps in /Applications and ~/Applications. Uninstall moves them to the Trash."
                        .into(),
                    cx,
                )
                .into_any_element(),
            Page::Disk => self.render_disk(&t, cx).into_any_element(),
            Page::Power => self.render_power(&t).into_any_element(),
            Page::Benchmarks => self.render_bench(&t, cx).into_any_element(),
            Page::Recorder => self.render_recorder(&t, cx).into_any_element(),
            Page::Settings => self.render_settings(&t, cx).into_any_element(),
        };
        let root = self.menu_actions(div().id("root"), cx);
        root.track_focus(&self.focus)
            .on_key_down(cx.listener(Self::on_key))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            // A click anywhere outside the filter field leaves it (the field stops this event).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.filter_focused {
                        this.blur_filter();
                        cx.notify();
                    }
                }),
            )
            .size_full()
            .flex()
            .flex_row()
            .gap_3()
            .p_3()
            .bg(t.window_tint)
            .font_family(".SystemUIFont")
            .text_color(t.text)
            .child(self.sidebar(&t, cx))
            .child(div().flex_1().h_full().min_w_0().child(content))
            .when_some(self.confirm.clone(), |d, c| {
                d.child(self.render_confirm(&t, c, cx))
            })
            .when_some(self.demo_caption.clone(), |d, text| {
                // Demo caption: a glass pill centred at the bottom of the window.
                d.child(
                    div()
                        .absolute()
                        .bottom(px(22.))
                        .left_0()
                        .right_0()
                        .flex()
                        .justify_center()
                        .child(
                            div()
                                .px_4()
                                .py_2()
                                .rounded_full()
                                .bg(gpui::rgba(0x0b1230d9))
                                .border_1()
                                .border_color(gpui::rgba(0xffffff33))
                                .shadow_lg()
                                .text_color(gpui::white())
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(text),
                        ),
                )
            })
    }
}
