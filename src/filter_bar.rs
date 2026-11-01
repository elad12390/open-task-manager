//! The process filter field: chips, a blinking caret, and the autocomplete panel.

use std::time::{Duration, Instant};

use gpui::{
    AnyElement, Context, CursorStyle, FontWeight, Hsla, MouseButton, Rgba, Stateful, anchored,
    deferred, div, prelude::*, px, rgb,
};

use crate::{
    app::{OtmApp, Page},
    filter::{Chip, Kind, Suggestion},
    theme::{Theme, with_alpha},
};

/// Notices (after an action) give way to the normal subtitle after this long; errors stay longer
/// so there is time to read them.
const NOTICE_FOR: Duration = Duration::from_secs(5);
const ERROR_FOR: Duration = Duration::from_secs(15);

/// A notice with a symbol as well as its colour, so success and failure differ without colour.
pub fn notice(msg: &str, err: bool, color: impl Into<Hsla>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(crate::ui::icon(
            if err { "triangle-alert" } else { "check" },
            13.,
            color,
        ))
        .child(msg.to_string())
}

pub fn kind_color(k: Kind) -> Rgba {
    rgb(match k {
        Kind::App => 0x3e7bfa,
        Kind::User => 0xa66cff,
        Kind::Category => 0x2fc9b4,
        Kind::State => 0xf5a524,
        Kind::Cpu => 0xff5a5f,
        Kind::Memory => 0x34c759,
        Kind::Column(_) => 0x5ac8fa,
    })
}

/// A small coloured "App" / "User" / … badge.
fn kind_badge(k: Kind) -> gpui::Div {
    let c = kind_color(k);
    div()
        .flex_shrink_0()
        .px(px(6.))
        .rounded_full()
        .bg(with_alpha(c, 0.2))
        .text_color(c)
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .child(k.label())
}

pub fn caret(t: &Theme, on: bool) -> gpui::Div {
    div()
        .flex_shrink_0()
        .w(px(1.5))
        .h(px(15.))
        .rounded_full()
        .when(on, |d| d.bg(t.accent))
}

