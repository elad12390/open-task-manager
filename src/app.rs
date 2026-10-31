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

    fn filterable(self) -> bool {
        matches!(
            self,
            Page::Processes | Page::Startup | Page::Services | Page::Connections | Page::Apps
        )
    }
}

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
    pub collapsed: HashSet<String>,
    pub rows: Rc<Vec<Row>>,
    pub resizing: Option<Resize>,
    pub column_menu: bool,
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
    /// Bundles whose icon has been asked for (rendered or failed), so each is rendered once.
    pub icons_requested: HashSet<String>,
    /// Owning app name -> its bundle path (for icons on owner rows).
    pub owner_bundles: std::collections::HashMap<String, String>,
    /// Right-click menu: the row it is for and where it opened.
    pub context_menu: Option<(crate::processes::RowId, gpui::Point<Pixels>)>,
}

impl OtmApp {
    /// Render (once, in the background) the icons of app bundles seen in the latest sample.
    pub fn request_icons(&mut self, cx: &mut Context<Self>) {
        let mut wanted: Vec<String> = Vec::new();
        for p in &self.snapshot.processes {
            if let Some(b) = crate::icons::bundle_of(&p.path) {
                if p.owner_app != crate::categories::OS_OWNER {
                    self.owner_bundles
                        .entry(p.owner_app.clone())
                        .or_insert_with(|| b.clone());
                }
                if self.icons_requested.insert(b.clone()) {
                    wanted.push(b);
                }
            }
        }
        if wanted.is_empty() {
            return;
        }
        self.run_bg(
            cx,
            move || crate::icons::render(&wanted),
            |app, found, _| app.icons.extend(found),
        );
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        if std::env::var_os("OTM_BACKGROUND").is_none() {
            window.focus(&focus);
        }
        let sampler = Arc::new(Mutex::new(Sampler::new()));
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
                        app.rebuild_rows();
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

        let mut app = Self {
            snapshot: Snapshot::default(),
            history: History::default(),
            layout: TableLayout::load(),
            settings: Settings::load(),
            page: Page::Processes,
            query: String::new(),
            collapsed: HashSet::new(),
            rows: Rc::new(Vec::new()),
            resizing: None,
            column_menu: false,
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
            owner_bundles: Default::default(),
            context_menu: None,
        };
        app.run_bg(cx, crate::sysops::system_info, |app, info, _| {
            app.system = info
        });
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

    fn query_changed(&mut self) {
        if self.page == Page::Processes {
            self.rebuild_rows();
        } else {
            self.rebuild_listing();
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let k = &ev.keystroke;
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
                ("space", false) if self.query.is_empty() => Some(Nav::Toggle),
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
                self.query.clear();
                self.column_menu = false;
                self.confirm_trash = None;
            }
            "backspace" => {
                self.query.pop();
            }
            _ => match &k.key_char {
                Some(ch) if self.page.filterable() && !ch.chars().any(char::is_control) => {
                    self.query.push_str(ch)
                }
                _ => return,
            },
        }
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
                                .child("Updates paused · ⌘P resumes"),
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
                            .child(ui::pill(t, "open-popout", "Popout", false).on_click(
                                cx.listener(|_, _, _, cx| {
                                    Popout::open(cx.entity(), cx);
                                }),
                            ))
                            .child(ui::pill(t, "open-saver", "Screensaver", false).on_click(
                                cx.listener(|_, _, _, cx| {
                                    Saver::open(cx.entity(), cx);
                                }),
                            )),
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
                .child(format!("{pct:.0}%")),
        )
        .child(ui::meter(t, pct, t.load_color(pct)))
}

impl Render for OtmApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = Theme::resolve(window.appearance(), &self.settings);
        window.set_rem_size(px(16. * self.settings.text_scale as f32 / 100.));
        self.sync_menus(cx);
        let content = match self.page {
            Page::Overview => self.render_overview(&t).into_any_element(),
            Page::Processes => self.render_processes(&t, cx).into_any_element(),
            Page::Users => self.render_users(&t).into_any_element(),
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
    }
}
