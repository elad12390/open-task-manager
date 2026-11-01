//! Secondary windows: the full-screen system-load screensaver and the always-on-top popout.

use gpui::{
    AnyWindowHandle, App, Bounds, Context, Entity, FocusHandle, FontWeight, Global, KeyDownEvent,
    MouseButton, Render, Subscription, Window, WindowBackgroundAppearance, WindowBounds,
    WindowKind, WindowOptions, div, point, prelude::*, px, rgba, size,
};

use crate::{
    app::OtmApp,
    metrics::{HISTORY, fmt_bytes},
    settings::{Skin, skins},
    theme::Theme,
    ui,
};

#[derive(Default)]
struct SecondaryWindows {
    popout: Option<AnyWindowHandle>,
    saver: Option<AnyWindowHandle>,
}

impl Global for SecondaryWindows {}

pub struct Saver {
    app: Entity<OtmApp>,
    focus: FocusHandle,
    _watch: Subscription,
}

impl Saver {
    pub fn open(app: Entity<OtmApp>, cx: &mut App) {
        cx.defer(move |cx| {
            if let Some(handle) = cx.default_global::<SecondaryWindows>().saver
                && handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_ok()
            {
                return;
            }
            let display = cx.primary_display();
            let bounds = display
                .map(|d| d.bounds())
                .unwrap_or_else(|| Bounds::centered(None, size(px(1440.), px(900.)), cx));
            let handle = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Fullscreen(bounds)),
                    titlebar: None,
                    kind: WindowKind::PopUp,
                    window_background: WindowBackgroundAppearance::Opaque,
                    ..Default::default()
                },
                move |window, cx| {
                    cx.new(|cx| {
                        let focus = cx.focus_handle();
                        window.focus(&focus);
                        Saver {
                            _watch: cx.observe(&app, |_, _, cx| cx.notify()),
                            app,
                            focus,
                        }
                    })
                },
            );
            cx.default_global::<SecondaryWindows>().saver = handle.ok().map(Into::into);
        });
    }
}

impl Render for Saver {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let s = &app.snapshot;
        let h = &app.history;
        let mem = if s.memory_total == 0 {
            0.
        } else {
            s.memory_used as f32 / s.memory_total as f32 * 100.
        };
        let net_max = h.net_down.iter().copied().fold(1024., f32::max);
        let band = |label: &'static str,
                    value: gpui::AnyElement,
                    values: Vec<f32>,
                    max: f32,
                    color: u32| {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_end()
                        .gap_4()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgba(0xffffff80))
                                .child(label),
                        )
                        .child(
                            div()
                                .text_3xl()
                                .font_weight(FontWeight::BOLD)
                                .text_color(rgba(color))
                                .child(value),
                        ),
                )
                .child(ui::sparkline(
                    values.into_iter(),
                    max,
                    150.,
                    rgba(color).into(),
                    HISTORY,
                ))
        };
        let top: Vec<_> = {
            let mut p: Vec<_> = s
                .processes
                .iter()
                .map(|p| (p.name.clone(), p.cpu))
                .collect();
            p.sort_by(|a, b| b.1.total_cmp(&a.1));
            p.truncate(5);
            p
        };
        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, ev: &KeyDownEvent, window, _| {
                if matches!(ev.keystroke.key.as_str(), "escape" | "f12" | "q") {
                    window.remove_window();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, window, _| window.remove_window())
            .size_full()
            .bg(rgba(0x07080bff))
            .font_family(".SystemUIFont")
            .p(px(64.))
            .flex()
            .flex_col()
            .gap(px(40.))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_between()
                    .text_color(rgba(0xffffff66))
                    .text_sm()
                    .child(format!("{} · {}", s.host, s.cpu_brand))
                    .child("Esc or click to close"),
            )
            .child(band(
                "CPU",
                crate::metrics::fmt_pct(s.cpu).into_any_element(),
                h.cpu.iter().copied().collect(),
                100.,
                0x5ac8faff,
            ))
            .child(band(
                "MEMORY",
                crate::metrics::fmt_pct(mem).into_any_element(),
                h.memory.iter().copied().collect(),
                100.,
                0xbf5af2ff,
            ))
            .child(band(
                "NETWORK",
                div()
                    .flex()
                    .flex_row()
                    .gap_3()
                    .child(ui::icon_text(
                        "arrow-down",
                        22.,
                        gpui::Hsla::from(rgba(0x30d158ff)),
                        format!("{}/s", fmt_bytes(s.net_down)),
                    ))
                    .child(ui::icon_text(
                        "arrow-up",
                        22.,
                        gpui::Hsla::from(rgba(0x30d158ff)),
                        format!("{}/s", fmt_bytes(s.net_up)),
                    ))
                    .into_any_element(),
                h.net_down.iter().copied().collect(),
                net_max,
                0x30d158ff,
            ))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_6()
                    .text_sm()
                    .text_color(rgba(0xffffff99))
                    .children(
                        top.into_iter()
                            .map(|(n, c)| div().child(format!("{n}  {c:.0}%"))),
                    ),
            )
    }
}

