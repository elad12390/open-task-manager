use gpui::{Div, FontWeight, Hsla, Stateful, div, prelude::*, px};

use crate::theme::Theme;

pub fn glass(t: &Theme) -> Div {
    div()
        .bg(t.glass)
        .border_1()
        .border_color(t.highlight)
        .rounded(px(20.))
        .shadow_md()
}

pub fn pill(
    t: &Theme,
    id: impl Into<gpui::ElementId>,
    label: impl Into<gpui::SharedString>,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .px_3()
        .py(px(5.))
        .rounded_full()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .when(active, |d| d.bg(t.accent).text_color(gpui::white()))
        .when(!active, |d| {
            d.text_color(t.text).hover(|s| s.bg(t.glass_hover))
        })
        .child(label.into())
}

pub fn segmented(t: &Theme) -> Div {
    div()
        .flex()
        .flex_row()
        .gap_0p5()
        .p_0p5()
        .rounded_full()
        .bg(t.glass_strong)
        .border_1()
        .border_color(t.stroke)
}

/// Bar sparkline from a series of values, scaled to `max`.
pub fn sparkline(
    values: impl Iterator<Item = f32>,
    max: f32,
    height: f32,
    color: Hsla,
    slots: usize,
) -> Div {
    let values: Vec<f32> = values.collect();
    let pad = slots.saturating_sub(values.len());
    div()
        .flex()
        .flex_row()
        .items_end()
        .gap(px(2.))
        .h(px(height))
        .w_full()
        .children((0..pad).map(|_| div().flex_1().h(px(1.))))
        .children(values.into_iter().map(move |v| {
            let h = (v / max.max(f32::EPSILON)).clamp(0., 1.) * height;
            div()
                .flex_1()
                .h(px(h.max(1.)))
                .rounded_t(px(2.))
                .bg(color.opacity(0.85))
        }))
}

pub fn meter(t: &Theme, pct: f32, color: Hsla) -> Div {
    div().h(px(6.)).w_full().rounded_full().bg(t.stroke).child(
        div()
            .h_full()
            .w(gpui::relative((pct / 100.).clamp(0., 1.)))
            .rounded_full()
            .bg(color),
    )
}
