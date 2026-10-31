use std::{collections::HashSet, ops::Range, path::PathBuf, rc::Rc};

use gpui::{
    AnyElement, ClipboardItem, Context, Div, FontWeight, SharedString, Stateful, div, img,
    prelude::*, px, uniform_list,
};

use crate::{
    app::{OtmApp, Page},
    holidays::{Holiday, HolidayMode},
    icons,
    metrics::{fmt_bytes, fmt_duration, fmt_size},
    processes::Nav,
    recorder,
    scrollbar::{self, Axis},
    settings::{Appearance, GLASS_SKIN, Glass, skins},
    sysops::{self, ServiceAction},
    tables::{Col, cell},
    theme::Theme,
    ui,
};

#[derive(Clone, Debug)]
pub enum Action {
    Load(PathBuf),
    Unload(PathBuf),
    Restart(String),
    Stop(String),
    AskTrash(PathBuf),
    Trash(PathBuf),
    CancelTrash,
    Reveal(String),
    /// Several actions run in turn, reported as one notice (`what`) or as the failures.
    Batch {
        what: String,
        actions: Vec<Action>,
    },
}

pub struct ActionButton {
    pub label: &'static str,
    pub action: Action,
    pub danger: bool,
}

/// The 16 px icon and its gap, before a listing's first cell.
const LIST_ICON_LEAD: f32 = 20.;

pub struct ListRow {
    /// Stable identity of the row (its file, path, label or endpoint), for the selection.
    pub key: String,
    pub cells: Vec<SharedString>,
    /// One sort key per cell.
    pub keys: Vec<crate::tables::SortKey>,
    pub dim: bool,
    pub actions: Vec<ActionButton>,
    /// The icon key (see `icons`) drawn before the first cell.
    pub icon: Option<String>,
}

pub struct Listing {
    pub headers: Vec<Col>,
    pub rows: Rc<Vec<ListRow>>,
    /// Rows before the filter.
    pub total: usize,
    /// Every row's cells before the filter, for the filter's suggestions.
    pub cells: Vec<Vec<SharedString>>,
    /// Whether the rows have icons, so the first cell reserves room for one.
    pub icons: bool,
}

impl Default for Listing {
    fn default() -> Self {
        Self {
            headers: Vec::new(),
            rows: Rc::new(Vec::new()),
            total: 0,
            cells: Vec::new(),
            icons: false,
        }
    }
}

fn btn(label: &'static str, action: Action) -> ActionButton {
    ActionButton {
        label,
        action,
        danger: false,
    }
}

fn danger(label: &'static str, action: Action) -> ActionButton {
    ActionButton {
        label,
        action,
        danger: true,
    }
}

/// Run one action (off the main thread); a batch runs every action and reports how many failed.
fn run_action(action: Action) -> Result<(), String> {
    match action {
        Action::Load(p) => sysops::toggle_startup(&p, true),
        Action::Unload(p) => sysops::toggle_startup(&p, false),
        Action::Restart(l) => sysops::control_service(&l, ServiceAction::Restart),
        Action::Stop(l) => sysops::control_service(&l, ServiceAction::Stop),
        Action::Trash(p) => sysops::trash_app(&p),
        Action::Batch { actions, .. } => {
            let total = actions.len();
            let errors: Vec<String> = actions
                .into_iter()
                .filter_map(|a| run_action(a).err())
                .collect();
            match errors.first() {
                None => Ok(()),
                Some(first) => Err(format!("{} of {total} failed ({first})", errors.len())),
            }
        }
        _ => Ok(()),
    }
}

