//! Thin overlay scrollbars for GPUI's scroll handles, which come without one. A bar shows only
//! while its content overflows; dragging the thumb scrolls, and a click on the track pages.

use gpui::{
    Context, Div, MouseButton, MouseDownEvent, Pixels, Point, ScrollHandle, Stateful,
    UniformListScrollHandle, div, point, prelude::*, px,
};

use crate::{
    app::OtmApp,
    theme::{Theme, with_alpha},
};

/// Width of a vertical bar (height of a horizontal one), including its inset.
const TRACK: f32 = 12.;
const BAR: f32 = 8.;
const MIN_THUMB: f32 = 28.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// Where the thumb sits on its track, in px from the track's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Thumb {
    pub start: f32,
    pub len: f32,
}

fn thumb_len(track: f32, max: f32) -> f32 {
    (track * track / (track + max)).clamp(MIN_THUMB.min(track), track)
}

/// The thumb for a track `track` px long over content that scrolls `max` px further, now
/// scrolled `scrolled` px. None when nothing overflows.
pub fn thumb(track: f32, max: f32, scrolled: f32) -> Option<Thumb> {
    if track <= 0. || max <= 0. {
        return None;
    }
    let len = thumb_len(track, max);
    let travel = track - len;
    Some(Thumb {
        start: travel * scrolled.clamp(0., max) / max,
        len,
    })
}

/// The scroll distance for a thumb whose start is `start` px into its track.
pub fn scrolled_at(track: f32, max: f32, start: f32) -> f32 {
    let travel = track - thumb_len(track, max);
    if travel <= 0. {
        return 0.;
    }
    start.clamp(0., travel) / travel * max
}

fn track_len(axis: Axis, handle: &ScrollHandle) -> f32 {
    let size = handle.bounds().size;
    f32::from(match axis {
        Axis::Vertical => size.height,
        Axis::Horizontal => size.width,
    })
}

fn max_scroll(axis: Axis, handle: &ScrollHandle) -> f32 {
    let max = handle.max_offset();
    f32::from(match axis {
        Axis::Vertical => max.height,
        Axis::Horizontal => max.width,
    })
}

fn scrolled(axis: Axis, handle: &ScrollHandle) -> f32 {
    let offset = handle.offset();
    -f32::from(match axis {
        Axis::Vertical => offset.y,
        Axis::Horizontal => offset.x,
    })
}

fn along(axis: Axis, pos: Point<Pixels>, origin: Point<Pixels>) -> f32 {
    f32::from(match axis {
        Axis::Vertical => pos.y - origin.y,
        Axis::Horizontal => pos.x - origin.x,
    })
}

fn scroll_to(axis: Axis, handle: &ScrollHandle, distance: f32) {
    let offset = handle.offset();
    let at = px(-distance);
    handle.set_offset(match axis {
        Axis::Vertical => point(offset.x, at),
        Axis::Horizontal => point(at, offset.y),
    });
}

/// A thumb held down: it follows the pointer until the mouse is released.
#[derive(Clone)]
pub struct ThumbDrag {
    handle: ScrollHandle,
    axis: Axis,
    /// Where on the thumb the press landed, in px.
    grab: f32,
}

impl ThumbDrag {
    pub fn drag_to(&self, pos: Point<Pixels>) {
        let track = track_len(self.axis, &self.handle);
        let max = max_scroll(self.axis, &self.handle);
        let start = along(self.axis, pos, self.handle.bounds().origin) - self.grab;
        scroll_to(self.axis, &self.handle, scrolled_at(track, max, start));
    }
}

/// A press on the track: on the thumb it starts a drag, elsewhere it pages towards the press.
fn press(handle: &ScrollHandle, axis: Axis, pos: Point<Pixels>) -> Option<ThumbDrag> {
    let track = track_len(axis, handle);
    let max = max_scroll(axis, handle);
    let now = scrolled(axis, handle);
    let th = thumb(track, max, now)?;
    let at = along(axis, pos, handle.bounds().origin);
    if (th.start..=th.start + th.len).contains(&at) {
        return Some(ThumbDrag {
            handle: handle.clone(),
            axis,
            grab: at - th.start,
        });
    }
    let page = if at < th.start { -track } else { track };
    scroll_to(axis, handle, (now + page).clamp(0., max));
    None
}