impl OtmApp {
    /// Blink the caret while the filter has focus, and retire notices after a few seconds.
    pub fn start_caret_blink(&self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            // The notice last seen (by its allocation, so setting the same text again restarts the
            // clock) and when it appeared.
            let mut seen: Option<(usize, Instant)> = None;
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(530))
                    .await;
                let alive = this.update(cx, |app, cx| {
                    let mut dirty = false;
                    if app.filter_focused {
                        app.caret_on = !app.caret_on;
                        dirty = true;
                    }
                    match &app.notice {
                        Some((msg, err)) => {
                            let id = msg.as_ptr() as usize;
                            match seen {
                                Some((last, at)) if last == id => {
                                    if at.elapsed() >= if *err { ERROR_FOR } else { NOTICE_FOR } {
                                        app.notice = None;
                                        seen = None;
                                        dirty = true;
                                    }
                                }
                                _ => seen = Some((id, Instant::now())),
                            }
                        }
                        None => seen = None,
                    }
                    if dirty {
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Byte index of the caret in `query`.
    pub fn caret_byte(&self) -> usize {
        let chars = self.query.chars().count();
        let at = chars - self.caret_back.min(chars);
        self.query
            .char_indices()
            .nth(at)
            .map_or(self.query.len(), |(i, _)| i)
    }

    /// The typed text with the caret where it is (when focused).
    pub fn query_with_caret(&self, t: &Theme) -> gpui::Div {
        let at = self.caret_byte();
        let (before, after) = self.query.split_at(at);
        div()
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .when(!before.is_empty(), |d| d.child(before.to_string()))
            .when(self.filter_focused, |d| d.child(caret(t, self.caret_on)))
            .when(!after.is_empty(), |d| d.child(after.to_string()))
    }

    /// The chips of the page being shown: process chips on Processes, listing chips elsewhere.
    pub fn active_chips(&self) -> &[Chip] {
        if self.page == Page::Processes {
            &self.chips
        } else {
            &self.list_chips
        }
    }

    pub fn active_chips_mut(&mut self) -> &mut Vec<Chip> {
        if self.page == Page::Processes {
            &mut self.chips
        } else {
            &mut self.list_chips
        }
    }

    pub fn filter_active(&self) -> bool {
        !self.query.is_empty() || !self.active_chips().is_empty()
    }

    pub fn suggestions(&self) -> Vec<Suggestion> {
        match self.page {
            Page::Processes => {
                crate::filter::suggest(&self.query, &self.chips, &self.snapshot.processes)
            }
            Page::Disk => {
                let folders: Vec<Vec<String>> = self
                    .scans
                    .first()
                    .map(|s| s.items.iter().map(|f| vec![f.path.clone()]).collect())
                    .unwrap_or_default();
                crate::filter::suggest_rows(&self.query, &self.list_chips, &["Folder"], &folders)
            }
            Page::Users | Page::Startup | Page::Services | Page::Connections | Page::Apps => {
                let titles: Vec<&'static str> =
                    self.listing.headers.iter().map(|h| h.title).collect();
                crate::filter::suggest_rows(
                    &self.query,
                    &self.list_chips,
                    &titles,
                    &self.listing.cells,
                )
            }
            _ => Vec::new(),
        }
    }

    /// Text is being typed towards a suggestion that is on show, so an empty result is not final.
    pub fn typing_to_suggestion(&self) -> bool {
        self.filter_focused
            && self.suggest_open
            && !self.query.is_empty()
            && !self.suggestions().is_empty()
    }

    pub fn focus_filter(&mut self) {
        self.filter_focused = true;
        self.suggest_open = true;
        self.caret_on = true;
    }

    pub fn blur_filter(&mut self) {
        self.filter_focused = false;
        self.suggest_open = false;
    }

    pub fn add_chip(&mut self, chip: Chip) {
        let chips = self.active_chips_mut();
        if !chips.contains(&chip) {
            chips.push(chip);
        }
        self.query.clear();
        self.caret_back = 0;
        self.suggest_ix = 0;
        self.suggest_open = false;
        self.caret_on = true;
        self.query_changed();
    }

    pub fn remove_chip(&mut self, ix: usize) {
        if ix < self.active_chips().len() {
            self.active_chips_mut().remove(ix);
            self.query_changed();
        }
    }

    /// `cpu…` or `mem…` typed without a usable number yet.
    fn typing_threshold(&self) -> bool {
        let q = self.query.trim().to_lowercase();
        self.page == Page::Processes
            && (q.starts_with("cpu") || q.starts_with("mem"))
            && q.len() > 3
            && crate::filter::parse_numeric(&q).is_none()
    }

    /// Turn chip `ix` back into text in the field, to change it.
    pub fn edit_chip(&mut self, ix: usize) {
        if ix >= self.active_chips().len() {
            return;
        }
        let chip = self.active_chips_mut().remove(ix);
        self.query = match chip {
            Chip::App(v) => format!("app:{v}"),
            Chip::User(v) => format!("user:{v}"),
            Chip::Category(v) => format!("cat:{v}"),
            Chip::State(v) => format!("state:{v}"),
            Chip::CpuAbove(x) => format!("cpu>{x}"),
            Chip::MemoryAbove(b) if b % (1 << 30) == 0 => format!("mem>{}g", b >> 30),
            Chip::MemoryAbove(b) => format!("mem>{}", b >> 20),
            Chip::Column { title, value } => format!("{}:{value}", title.to_lowercase()),
        };
        self.caret_back = 0;
        self.suggest_ix = 0;
        self.focus_filter();
        self.query_changed();
    }

    pub fn clear_filter(&mut self) {
        self.query.clear();
        self.caret_back = 0;
        self.active_chips_mut().clear();
        self.suggest_ix = 0;
        self.query_changed();
    }

    fn chip_view(
        &self,
        t: &Theme,
        ix: usize,
        chip: &Chip,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let c = kind_color(chip.kind());
        let bundle = match chip {
            Chip::App(name) => self.owner_bundles.get(name).cloned(),
            _ => None,
        };
        div()
            .id(("filter-chip", ix))
            .debug_selector(move || format!("filter-chip-{ix}"))
            .flex_shrink_0()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .h(px(24.))
            .pl(px(4.))
            .pr(px(1.))
            .rounded_full()
            .cursor_pointer()
            .tooltip(crate::ui::tip("Click to edit"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.edit_chip(ix);
                    window.focus(&this.focus);
                    cx.notify();
                }),
            )
            .bg(with_alpha(c, 0.14))
            .border_1()
            .border_color(with_alpha(c, 0.4))
            .text_xs()
            .child(kind_badge(chip.kind()))
            .when_some(bundle, |d, b| d.child(self.icon_slot(Some(&b))))
            .child(
                div()
                    .max_w(px(160.))
                    .truncate()
                    .text_color(t.text)
                    .font_weight(FontWeight::MEDIUM)
                    .child(chip.value()),
            )
            .child(
                div()
                    .id(("filter-chip-remove", ix))
                    .debug_selector(move || format!("filter-chip-remove-{ix}"))
                    .size(px(22.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_color(t.text_dim)
                    .cursor_pointer()
                    .hover(|h| h.bg(with_alpha(c, 0.3)).text_color(t.text))
                    .tooltip(crate::ui::tip("Remove"))
                    .child(crate::ui::icon("x", 12., t.text_dim))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.remove_chip(ix);
                            cx.notify();
                        }),
                    ),
            )
    }

    fn filter_placeholder(&self, focused: bool) -> String {
        let find = crate::platform::CAPS.mod_label;
        let what = match (self.page, focused) {
            (Page::Processes, true) => return "Name, app, user, category… or cpu>5, mem>1g".into(),
            (_, true) => return "Pick a value".into(),
            (Page::Processes, false) => "processes",
            (Page::Users, false) => "users",
            (Page::Startup, false) => "startup items",
            (Page::Services, false) => "services",
            (Page::Connections, false) => "connections",
            (Page::Apps, false) => "apps",
            (Page::Disk, false) => "folders",
            _ => return format!("Filter  {find}F"),
        };
        format!("Filter {what}  {find}F")
    }

    /// The filter field above a listing or Disk Space table.
    pub fn page_filter(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(280.))
            .mx_4()
            .mb_2()
            .child(self.filter_field(t, cx))
    }