impl OtmApp {
    /// Run `work` off the main thread, then apply its result on the UI thread.
    pub fn run_bg<R: Send + 'static>(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce() -> R + Send + 'static,
        done: impl FnOnce(&mut Self, R, &mut Context<Self>) + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let r = cx.background_executor().spawn(async move { work() }).await;
            this.update(cx, |app, cx| {
                done(app, r, cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn open_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        self.query.clear();
        self.list_chips.clear();
        self.caret_back = 0;
        self.listing_loading = !self.demo_mode
            && matches!(
                page,
                Page::Startup | Page::Services | Page::Connections | Page::Apps
            );
        self.rebuild_listing();
        self.blur_filter();
        self.column_menu = false;
        self.confirm_trash = None;
        self.clear_list_selection();
        self.load_page(cx);
    }

    pub fn load_page(&mut self, cx: &mut Context<Self>) {
        if self.demo_mode {
            // Demo mode shows its fictional data only; nothing is read from this Mac.
            self.rebuild_listing();
            return;
        }
        match self.page {
            Page::Startup => self.run_bg(cx, sysops::startup_items, |app, v, _| {
                app.listing_loading = false;
                app.startup = v;
                app.rebuild_listing();
            }),
            Page::Services => self.run_bg(cx, sysops::services, |app, v, _| {
                app.listing_loading = false;
                app.services = v;
                app.rebuild_listing();
            }),
            Page::Connections => self.run_bg(cx, sysops::connections, |app, v, _| {
                app.listing_loading = false;
                app.connections = v;
                app.rebuild_listing();
            }),
            Page::Apps => {
                self.run_bg(cx, sysops::installed_apps, |app, v, cx| {
                    let paths: Vec<PathBuf> = v.iter().map(|a| a.path.clone()).collect();
                    app.listing_loading = false;
                    app.apps = v;
                    app.rebuild_listing();
                    app.run_bg(
                        cx,
                        move || sysops::app_sizes(&paths),
                        |app, sizes, _| {
                            for (a, s) in app.apps.iter_mut().zip(sizes) {
                                a.bytes = s;
                            }
                            app.rebuild_listing();
                        },
                    );
                });
            }
            // Built from the live snapshot, so it is ready at once and refreshes with it.
            Page::Users => self.rebuild_listing(),
            Page::Power => self.run_bg(cx, sysops::power, |app, v, _| app.power = Some(v)),
            Page::Disk => {
                if self.scans.is_empty() {
                    self.scans = sysops::load_scans();
                }
            }
            Page::Recorder => self.recordings = recorder::list(),
            _ => {}
        }
    }

    pub fn perform(&mut self, action: Action, cx: &mut Context<Self>) {
        match action {
            Action::AskTrash(p) => {
                self.confirm_trash = Some(p);
                self.rebuild_listing();
                cx.notify();
                return;
            }
            Action::CancelTrash => {
                self.confirm_trash = None;
                self.rebuild_listing();
                cx.notify();
                return;
            }
            Action::Reveal(p) => {
                sysops::reveal_in_finder(&p);
                return;
            }
            _ => {}
        }
        self.confirm_trash = None;
        let describe = match &action {
            Action::Load(p) => format!(
                "Loaded {}",
                p.file_stem().unwrap_or_default().to_string_lossy()
            ),
            Action::Unload(p) => format!(
                "Unloaded {}",
                p.file_stem().unwrap_or_default().to_string_lossy()
            ),
            Action::Restart(l) => format!("Restarted {l}"),
            Action::Stop(l) => format!("Stopped {l}"),
            Action::Trash(p) => format!(
                "Moved {} to the Trash",
                p.file_name().unwrap_or_default().to_string_lossy()
            ),
            Action::Batch { what, .. } => what.clone(),
            _ => String::new(),
        };
        self.run_bg(
            cx,
            move || run_action(action),
            move |app, result, cx| {
                app.notice = Some(match result {
                    Ok(()) => (describe, false),
                    Err(e) => (e, true),
                });
                if app.page == Page::Processes {
                    app.set_cursor(None);
                } else {
                    app.load_page(cx);
                }
            },
        );
    }

    /// The selected listing rows that are on screen, in list order.
    pub fn selected_list_rows(&self) -> Vec<&ListRow> {
        self.listing
            .rows
            .iter()
            .filter(|r| self.list_selection.contains(&r.key))
            .collect()
    }

    pub fn is_listing(&self) -> bool {
        !Self::listing_table(self.page).is_empty()
    }

    fn clear_list_selection(&mut self) {
        self.list_selection.clear();
        self.list_cursor = None;
        self.list_anchor = None;
        self.bulk_trash = None;
    }

    fn set_list_cursor(&mut self, key: String) {
        self.list_selection = HashSet::from([key.clone()]);
        self.list_anchor = Some(key.clone());
        self.list_cursor = Some(key);
    }

    fn list_position(&self, key: &str) -> Option<usize> {
        self.listing.rows.iter().position(|r| r.key == key)
    }

    fn extend_list_selection(&mut self, key: String) {
        let from = self
            .list_anchor
            .clone()
            .or_else(|| self.list_cursor.clone())
            .unwrap_or_else(|| key.clone());
        let (Some(a), Some(b)) = (self.list_position(&from), self.list_position(&key)) else {
            return self.set_list_cursor(key);
        };
        let (lo, hi) = (a.min(b), a.max(b));
        self.list_selection = self.listing.rows[lo..=hi]
            .iter()
            .map(|r| r.key.clone())
            .collect();
        self.list_cursor = Some(key);
    }

    fn toggle_list_row(&mut self, key: String) {
        if !self.list_selection.remove(&key) {
            self.list_selection.insert(key.clone());
        }
        self.list_anchor = Some(key.clone());
        self.list_cursor = Some(key);
    }

    /// A click on a listing row, with the same modifiers as the process table.
    fn click_list_row(&mut self, key: String, m: gpui::Modifiers) {
        if m.shift {
            self.extend_list_selection(key);
        } else if m.platform || m.control {
            self.toggle_list_row(key);
        } else {
            self.set_list_cursor(key);
        }
    }

    /// Arrow keys on a listing move its cursor and select that row; Shift extends the range.
    pub fn move_list_cursor(&mut self, nav: Nav, extend: bool) {
        let rows = self.listing.rows.clone();
        if rows.is_empty() {
            return;
        }
        let last = rows.len() - 1;
        let at = self
            .list_cursor
            .as_ref()
            .and_then(|k| self.list_position(k));
        let page = ((f32::from(
            self.listing_scroll
                .0
                .borrow()
                .base_handle
                .bounds()
                .size
                .height,
        ) / 30.) as usize)
            .saturating_sub(1)
            .max(1);
        let target = match (at, nav) {
            (None, Nav::Up | Nav::End) => last,
            (None, _) => 0,
            (Some(p), Nav::Up) => p.saturating_sub(1),
            (Some(p), Nav::Down) => (p + 1).min(last),
            (Some(p), Nav::PageUp) => p.saturating_sub(page),
            (Some(p), Nav::PageDown) => (p + page).min(last),
            (Some(_), Nav::Home) => 0,
            (Some(_), Nav::End) => last,
            _ => return,
        };
        let key = rows[target].key.clone();
        if extend {
            self.extend_list_selection(key);
        } else {
            self.set_list_cursor(key);
        }
        let (first, shown) = {
            let state = self.listing_scroll.0.borrow();
            let handle = &state.base_handle;
            let top = (-f32::from(handle.offset().y) / 30.).max(0.);
            let shown = (f32::from(handle.bounds().size.height) / 30.).floor();
            (top.ceil() as usize, shown as usize)
        };
        if shown == 0 {
            self.listing_scroll
                .scroll_to_item(target, gpui::ScrollStrategy::Center);
        } else if target < first {
            self.listing_scroll
                .scroll_to_item(target, gpui::ScrollStrategy::Top);
        } else if target + 1 > first + shown.saturating_sub(1) {
            self.listing_scroll
                .scroll_to_item(target, gpui::ScrollStrategy::Bottom);
        }
    }

    pub fn select_all_list(&mut self) {
        self.list_selection = self.listing.rows.iter().map(|r| r.key.clone()).collect();
    }

    /// Esc with several listing rows selected: keep only the cursor row. Returns whether it did.
    pub fn collapse_list_selection(&mut self) -> bool {
        if self.list_selection.len() < 2 {
            return false;
        }
        match self.list_cursor.clone() {
            Some(key) => self.set_list_cursor(key),
            None => self.clear_list_selection(),
        }
        true
    }

    /// Copy the selected listing rows, one line per row (tab-separated cells).
    pub fn copy_list_rows(&mut self, cx: &mut Context<Self>) {
        let rows = self.selected_list_rows();
        if rows.is_empty() {
            return;
        }
        let n = rows.len();
        let text = rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect::<Vec<_>>()
            .join("\n");
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.notice = Some((
            format!("Copied {n} {}", if n == 1 { "row" } else { "rows" }),
            false,
        ));
        cx.notify();
    }

    /// The table id a listing page keeps its sort under.
    pub fn listing_table(page: Page) -> &'static str {
        match page {
            Page::Users => "users",
            Page::Startup => "startup",
            Page::Services => "services",
            Page::Connections => "connections",
            Page::Apps => "apps",
            _ => "",
        }
    }

    pub fn rebuild_listing(&mut self) {
        use crate::tables::{SortKey as K, num, text};
        let row = |key: String,
                   icon: Option<String>,
                   cells: Vec<String>,
                   keys: Vec<K>,
                   dim: bool,
                   actions: Vec<ActionButton>| ListRow {
            key,
            cells: cells.into_iter().map(SharedString::from).collect(),
            keys,
            dim,
            actions,
            icon,
        };
        let by_pid: std::collections::HashMap<u32, &crate::metrics::ProcessInfo> =
            self.snapshot.processes.iter().map(|p| (p.pid, p)).collect();
        let pid_icon = |pid: u32| {
            by_pid.get(&pid).and_then(|p| {
                icons::process_key(&p.name, &p.path, &p.owner_app, &self.owner_bundles)
            })
        };
        let (headers, rows): (Vec<Col>, Vec<ListRow>) = match self.page {
            Page::Users => {
                let mut users: std::collections::HashMap<&str, (usize, f32, u64)> =
                    Default::default();
                for p in &self.snapshot.processes {
                    let e = users.entry(p.user.as_str()).or_default();
                    e.0 += 1;
                    e.1 += p.cpu;
                    e.2 += p.memory;
                }
                let total = self.snapshot.memory_total.max(1) as f64;
                let mut users: Vec<_> = users.into_iter().collect();
                users.sort_by_key(|u| std::cmp::Reverse(u.1.2));
                (
                    vec![
                        text("User", 220.),
                        num("Processes", 100.),
                        num("CPU", 90.),
                        num("Memory", 110.),
                        num("Share of Memory", 140.),
                    ],
                    users
                        .into_iter()
                        .map(|(name, (count, cpu, memory))| {
                            let share = memory as f64 / total * 100.;
                            row(
                                name.to_string(),
                                Some(icons::USER_ICON.to_string()),
                                vec![
                                    name.to_string(),
                                    count.to_string(),
                                    format!("{cpu:.1}%"),
                                    fmt_bytes(memory),
                                    format!("{share:.1}%"),
                                ],
                                vec![
                                    K::text(name),
                                    K::num(count as f64),
                                    K::num(cpu),
                                    K::num(memory as f64),
                                    K::num(share),
                                ],
                                false,
                                vec![],
                            )
                        })
                        .collect(),
                )
            }
            Page::Startup => (
                vec![
                    text("Label", 380.),
                    text("Scope", 120.),
                    text("State", 100.),
                    text("File", 420.),
                ],
                self.startup
                    .iter()
                    .map(|s| {
                        let actions = if !s.editable {
                            vec![]
                        } else if s.loaded {
                            vec![btn("Disable", Action::Unload(s.file.clone()))]
                        } else {
                            vec![btn("Enable", Action::Load(s.file.clone()))]
                        };
                        let state = if s.loaded { "Loaded" } else { "Not loaded" };
                        let file = s.file.display().to_string();
                        row(
                            file.clone(),
                            Some(icons::path_key(
                                &s.program.as_ref().unwrap_or(&s.file).to_string_lossy(),
                            )),
                            vec![s.label.clone(), s.scope.into(), state.into(), file.clone()],
                            vec![
                                K::text(&s.label),
                                K::text(s.scope),
                                K::text(state),
                                K::text(&file),
                            ],
                            !s.loaded,
                            actions,
                        )
                    })
                    .collect(),
            ),
            Page::Services => (
                vec![
                    text("Label", 460.),
                    num("PID", 90.),
                    num("Last Exit", 90.),
                    text("State", 100.),
                ],
                self.services
                    .iter()
                    .map(|s| {
                        let mut actions = vec![btn("Restart", Action::Restart(s.label.clone()))];
                        if s.pid.is_some() {
                            actions.push(danger("Stop", Action::Stop(s.label.clone())));
                        }
                        let state = if s.pid.is_some() { "Running" } else { "Idle" };
                        row(
                            s.label.clone(),
                            s.pid.and_then(pid_icon).or_else(|| {
                                s.program
                                    .as_ref()
                                    .map(|p| icons::path_key(&p.to_string_lossy()))
                            }),
                            vec![
                                s.label.clone(),
                                s.pid.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
                                s.status.to_string(),
                                state.into(),
                            ],
                            vec![
                                K::text(&s.label),
                                K::num(s.pid.map_or(-1., f64::from)),
                                K::num(s.status as f64),
                                K::text(state),
                            ],
                            s.pid.is_none(),
                            actions,
                        )
                    })
                    .collect(),
            ),
            Page::Connections => (
                vec![
                    text("Process", 200.),
                    num("PID", 80.),
                    text("Protocol", 90.),
                    text("Endpoint", 460.),
                    text("State", 120.),
                ],
                self.connections
                    .iter()
                    .map(|c| {
                        row(
                            format!("{} {} {}", c.pid, c.proto, c.endpoint),
                            pid_icon(c.pid).or_else(|| {
                                icons::process_key(&c.process, "", "", &self.owner_bundles)
                            }),
                            vec![
                                c.process.clone(),
                                c.pid.to_string(),
                                c.proto.clone(),
                                c.endpoint.clone(),
                                c.state.clone(),
                            ],
                            vec![
                                K::text(&c.process),
                                K::num(c.pid),
                                K::text(&c.proto),
                                K::text(&c.endpoint),
                                K::text(&c.state),
                            ],
                            c.state.is_empty(),
                            vec![],
                        )
                    })
                    .collect(),
            ),
            Page::Apps => (
                vec![
                    text("Name", 280.),
                    text("Version", 120.),
                    num("Size", 100.),
                    text("Location", 360.),
                ],
                self.apps
                    .iter()
                    .map(|a| {
                        let path = a.path.display().to_string();
                        let actions = if self.confirm_trash.as_ref() == Some(&a.path) {
                            vec![
                                danger("Move to Trash", Action::Trash(a.path.clone())),
                                btn("Cancel", Action::CancelTrash),
                            ]
                        } else {
                            vec![
                                btn("Reveal", Action::Reveal(path.clone())),
                                danger("Uninstall", Action::AskTrash(a.path.clone())),
                            ]
                        };
                        row(
                            path.clone(),
                            Some(icons::path_key(&a.path.to_string_lossy())),
                            vec![
                                a.name.clone(),
                                a.version.clone(),
                                a.bytes.map(fmt_size).unwrap_or_else(|| "…".into()),
                                path.clone(),
                            ],
                            vec![
                                K::text(&a.name),
                                K::text(&a.version),
                                K::num(a.bytes.map_or(-1., |b| b as f64)),
                                K::text(&path),
                            ],
                            false,
                            actions,
                        )
                    })
                    .collect(),
            ),
            _ => (vec![], vec![]),
        };
        // The filter searches every column and applies the column chips; the chosen sort applies on
        // top of each page's order.
        let total = rows.len();
        let cells = rows.iter().map(|r| r.cells.clone()).collect();
        let titles: Vec<&str> = headers.iter().map(|h| h.title).collect();
        let has_icons = rows.iter().any(|r| r.icon.is_some());
        let mut rows: Vec<ListRow> = rows
            .into_iter()
            .filter(|r| {
                crate::filter::listing_matches(&self.list_chips, &self.query, &titles, &r.cells)
            })
            .collect();
        crate::tables::sort_items(
            &mut rows,
            self.table_sort(Self::listing_table(self.page)),
            |r, c| r.keys.get(c).cloned().unwrap_or(K::Num(0.)),
        );
        self.listing = Listing {
            headers,
            rows: Rc::new(rows),
            total,
            cells,
            icons: has_icons,
        };
    }

    fn page_frame(&self, t: &Theme, title: &'static str, subtitle: String) -> Div {
        ui::glass(t)
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .px_4()
                    .pt_4()
                    .pb_3()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_xl().font_weight(FontWeight::BOLD).child(title))
                            .child(div().text_xs().text_color(t.text_dim).child(subtitle)),
                    )
                    .child(div().flex_1())
                    .when_some(self.notice.clone(), |d, (msg, err)| {
                        d.child(
                            div()
                                .px_3()
                                .py_1()
                                .rounded_full()
                                .text_xs()
                                // Fixed dark fills: white text stays readable in light and dark.
                                .bg(if err {
                                    gpui::rgb(0xc4001a)
                                } else {
                                    gpui::rgb(0x1e7b34)
                                })
                                .text_color(gpui::white())
                                .child(crate::filter_bar::notice_text(&msg, err)),
                        )
                    }),
            )
    }

    fn action_button(
        &self,
        t: &Theme,
        id: (SharedString, usize),
        b: &ActionButton,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let action = b.action.clone();
        div()
            .id(id)
            .px_2()
            .py(px(2.))
            .rounded_full()
            .text_xs()
            .cursor_pointer()
            .border_1()
            .when(b.danger, |d| d.border_color(t.hot).text_color(t.hot))
            .when(!b.danger, |d| d.border_color(t.accent).text_color(t.accent))
            .hover(|h| h.bg(t.glass_hover))
            .child(b.label)
            .on_click(cx.listener(move |this, _, _, cx| {
                // A button in a row acts on that row without also selecting it.
                cx.stop_propagation();
                this.perform(action.clone(), cx)
            }))
    }

    pub fn render_listing(
        &mut self,
        t: &Theme,
        title: &'static str,
        subtitle: String,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = *t;
        let count = self.listing.rows.len();
        let has_actions = self.listing.rows.iter().any(|r| !r.actions.is_empty());
        self.page_frame(t, title, subtitle)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(self.page_filter(t, cx))
                    .child(div().mb_2().text_xs().text_color(t.text_dim).child(
                        if self.listing_loading {
                            "Loading…".to_string()
                        } else if !self.filter_active() {
                            format!("{count} shown")
                        } else {
                            format!("{count} of {} shown", self.listing.total)
                        },
                    ))
                    .children(self.bulk_actions(t, cx))
                    .child(div().flex_1())
                    .when(self.page != Page::Users, |d| {
                        d.child(
                            div().mb_2().mr_4().child(
                                ui::pill(t, "refresh", "↻ Refresh", false)
                                    .on_click(cx.listener(|this, _, _, cx| this.load_page(cx))),
                            ),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .h(px(28.))
                    .items_center()
                    .px_3()
                    .border_b_1()
                    .border_color(t.stroke)
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text_dim)
                    .child(
                        self.sort_header(
                            t,
                            Self::listing_table(self.page),
                            &self.listing.headers,
                            true,
                            cx,
                        )
                        .flex_1()
                        .min_w_0(),
                    )
                    .when(has_actions, |d| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .w(px(ACTIONS_W))
                                .px_2()
                                .child("Actions"),
                        )
                    }),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            "listing",
                            count,
                            cx.processor(move |this, range: Range<usize>, _, cx| {
                                let rows = this.listing.rows.clone();
                                let widths: Vec<f32> =
                                    this.listing.headers.iter().map(|h| h.width).collect();
                                range
                                    .filter_map(|ix| {
                                        let r = rows.get(ix)?;
                                        Some(this.render_list_row(&theme, ix, r, &widths, cx))
                                    })
                                    .collect::<Vec<_>>()
                            }),
                        )
                        .flex_1()
                        .py_1()
                        .track_scroll(self.listing_scroll.clone()),
                    )
                    .when_some(
                        scrollbar::scrollbar(
                            t,
                            "listing-vbar",
                            Axis::Vertical,
                            &scrollbar::of_list(&self.listing_scroll),
                            cx,
                        ),
                        |d, bar| d.child(bar),
                    ),
            )
            .when(!self.listing_loading && count == 0, |d| {
                d.child(self.listing_empty(t, cx))
            })
            .when_some(self.bulk_trash.clone(), |d, paths| {
                d.child(self.trash_sheet(t, paths, cx))
            })
    }

    /// Bulk buttons for two or more selected rows: each row's own action with the same label
    /// (Enable, Disable, Restart, Stop) applied to all of them, plus Copy or Move to Trash.
    fn bulk_actions(&self, t: &Theme, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let rows = self.selected_list_rows();
        if rows.len() < 2 {
            return vec![];
        }
        let n = rows.len();
        let mut groups: Vec<(&'static str, bool, Vec<Action>)> = Vec::new();
        for b in rows.iter().flat_map(|r| r.actions.iter()) {
            if matches!(b.label, "Reveal" | "Uninstall" | "Move to Trash" | "Cancel") {
                continue;
            }
            match groups.iter_mut().find(|g| g.0 == b.label) {
                Some(g) => g.2.push(b.action.clone()),
                None => groups.push((b.label, b.danger, vec![b.action.clone()])),
            }
        }
        let noun = match self.page {
            Page::Startup => "startup items",
            Page::Services => "services",
            _ => "items",
        };
        let mut out: Vec<AnyElement> = Vec::new();
        for (i, (label, danger_btn, actions)) in groups.into_iter().enumerate() {
            let count = actions.len();
            let past = match label {
                "Enable" => "Enabled",
                "Disable" => "Disabled",
                "Restart" => "Restarted",
                "Stop" => "Stopped",
                other => other,
            };
            let batch = Action::Batch {
                what: format!("{past} {count} {noun}"),
                actions,
            };
            out.push(
                ui::pill(t, ("bulk", i), format!("{label} {count}"), false)
                    .debug_selector(move || format!("bulk-{label}"))
                    .flex_shrink_0()
                    .ml_2()
                    .when(danger_btn, |d| d.text_color(t.hot))
                    .on_click(cx.listener(move |this, _, _, cx| this.perform(batch.clone(), cx)))
                    .into_any_element(),
            );
        }
        if self.page == Page::Apps {
            let paths: Vec<PathBuf> = rows.iter().map(|r| PathBuf::from(&r.key)).collect();
            out.push(
                ui::pill(t, "bulk-trash", format!("Move {n} to Trash…"), false)
                    .debug_selector(|| "bulk-trash".into())
                    .flex_shrink_0()
                    .ml_2()
                    .text_color(t.hot)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.bulk_trash = Some(paths.clone());
                        cx.notify();
                    }))
                    .into_any_element(),
            );
        }
        if matches!(self.page, Page::Users | Page::Connections) {
            out.push(
                ui::pill(t, "bulk-copy", format!("Copy {n}"), false)
                    .debug_selector(|| "bulk-copy".into())
                    .flex_shrink_0()
                    .ml_2()
                    .on_click(cx.listener(|this, _, _, cx| this.copy_list_rows(cx)))
                    .into_any_element(),
            );
        }
        out
    }

    /// One confirmation for moving several apps to the Trash, naming them.
    fn trash_sheet(
        &self,
        t: &Theme,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let names: Vec<String> = paths
            .iter()
            .map(|p| {
                p.file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        let mut shown = names.iter().take(6).cloned().collect::<Vec<_>>().join(", ");
        if names.len() > 6 {
            shown.push_str(&format!(" and {} more", names.len() - 6));
        }
        let n = paths.len();
        let trash = paths.clone();
        div()
            .id("trash-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x00000052))
            .occlude()
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("trash-sheet")
                    .debug_selector(|| "trash-sheet".into())
                    .w(px(400.))
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded(px(16.))
                    .bg(crate::theme::with_alpha(t.glass_strong, 0.98))
                    .border_1()
                    .border_color(t.highlight)
                    .shadow_lg()
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .child(format!("Move {n} apps to the Trash?")),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(t.text_dim)
                            .child(format!("{shown}. You can put them back from the Trash.")),
                    )
                    .child(
                        div()
                            .mt_3()
                            .flex()
                            .flex_row()
                            .justify_end()
                            .gap_2()
                            .child(
                                ui::pill(t, "trash-cancel", "Cancel", true)
                                    .debug_selector(|| "trash-cancel".into())
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.bulk_trash = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                ui::pill(t, "trash-confirm", "Move to Trash", false)
                                    .debug_selector(|| "trash-confirm".into())
                                    .text_color(t.hot)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.bulk_trash = None;
                                        let actions = trash
                                            .iter()
                                            .map(|p| Action::Trash(p.clone()))
                                            .collect();
                                        this.perform(
                                            Action::Batch {
                                                what: format!("Moved {n} apps to the Trash"),
                                                actions,
                                            },
                                            cx,
                                        );
                                    })),
                            ),
                    ),
            )
    }

    /// What an empty listing says: nothing matches the filter (with a way out), or nothing exists.
    fn listing_empty(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let filtered = self.filter_active();
        let what = match self.page {
            Page::Users => "No users",
            Page::Startup => "No startup items",
            Page::Services => "No services",
            Page::Connections => "No open connections",
            Page::Apps => "No apps found",
            _ => "Nothing to show",
        };
        div()
            .absolute()
            .top(px(200.))
            .left_0()
            .right_0()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .child(div().text_base().font_weight(FontWeight::SEMIBOLD).child(
                if !self.query.trim().is_empty() {
                    format!("Nothing matches “{}”", self.query.trim())
                } else if filtered {
                    "Nothing matches the filter".to_string()
                } else {
                    what.to_string()
                },
            ))
            .when(filtered, |d| {
                d.child(
                    ui::pill(t, "listing-clear", "Clear filter", true)
                        .debug_selector(|| "listing-clear".into())
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.clear_filter();
                            cx.notify();
                        })),
                )
            })
    }

    /// A 16 px icon, or an empty slot of the same size while it renders, so the text stays aligned.
    pub(crate) fn list_icon(&self, key: Option<&str>) -> AnyElement {
        match key.and_then(|k| self.icons.get(k)) {
            Some(path) => img(path.clone())
                .size(px(16.))
                .flex_shrink_0()
                .into_any_element(),
            None => div().w(px(16.)).flex_shrink_0().into_any_element(),
        }
    }

    fn render_list_row(
        &self,
        t: &Theme,
        ix: usize,
        r: &ListRow,
        widths: &[f32],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_actions = self.listing.rows.iter().any(|r| !r.actions.is_empty());
        let selected = self.list_selection.contains(&r.key);
        let key = r.key.clone();
        div()
            .id(("list-row", ix))
            .debug_selector(move || format!("list-row-{ix}"))
            .w_full()
            .h(px(30.))
            .mx_2()
            .px_1()
            .flex()
            .flex_row()
            .items_center()
            .rounded(px(8.))
            .text_sm()
            .when(ix % 2 == 1 && !selected, |d| d.bg(t.row_alt))
            .when(selected, |d| d.bg(t.selection).text_color(gpui::white()))
            .when(!selected, |d| d.hover(|h| h.bg(t.glass_hover)))
            .when(r.dim && !selected, |d| d.text_color(t.text_dim))
            .on_click(cx.listener(move |this, ev: &gpui::ClickEvent, window, cx| {
                this.click_list_row(key.clone(), ev.modifiers());
                window.focus(&this.focus);
                cx.notify();
            }))
            .children(r.cells.iter().zip(widths).enumerate().map(|(i, (c, w))| {
                let last = i + 1 == widths.len();
                let header = self.listing.headers.get(i);
                let numeric = header.is_some_and(|h| h.numeric);
                let lead = i == 0 && self.listing.icons;
                let full = c.to_string();
                // The last column fills what the list has left after the others and the actions.
                let room = if last {
                    let list = f32::from(
                        self.listing_scroll
                            .0
                            .borrow()
                            .base_handle
                            .bounds()
                            .size
                            .width,
                    );
                    let others: f32 = widths[..i].iter().sum();
                    let actions = if has_actions { ACTIONS_W } else { 0. };
                    if list > 0. {
                        list - others - actions - 24.
                    } else {
                        *w
                    }
                } else {
                    *w
                } - 16.
                    - if lead { LIST_ICON_LEAD } else { 0. };
                // Paths and endpoints keep their end; everything else ends in "…".
                let shown = if header
                    .is_some_and(|h| matches!(h.title, "File" | "Location" | "Endpoint"))
                {
                    crate::ui::ellipsize_middle(cx, full.clone(), room, self.text_px())
                } else {
                    crate::ui::ellipsize(cx, full.clone(), room, self.text_px())
                };
                let cut = shown != full;
                cell(last, *w)
                    .id(("list-cell", ix * 16 + i))
                    .whitespace_nowrap()
                    .when(numeric, |d| d.text_right().font_family("Menlo").text_xs())
                    .when(i > 0 && !selected, |d| d.text_color(t.text_dim))
                    .when(lead, |d| {
                        d.flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(self.list_icon(r.icon.as_deref()))
                    })
                    .child(shown)
                    .when(cut, |d| d.tooltip(crate::ui::tip(full)))
            }))
            .when(
                self.listing.rows.iter().any(|r| !r.actions.is_empty()),
                |d| {
                    d.child(
                        div()
                            .flex_shrink_0()
                            .w(px(ACTIONS_W))
                            .flex()
                            .flex_row()
                            .gap_1()
                            .px_2()
                            .children(r.actions.iter().enumerate().map(|(i, b)| {
                                self.action_button(
                                    t,
                                    (SharedString::from(format!("act-{ix}")), i),
                                    b,
                                    cx,
                                )
                            })),
                    )
                },
            )
            .into_any_element()
    }

    pub fn render_disk(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let disks = self.snapshot.disks.clone();
        let latest = self.scans.first().cloned();
        let max = latest
            .as_ref()
            .and_then(|s| s.items.first())
            .map(|i| i.bytes)
            .unwrap_or(1)
            .max(1);
        use crate::tables::{SortKey as K, sort_items};
        let mut folders: Vec<sysops::FolderSize> = latest
            .iter()
            .flat_map(|s| s.items.iter().cloned())
            .filter(|f| {
                crate::filter::listing_matches(
                    &self.list_chips,
                    &self.query,
                    &["Folder"],
                    &[f.path.as_str()],
                )
            })
            .collect();
        sort_items(&mut folders, self.table_sort("folders"), |f, c| {
            if c == 0 {
                K::text(&f.path)
            } else {
                K::num(f.bytes as f64)
            }
        });
        let mut scans: Vec<(u64, usize, u64)> = self
            .scans
            .iter()
            .map(|s| (s.at, s.items.len(), s.items.iter().map(|i| i.bytes).sum()))
            .collect();
        sort_items(&mut scans, self.table_sort("scans"), |s, c| {
            K::num(match c {
                0 => s.0 as f64,
                1 => s.1 as f64,
                _ => s.2 as f64,
            })
        });
        self.page_frame(
            t,
            "Disk Space",
            if self.scanning {
                "Scanning your home folder (up to 5 minutes)…".into()
            } else {
                format!("{} saved scans", self.scans.len())
            },
        )
        .child(self.page_filter(t, cx))
        .child(scrollbar::scroll_area(
            t,
            "disk-scroll",
            "disk-vbar",
            &self.disk_scroll,
            div()
                .px_4()
                .pb_4()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap_3()
                        .children(disks.iter().map(|d| {
                            let used = d.total.saturating_sub(d.available);
                            let pct = if d.total == 0 {
                                0.
                            } else {
                                used as f32 / d.total as f32 * 100.
                            };
                            card(t)
                                .w(px(260.))
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(d.mount.clone()),
                                )
                                .child(div().text_xs().text_color(t.text_dim).child(format!(
                                    "{} used of {} · {} free",
                                    fmt_size(used),
                                    fmt_size(d.total),
                                    fmt_size(d.available)
                                )))
                                .child(ui::meter(t, pct, t.load_color(pct)))
                        })),
                )
                .child(
                    card(t)
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .child(
                                    div()
                                        .flex_1()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child("Largest folders in your home"),
                                )
                                .child(
                                    ui::pill(
                                        t,
                                        "scan",
                                        if self.scanning {
                                            "Scanning…"
                                        } else {
                                            "Scan now"
                                        },
                                        self.scanning,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            if this.scanning {
                                                return;
                                            }
                                            this.scanning = true;
                                            this.run_bg(cx, sysops::scan_home, |app, scans, _| {
                                                app.scanning = false;
                                                app.scans = scans;
                                            });
                                        },
                                    )),
                                ),
                        )
                        .when(latest.is_none(), |d| {
                            d.child(
                                div()
                                    .text_sm()
                                    .text_color(t.text_dim)
                                    .child("No scan yet. Press Scan now."),
                            )
                        })
                        .when(latest.is_some(), |d| {
                            d.child(self.sort_header(t, "folders", &FOLDER_COLS, true, cx))
                        })
                        .children(folders.into_iter().enumerate().map(|(i, item)| {
                            let path = item.path.clone();
                            let icon = icons::path_key(&item.path);
                            div()
                                .id(("folder", i))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .py_1()
                                .cursor_pointer()
                                .hover(|h| h.bg(t.glass_hover))
                                .rounded(px(6.))
                                .child(
                                    cell(false, FOLDER_COLS[0].width)
                                        .text_sm()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1()
                                        .child(self.list_icon(Some(&icon)))
                                        .child(ui::ellipsize(
                                            cx,
                                            item.path.clone(),
                                            FOLDER_COLS[0].width - 16. - LIST_ICON_LEAD,
                                            self.text_px(),
                                        )),
                                )
                                .child(
                                    cell(false, FOLDER_COLS[1].width)
                                        .text_sm()
                                        .text_right()
                                        .child(fmt_size(item.bytes)),
                                )
                                .child(cell(true, 0.).child(ui::meter(
                                    t,
                                    item.bytes as f32 / max as f32 * 100.,
                                    t.accent.into(),
                                )))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.perform(Action::Reveal(path.clone()), cx)
                                }))
                        })),
                )
                .child(
                    card(t)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Scan history"),
                        )
                        .when(!self.scans.is_empty(), |d| {
                            d.child(self.sort_header(t, "scans", &SCAN_COLS, true, cx))
                        })
                        .children(scans.into_iter().map(|(at, folders, total)| {
                            let ago = sysops::now_secs().saturating_sub(at);
                            div()
                                .flex()
                                .flex_row()
                                .text_sm()
                                .child(
                                    cell(false, SCAN_COLS[0].width)
                                        .text_right()
                                        .child(format!("{} ago", fmt_duration(ago))),
                                )
                                .child(
                                    cell(false, SCAN_COLS[1].width)
                                        .text_right()
                                        .text_color(t.text_dim)
                                        .child(folders.to_string()),
                                )
                                .child(
                                    cell(true, 0.)
                                        .text_right()
                                        .text_color(t.text_dim)
                                        .child(fmt_size(total)),
                                )
                        })),
                ),
            cx,
        ))
    }

    pub fn render_power(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.power.clone().unwrap_or_default();
        let row = |k: &'static str, v: String| {
            div()
                .flex()
                .flex_row()
                .py_1()
                .text_sm()
                .child(div().w(px(200.)).text_color(t.text_dim).child(k))
                .child(div().child(v))
        };
        let opt = |v: Option<String>| v.unwrap_or_else(|| "—".into());
        self.page_frame(
            t,
            "Power & Thermal",
            "Battery, charger and thermal state from pmset and ioreg".into(),
        )
        .child(scrollbar::scroll_area(
            t,
            "power-scroll",
            "power-vbar",
            &self.power_scroll,
            div()
                .px_4()
                .pb_4()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .child(
                    card(t)
                        .w(px(360.))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.text_dim)
                                .child("BATTERY"),
                        )
                        .child(
                            div()
                                .text_2xl()
                                .font_weight(FontWeight::BOLD)
                                .child(opt(p.percent.map(|x| format!("{x}%")))),
                        )
                        .child(ui::meter(t, p.percent.unwrap_or(0) as f32, t.good.into()))
                        .child(row("Power source", p.source.clone()))
                        .child(row("Status", p.status.clone()))
                        .child(row("Draw", opt(p.watts.map(|w| format!("{w:.1} W")))))
                        .child(row("Voltage", opt(p.voltage.map(|v| format!("{v:.2} V"))))),
                )
                .child(
                    card(t)
                        .w(px(360.))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(t.text_dim)
                                .child("HEALTH & THERMAL"),
                        )
                        .child(row(
                            "Battery health",
                            opt(p.health.map(|h| format!("{h:.0}%"))),
                        ))
                        .child(row("Cycle count", opt(p.cycles.map(|c| c.to_string()))))
                        .child(row(
                            "Battery temperature",
                            opt(p.temperature.map(|c| format!("{c:.1} °C"))),
                        ))
                        .child(row("Thermal state", p.thermal.clone()))
                        .child(row(
                            "Low Power Mode",
                            if p.low_power_mode {
                                "On".into()
                            } else {
                                "Off".into()
                            },
                        ))
                        .child(row("CPU frequency", "Needs root (powermetrics)".into())),
                ),
            cx,
        ))
    }

    pub fn render_bench(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let b = self.bench.clone();
        let metric = |label: &'static str, value: String, note: &'static str| {
            card(t)
                .w(px(250.))
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(t.text_dim)
                        .child(label),
                )
                .child(div().text_2xl().font_weight(FontWeight::BOLD).child(value))
                .child(div().text_xs().text_color(t.text_dim).child(note))
        };
        let v = |f: fn(&sysops::BenchResult) -> f64, unit: &str| {
            b.as_ref()
                .map(|r| format!("{:.0} {unit}", f(r)))
                .unwrap_or_else(|| "—".into())
        };
        self.page_frame(
            t,
            "Benchmarks",
            "CPU single and all cores, memory bandwidth, and uncached disk (F_NOCACHE)".into(),
        )
        .child(
            div().px_4().pb_3().child(
                ui::pill(
                    t,
                    "bench",
                    if self.benching {
                        "Running… (about 20 s)"
                    } else {
                        "Run benchmarks"
                    },
                    self.benching,
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    if this.benching {
                        return;
                    }
                    this.benching = true;
                    this.run_bg(cx, sysops::run_benchmarks, |app, r, _| {
                        app.benching = false;
                        app.bench = Some(r);
                    });
                })),
            ),
        )
        .child(scrollbar::scroll_area(
            t,
            "bench-scroll",
            "bench-vbar",
            &self.bench_scroll,
            div()
                .px_4()
                .pb_4()
                .flex()
                .flex_row()
                .flex_wrap()
                .gap_3()
                .child(metric(
                    "SINGLE CORE",
                    v(|r| r.single_core, "Mops/s"),
                    "integer mixing, 1 thread · higher is better",
                ))
                .child(metric(
                    "ALL CORES",
                    v(|r| r.multi_core, "Mops/s"),
                    "same workload on every logical core · higher is better",
                ))
                .child(metric(
                    "SCALING",
                    b.as_ref()
                        .map(|r| {
                            format!(
                                "{:.1}× / {}",
                                r.multi_core / r.single_core.max(1.),
                                r.threads
                            )
                        })
                        .unwrap_or_else(|| "—".into()),
                    "multi ÷ single, threads",
                ))
                .child(metric(
                    "MEMORY",
                    b.as_ref()
                        .map(|r| format!("{:.1} GB/s", r.memory_gbps))
                        .unwrap_or_else(|| "—".into()),
                    "256 MB copy, 4 passes · higher is better",
                ))
                .child(metric(
                    "DISK WRITE",
                    v(|r| r.disk_write_mbps, "MB/s"),
                    "512 MB sequential, cache bypassed · higher is better",
                ))
                .child(metric(
                    "DISK READ",
                    v(|r| r.disk_read_mbps, "MB/s"),
                    "512 MB sequential, cache bypassed · higher is better",
                )),
            cx,
        ))
    }

    pub fn render_recorder(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let recording = self.recording.as_ref().map(|f| f.len());
        let replay = self.replay.clone();
        self.page_frame(
            t,
            "Flight Recorder",
            format!(
                "Saved as {} files in {}",
                recorder::FORMAT,
                recorder::dir().display()
            ),
        )
        .child(
            div()
                .px_4()
                .pb_3()
                .flex()
                .flex_row()
                .gap_2()
                .items_center()
                .child(match recording {
                    None => ui::pill(t, "rec", "● Start recording", false).on_click(cx.listener(
                        |this, _, _, cx| {
                            this.recording = Some(Vec::new());
                            cx.notify();
                        },
                    )),
                    Some(_) => ui::pill(t, "rec", "■ Stop and save", true).on_click(cx.listener(
                        |this, _, _, cx| {
                            let frames = this.recording.take().unwrap_or_default();
                            this.notice = Some(match recorder::save(frames) {
                                Ok(p) => (
                                    format!(
                                        "Saved {}",
                                        p.file_name().unwrap_or_default().to_string_lossy()
                                    ),
                                    false,
                                ),
                                Err(e) => (e, true),
                            });
                            this.recordings = recorder::list();
                            cx.notify();
                        },
                    )),
                })
                .when_some(recording, |d, n| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(t.hot)
                            .child(format!("Recording · {n} frames")),
                    )
                }),
        )
        .child(scrollbar::scroll_area(
            t,
            "rec-scroll",
            "rec-vbar",
            &self.rec_scroll,
            div()
                .px_4()
                .pb_4()
                .flex()
                .flex_col()
                .gap_3()
                .when_some(replay, |d, rec| {
                    let peak = rec.frames.iter().map(|f| f.cpu).fold(0., f32::max);
                    d.child(
                        card(t)
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(format!(
                                "Replay · {} frames · peak CPU {peak:.0}%",
                                rec.frames.len()
                            )))
                            .child(div().text_xs().text_color(t.text_dim).child("CPU"))
                            .child(ui::sparkline(
                                rec.frames.iter().map(|f| f.cpu),
                                100.,
                                60.,
                                t.accent.into(),
                                rec.frames.len().max(1),
                            ))
                            .child(div().text_xs().text_color(t.text_dim).child("Memory"))
                            .child(ui::sparkline(
                                rec.frames.iter().map(|f| f.memory),
                                100.,
                                40.,
                                t.good.into(),
                                rec.frames.len().max(1),
                            ))
                            .children(
                                rec.frames
                                    .iter()
                                    .max_by(|a, b| a.cpu.total_cmp(&b.cpu))
                                    .map(|f| {
                                        div().text_xs().text_color(t.text_dim).child(format!(
                                            "Busiest at peak: {}",
                                            f.top
                                                .iter()
                                                .map(|(n, c)| format!("{n} {c:.0}%"))
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        ))
                                    }),
                            ),
                    )
                })
                .child(
                    card(t)
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Saved recordings"),
                        )
                        .children(self.recordings.iter().enumerate().map(|(i, path)| {
                            let p = path.clone();
                            div()
                                .id(("recording", i))
                                .flex()
                                .flex_row()
                                .py_1()
                                .px_1()
                                .rounded(px(6.))
                                .text_sm()
                                .cursor_pointer()
                                .hover(|h| h.bg(t.glass_hover))
                                .child(
                                    div().flex_1().child(
                                        path.file_name()
                                            .unwrap_or_default()
                                            .to_string_lossy()
                                            .into_owned(),
                                    ),
                                )
                                .child(div().text_xs().text_color(t.accent).child("Replay"))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    match recorder::load(&p) {
                                        Ok(r) => this.replay = Some(r),
                                        Err(e) => this.notice = Some((e, true)),
                                    }
                                    cx.notify();
                                }))
                        })),
                ),
            cx,
        ))
    }

    pub fn render_settings(&mut self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.settings.clone();
        let row = |label: &'static str, control: AnyElement| {
            div()
                .flex()
                .flex_row()
                .items_center()
                .py_2()
                .child(div().w(px(220.)).text_sm().child(label))
                .child(control)
        };
        self.page_frame(
            t,
            "Settings",
            format!(
                "Saved to {}",
                sysops::data_dir().join("native-settings.json").display()
            ),
        )
        .child(scrollbar::scroll_area(
            t,
            "settings-scroll",
            "settings-vbar",
            &self.settings_scroll,
            div().px_4().pb_4().child(
                card(t)
                    .child(row(
                        "Appearance",
                        ui::segmented(t)
                            .children(
                                [Appearance::System, Appearance::Light, Appearance::Dark].map(
                                    |a| {
                                        ui::pill(
                                            t,
                                            SharedString::from(format!("appearance-{a:?}")),
                                            format!("{a:?}"),
                                            s.appearance == a,
                                        )
                                        .on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.settings.appearance = a;
                                                this.settings.save();
                                                cx.notify();
                                            }),
                                        )
                                    },
                                ),
                            )
                            .into_any_element(),
                    ))
                    .child(row(
                        "Glass",
                        ui::segmented(t)
                            .children([Glass::Clear, Glass::Tinted].map(|g| {
                                ui::pill(
                                    t,
                                    SharedString::from(format!("glass-{g:?}")),
                                    format!("{g:?}"),
                                    s.glass == g,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.settings.glass = g;
                                        this.settings.save();
                                        cx.notify();
                                    },
                                ))
                            }))
                            .into_any_element(),
                    ))
                    .child(row(
                        "Refresh every",
                        ui::segmented(t)
                            .children([1u64, 2, 5, 10].map(|n| {
                                ui::pill(
                                    t,
                                    ("refresh-rate", n as usize),
                                    format!("{n} s"),
                                    s.refresh_secs == n,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.settings.refresh_secs = n;
                                        this.settings.save();
                                        cx.notify();
                                    },
                                ))
                            }))
                            .into_any_element(),
                    ))
                    .child(row(
                        "Text size",
                        ui::segmented(t)
                            .children([90u32, 100, 115, 130].map(|n| {
                                ui::pill(
                                    t,
                                    ("text-scale", n as usize),
                                    format!("{n}%"),
                                    s.text_scale == n,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.settings.text_scale = n;
                                        this.settings.save();
                                        cx.notify();
                                    },
                                ))
                            }))
                            .into_any_element(),
                    ))
                    .child(row(
                        "High contrast",
                        ui::segmented(t)
                            .children([true, false].map(|on| {
                                ui::pill(
                                    t,
                                    ("contrast", on as usize),
                                    if on { "On" } else { "Off" },
                                    s.high_contrast == on,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.settings.high_contrast = on;
                                        this.settings.save();
                                        cx.notify();
                                    },
                                ))
                            }))
                            .into_any_element(),
                    ))
                    .child(row(
                        "Popout skin",
                        ui::segmented(t)
                            .children(
                                std::iter::once(GLASS_SKIN.to_string())
                                    .chain(skins().into_iter().map(|k| k.name))
                                    .enumerate()
                                    .map(|(i, name)| {
                                        let active = s.popout_skin == name;
                                        ui::pill(t, ("skin", i), name.clone(), active).on_click(
                                            cx.listener(move |this, _, _, cx| {
                                                this.settings.popout_skin = name.clone();
                                                this.settings.save();
                                                cx.notify();
                                            }),
                                        )
                                    }),
                            )
                            .into_any_element(),
                    ))
                    .child(row(
                        "Mark this app in Processes",
                        ui::segmented(t)
                            .children([true, false].map(|on| {
                                ui::pill(
                                    t,
                                    ("mark-self", on as usize),
                                    if on { "On" } else { "Off" },
                                    s.mark_self == on,
                                )
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.settings.mark_self = on;
                                        this.settings.save();
                                        cx.notify();
                                    },
                                ))
                            }))
                            .into_any_element(),
                    ))
                    .child(row(
                        "Holiday themes",
                        ui::segmented(t)
                            .flex_wrap()
                            .max_w(px(560.))
                            .children(holiday_modes().into_iter().map(|(label, mode)| {
                                let selector = format!("holiday-{label}");
                                ui::pill(
                                    t,
                                    SharedString::from(selector.clone()),
                                    label,
                                    s.holiday_theme == mode,
                                )
                                .debug_selector(move || selector)
                                .on_click(cx.listener(
                                    move |this, _, _, cx| {
                                        this.settings.holiday_theme = mode;
                                        this.settings.save();
                                        cx.notify();
                                    },
                                ))
                            }))
                            .into_any_element(),
                    )),
            ),
            cx,
        ))
    }
}

/// The Holiday themes choices: Automatic, Off, then each holiday to preview.
fn holiday_modes() -> Vec<(&'static str, HolidayMode)> {
    [
        ("Automatic", HolidayMode::Automatic),
        ("Off", HolidayMode::Off),
    ]
    .into_iter()
    .chain(Holiday::ALL.map(|h| (h.name(), HolidayMode::Pinned(h))))
    .collect()
}

const ACTIONS_W: f32 = 230.;

/// A table cell: fixed width, except the last column which takes the remaining space.
const FOLDER_COLS: [Col; 3] = [
    crate::tables::text("Folder", 340.),
    crate::tables::num("Size", 100.),
    crate::tables::num("Share", 0.),
];

/// "When" sorts by time: newest first on the first click.
const SCAN_COLS: [Col; 3] = [
    crate::tables::num("When", 160.),
    crate::tables::num("Folders", 100.),
    crate::tables::num("Total", 0.),
];

fn card(t: &Theme) -> Div {
    div()
        .p_4()
        .flex()
        .flex_col()
        .gap_2()
        .rounded(px(18.))
        .bg(t.glass_strong)
        .border_1()
        .border_color(t.highlight)
}
