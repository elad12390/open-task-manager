//! The process filter field: chips, a blinking caret, and the autocomplete panel.

use std::time::{Duration, Instant};

use gpui::{
    AnyElement, Context, CursorStyle, FontWeight, MouseButton, Rgba, Stateful, anchored, deferred,
    div, prelude::*, px, rgb,
};

use crate::{
    app::{OtmApp, Page},
    filter::{Chip, Kind, Suggestion},
    theme::{Theme, with_alpha},
};

/// Notices (after an action) give way to the normal subtitle after this long.
const NOTICE_FOR: Duration = Duration::from_secs(5);

pub fn kind_color(k: Kind) -> Rgba {
    rgb(match k {
        Kind::App => 0x3e7bfa,
        Kind::User => 0xa66cff,
        Kind::Category => 0x2fc9b4,
        Kind::State => 0xf5a524,
        Kind::Cpu => 0xff5a5f,
        Kind::Memory => 0x34c759,
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
            let mut seen: Option<(String, Instant)> = None;
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
                        Some((msg, _)) => match &seen {
                            Some((last, at)) if last == msg => {
                                if at.elapsed() >= NOTICE_FOR {
                                    app.notice = None;
                                    seen = None;
                                    dirty = true;
                                }
                            }
                            _ => seen = Some((msg.clone(), Instant::now())),
                        },
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

    pub fn filter_active(&self) -> bool {
        !self.query.is_empty() || !self.chips.is_empty()
    }

    pub fn suggestions(&self) -> Vec<Suggestion> {
        if self.page != Page::Processes {
            return Vec::new();
        }
        crate::filter::suggest(&self.query, &self.chips, &self.snapshot.processes)
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
        if !self.chips.contains(&chip) {
            self.chips.push(chip);
        }
        self.query.clear();
        self.suggest_ix = 0;
        self.suggest_open = false;
        self.caret_on = true;
        self.rebuild_rows();
    }

    pub fn remove_chip(&mut self, ix: usize) {
        if ix < self.chips.len() {
            self.chips.remove(ix);
            self.rebuild_rows();
        }
    }

    pub fn clear_filter(&mut self) {
        self.query.clear();
        self.chips.clear();
        self.suggest_ix = 0;
        self.rebuild_rows();
        self.rebuild_listing();
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
            .h(px(22.))
            .pl(px(4.))
            .pr(px(2.))
            .rounded_full()
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
                    .size(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .text_color(t.text_dim)
                    .cursor_pointer()
                    .hover(|h| h.bg(with_alpha(c, 0.3)).text_color(t.text))
                    .child("×")
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

    /// The filter field with its chips, caret and (while open) suggestions.
    pub fn filter_field(&self, t: &Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let focused = self.filter_focused;
        let suggestions = if focused && self.suggest_open {
            self.suggestions()
        } else {
            Vec::new()
        };
        let placeholder = self.query.is_empty() && self.chips.is_empty();
        div()
            .relative()
            .flex_1()
            .min_w(px(240.))
            .child(
                div()
                    .id("filter-field")
                    .debug_selector(|| "filter-field".into())
                    .w_full()
                    .h(px(32.))
                    .px_2()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .overflow_hidden()
                    .rounded(px(16.))
                    .bg(t.glass_strong)
                    .border_1()
                    .border_color(if focused { t.accent } else { t.stroke })
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
                    .child(
                        div()
                            .flex_shrink_0()
                            .pl_1()
                            .text_color(if focused { t.accent } else { t.text_faint })
                            .child("⌕"),
                    )
                    .children(
                        self.chips
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
                            .when(!self.query.is_empty(), |d| {
                                d.child(div().flex_shrink_0().child(self.query.clone()))
                            })
                            .when(focused, |d| d.child(caret(t, self.caret_on)))
                            .when(placeholder, |d| {
                                d.child(div().pl_1().truncate().text_color(t.text_faint).child(
                                    if focused {
                                        "Name, app, user, category… or cpu>5, mem>1g"
                                    } else {
                                        "Filter processes  ⌘F"
                                    },
                                ))
                            }),
                    )
                    .when(self.filter_active(), |d| {
                        d.child(
                            div()
                                .id("filter-clear")
                                .debug_selector(|| "filter-clear".into())
                                .flex_shrink_0()
                                .size(px(18.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_full()
                                .bg(t.glass_hover)
                                .text_xs()
                                .text_color(t.text_dim)
                                .cursor_pointer()
                                .hover(|h| h.text_color(t.text))
                                .child("✕")
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
                    .when(i != highlighted, |d| d.hover(|h| h.bg(t.glass_hover)))
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
                            .child(format!(
                                "{} process{}",
                                s.count,
                                if s.count == 1 { "" } else { "es" }
                            )),
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
                    .child("⌫ remove last")
                    .child("esc close"),
            )
            .into_any_element()
    }
}