    /// The filter field with its chips, caret and (while open) suggestions.
    pub fn filter_field(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.filter_focused;
        let suggestions = if focused && self.suggest_open {
            self.suggestions()
        } else {
            Vec::new()
        };
        let placeholder = self.query.is_empty() && self.active_chips().is_empty();
        let (id, selector) = if self.page == Page::Processes {
            ("filter-field", "filter-field")
        } else {
            ("page-filter", "page-filter")
        };
        div()
            .relative()
            .flex_1()
            .min_w(px(240.))
            .child(
                div()
                    .id(id)
                    .debug_selector(move || selector.into())
                    .w_full()
                    .h(px(32.))
                    .px_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.))
                    .overflow_hidden()
                    .rounded(px(16.))
                    .bg(t.glass_strong)
                    .when(focused, |d| d.border_2().border_color(t.accent))
                    .when(!focused, |d| d.border_1().border_color(t.text_faint))
                    .cursor(CursorStyle::IBeam)
                    .text_sm()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            if this.filter_focused {
                                this.suggest_open = !this.suggest_open;
                                this.caret_on = true;
                            } else {
                                this.focus_filter();
                            }
                            window.focus(&this.focus);
                            cx.notify();
                        }),
                    )
                    .child(div().flex_shrink_0().pl_1().child(crate::ui::icon(
                        "search",
                        14.,
                        if focused { t.accent } else { t.text_faint },
                    )))
                    .children(
                        self.active_chips()
                            .iter()
                            .enumerate()
                            .map(|(i, c)| self.chip_view(t, i, c, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_row()
                            .items_center()
                            .when(!self.query.is_empty() || focused, |d| {
                                d.child(self.query_with_caret(t))
                            })
                            .when(placeholder, |d| {
                                d.child(
                                    div()
                                        .pl_1()
                                        .truncate()
                                        .text_color(t.text_faint)
                                        .child(self.filter_placeholder(focused)),
                                )
                            }),
                    )
                    .when(self.filter_active(), |d| {
                        d.child(
                            div()
                                .id("filter-clear")
                                .debug_selector(|| "filter-clear".into())
                                .flex_shrink_0()
                                .size(px(22.))
                                .tooltip(crate::ui::tip("Clear (Esc)"))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(t.glass_hover)
                                .text_xs()
                                .text_color(t.text_dim)
                                .cursor_pointer()
                                .child(crate::ui::icon("x", 12., t.text_dim))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.clear_filter();
                                        cx.notify();
                                    }),
                                ),
                        )
                    }),
            )
            .when(
                suggestions.is_empty() && focused && self.typing_threshold(),
                |d| {
                    d.child(
                        div().absolute().top(px(38.)).left_0().child(
                            deferred(
                                anchored().child(
                                    div()
                                        .id("filter-hint")
                                        .debug_selector(|| "filter-hint".into())
                                        .occlude()
                                        .px_3()
                                        .py_2()
                                        .rounded(px(10.))
                                        .bg(with_alpha(t.glass_strong, 0.97))
                                        .border_1()
                                        .border_color(t.highlight)
                                        .shadow_lg()
                                        .text_xs()
                                        .text_color(t.text_dim)
                                        .child("Try cpu>5 or mem>1.5g"),
                                ),
                            )
                            .with_priority(1),
                        ),
                    )
                },
            )
            .when(!suggestions.is_empty(), |d| {
                d.child(
                    div().absolute().top(px(38.)).left_0().child(
                        deferred(
                            anchored()
                                .snap_to_window_with_margin(px(8.))
                                .child(self.suggestions_panel(t, suggestions, cx)),
                        )
                        .with_priority(1),
                    ),
                )
            })
    }

    fn suggestions_panel(
        &self,
        t: &Theme,
        suggestions: Vec<Suggestion>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = self.suggest_ix.min(suggestions.len().saturating_sub(1));
        let title = if self.query.is_empty() {
            "FILTER BY"
        } else {
            "SUGGESTIONS"
        };
        div()
            .id("filter-suggestions")
            .debug_selector(|| "filter-suggestions".into())
            .occlude()
            .w(px(400.))
            .p_1()
            .flex()
            .flex_col()
            .bg(with_alpha(t.glass_strong, 0.97))
            .border_1()
            .border_color(t.highlight)
            .rounded(px(12.))
            .shadow_lg()
            .text_sm()
            .child(
                div()
                    .px_2()
                    .pt_1()
                    .pb(px(2.))
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(t.text_dim)
                    .child(title),
            )
            .children(suggestions.into_iter().enumerate().map(|(i, s)| {
                let kind = s.chip.kind();
                let bundle = match &s.chip {
                    Chip::App(name) => self.owner_bundles.get(name).cloned(),
                    _ => None,
                };
                let chip = s.chip.clone();
                div()
                    .id(("suggestion", i))
                    .debug_selector(move || format!("suggestion-{i}"))
                    .h(px(30.))
                    .px_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(i == highlighted, |d| d.bg(t.accent_soft))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.suggest_ix != i {
                            this.suggest_ix = i;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.add_chip(chip.clone());
                            cx.notify();
                        }),
                    )
                    .child(
                        div()
                            .w(px(76.))
                            .flex_shrink_0()
                            .flex()
                            .child(kind_badge(kind)),
                    )
                    .when(kind == Kind::App, |d| {
                        d.child(self.icon_slot(bundle.as_deref()))
                    })
                    .child(div().flex_1().min_w_0().truncate().child(s.chip.value()))
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_xs()
                            .text_color(t.text_dim)
                            .child(s.count_label()),
                    )
            }))
            .child(
                div()
                    .mt_1()
                    .px_2()
                    .py_1()
                    .border_t_1()
                    .border_color(t.stroke)
                    .flex()
                    .flex_row()
                    .gap_3()
                    .text_xs()
                    .text_color(t.text_faint)
                    .child("↑↓ choose")
                    .child("⏎ add")
                    .when(
                        self.query.is_empty() && !self.active_chips().is_empty(),
                        |d| d.child("⌫ remove last"),
                    )
                    .child("esc close"),
            )
            .into_any_element()
    }
}
