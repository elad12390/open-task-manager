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
        .when(active, |d| d.bg(t.selection).text_color(gpui::white()))
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

/// `text` shortened with "…" to fit `width` px in the system font at `font_size` px.
///
/// GPUI's own `truncate()` measures text before its box has a width inside flex rows and then keeps
/// that measurement, so long cells were clipped mid-letter; this shortens the string up front.
pub fn ellipsize(cx: &gpui::App, text: String, width: f32, font_size: f32) -> String {
    if width <= 0. {
        return String::new();
    }
    let mut wrapper = cx
        .text_system()
        .line_wrapper(gpui::font(".SystemUIFont"), px(font_size));
    wrapper
        .truncate_line(text.into(), px(width), "…", &mut Vec::new())
        .to_string()
}

/// Like [`ellipsize`], but keeps the start and the end with "…" in the middle, for paths and
/// endpoints where the end (the file or port) matters most.
pub fn ellipsize_middle(cx: &gpui::App, text: String, width: f32, font_size: f32) -> String {
    let fitted = ellipsize(cx, text.clone(), width, font_size);
    if fitted == text || width <= 0. {
        return fitted;
    }
    let mut wrapper = cx
        .text_system()
        .line_wrapper(gpui::font(".SystemUIFont"), px(font_size));
    let head = wrapper
        .truncate_line(text.clone().into(), px(width * 0.4), "…", &mut Vec::new())
        .to_string();
    // Measure the tail by truncating the reversed string, then turn it back around.
    let reversed: String = text.chars().rev().collect();
    let tail: String = wrapper
        .truncate_line(reversed.into(), px(width * 0.6), "", &mut Vec::new())
        .chars()
        .rev()
        .collect();
    format!("{head}{tail}")
}

/// A small help tag shown on hover, e.g. the full value of a shortened cell.
pub struct Tip(pub gpui::SharedString);

impl gpui::Render for Tip {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let t = Theme::for_appearance(window.appearance());
        div()
            .max_w(px(520.))
            .px_2()
            .py_1()
            .rounded(px(6.))
            .bg(crate::theme::with_alpha(t.glass_strong, 0.98))
            .border_1()
            .border_color(t.stroke)
            .shadow_md()
            .text_xs()
            .text_color(t.text)
            .child(self.0.clone())
    }
}

/// A tooltip builder for `.tooltip(...)` that shows `text`.
pub fn tip(
    text: impl Into<gpui::SharedString>,
) -> impl Fn(&mut gpui::Window, &mut gpui::App) -> gpui::AnyView + 'static {
    let text = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
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