/// The scroll handle of a `uniform_list`.
pub fn of_list(list: &UniformListScrollHandle) -> ScrollHandle {
    list.0.borrow().base_handle.clone()
}

/// A bar for `handle`, positioned at the edge of its container; `None` when nothing overflows.
/// The caller places it inside the same positioned box as the scroll area, so the track covers
/// the scroll area's bounds.
pub fn scrollbar(
    t: &Theme,
    id: &'static str,
    axis: Axis,
    handle: &ScrollHandle,
    cx: &mut Context<OtmApp>,
) -> Option<Stateful<Div>> {
    let th = thumb(
        track_len(axis, handle),
        max_scroll(axis, handle),
        scrolled(axis, handle),
    )?;
    let handle = handle.clone();
    let track = div()
        .id(id)
        .group(id)
        .absolute()
        .debug_selector(|| id.into());
    let track = match axis {
        Axis::Vertical => track.top_0().bottom_0().right_0().w(px(TRACK)),
        Axis::Horizontal => track.left_0().right_0().bottom_0().h(px(TRACK)),
    };
    let bar = div()
        .absolute()
        .rounded_full()
        .bg(with_alpha(t.text, 0.28))
        .group_hover(id, |s| s.bg(with_alpha(t.text, 0.5)));
    let bar = match axis {
        Axis::Vertical => bar.top(px(th.start)).right(px(2.)).w(px(BAR)).h(px(th.len)),
        Axis::Horizontal => bar
            .left(px(th.start))
            .bottom(px(2.))
            .h(px(BAR))
            .w(px(th.len)),
    };
    Some(track.child(bar).on_mouse_down(
        MouseButton::Left,
        cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
            this.thumb_drag = press(&handle, axis, ev.position);
            cx.notify();
        }),
    ))
}

/// A vertically scrolling page body with its overlay bar. The bar shows only when `body` overflows.
pub fn scroll_area(
    t: &Theme,
    id: &'static str,
    bar: &'static str,
    handle: &ScrollHandle,
    body: Div,
    cx: &mut Context<OtmApp>,
) -> Div {
    let area = body
        .id(id)
        .size_full()
        .overflow_y_scroll()
        .track_scroll(handle);
    div()
        .relative()
        .size_full()
        .flex_1()
        .min_h_0()
        .child(area)
        .when_some(scrollbar(t, bar, Axis::Vertical, handle, cx), |d, b| {
            d.child(b)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_thumb_without_overflow() {
        assert_eq!(thumb(300., 0., 0.), None);
        assert_eq!(thumb(0., 100., 0.), None);
    }

    #[test]
    fn thumb_length_follows_the_visible_share() {
        let t = thumb(300., 300., 0.).unwrap();
        assert_eq!(t.len, 150.);
        assert_eq!(t.start, 0.);
        let end = thumb(300., 300., 300.).unwrap();
        assert_eq!(end.start, 300. - 150.);
    }

    #[test]
    fn thumb_never_shorter_than_its_minimum() {
        let t = thumb(300., 1e6, 0.).unwrap();
        assert_eq!(t.len, MIN_THUMB);
    }

    #[test]
    fn dragging_the_thumb_maps_back_to_the_same_scroll() {
        for scrolled in [0., 40., 123.5, 300.] {
            let t = thumb(300., 500., scrolled).unwrap();
            let back = scrolled_at(300., 500., t.start);
            assert!((back - scrolled).abs() < 1e-3, "{scrolled} -> {back}");
        }
    }

    #[test]
    fn dragging_past_either_end_stops_at_the_end() {
        assert_eq!(scrolled_at(300., 300., -50.), 0.);
        assert_eq!(scrolled_at(300., 300., 1e6), 300.);
    }
}