pub struct Popout {
    app: Entity<OtmApp>,
    _watch: Subscription,
}

impl Popout {
    pub fn open(app: Entity<OtmApp>, cx: &mut App) {
        cx.defer(move |cx| {
            if let Some(handle) = cx.default_global::<SecondaryWindows>().popout
                && handle
                    .update(cx, |_, window, _| window.activate_window())
                    .is_ok()
            {
                return;
            }
            let bounds = Bounds::new(point(px(80.), px(80.)), size(px(300.), px(150.)));
            let chrome = crate::platform::window_chrome(crate::platform::WindowRole::Popout);
            let handle = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: chrome.titlebar,
                    kind: WindowKind::PopUp,
                    is_resizable: false,
                    is_minimizable: false,
                    window_background: chrome.background,
                    focus: false,
                    ..Default::default()
                },
                move |_, cx| {
                    cx.new(|cx| Popout {
                        _watch: cx.observe(&app, |_, _, cx| cx.notify()),
                        app,
                    })
                },
            );
            cx.default_global::<SecondaryWindows>().popout = handle.ok().map(Into::into);
        });
    }
}

impl Render for Popout {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.app.read(cx);
        let mut t = Theme::resolve(window.appearance(), &app.settings);
        let mut radius = 20.;
        if let Some(skin) = skins()
            .into_iter()
            .find(|k| k.name == app.settings.popout_skin)
        {
            t.window_tint = rgba(Skin::color(&skin.background, 0xff));
            t.glass = rgba(Skin::color(&skin.background, 0xff));
            t.highlight = rgba(Skin::color(&skin.ink, 0x33));
            t.stroke = rgba(Skin::color(&skin.ink, 0x26));
            t.text = rgba(Skin::color(&skin.ink, 0xff));
            t.text_dim = rgba(Skin::color(&skin.ink, 0xa6));
            t.accent = rgba(Skin::color(&skin.accent, 0xff));
            radius = skin.corner_radius;
        }
        let s = &app.snapshot;
        let mem = if s.memory_total == 0 {
            0.
        } else {
            s.memory_used as f32 / s.memory_total as f32 * 100.
        };
        let line = |label: &'static str, value: String, pct: f32| {
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
                        .child(div().text_color(t.text_dim).child(label))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(value)),
                )
                .child(ui::meter(&t, pct, t.load_color(pct)))
        };
        div()
            .id("popout")
            .on_mouse_down(MouseButton::Left, |_, window, _| window.start_window_move())
            .on_mouse_down(MouseButton::Right, |_, window, _| window.remove_window())
            .size_full()
            .p_3()
            .bg(t.window_tint)
            .font_family(".SystemUIFont")
            .text_color(t.text)
            .child(
                ui::glass(&t)
                    .rounded(px(radius))
                    .size_full()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(line("CPU", crate::metrics::fmt_pct(s.cpu), s.cpu))
                    .child(line("Memory", crate::metrics::fmt_pct(mem), mem))
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .text_xs()
                            .child(div().text_color(t.text_dim).child("Network"))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .gap_2()
                                    .child(ui::icon_text(
                                        "arrow-down",
                                        11.,
                                        t.text,
                                        format!("{}/s", fmt_bytes(s.net_down)),
                                    ))
                                    .child(ui::icon_text(
                                        "arrow-up",
                                        11.,
                                        t.text,
                                        format!("{}/s", fmt_bytes(s.net_up)),
                                    )),
                            ),
                    ),
            )
    }
}
