//! Headless UI tests for the scrolling pages: the overlay bar shows only while a page overflows,
//! it pages and drags like the table bars, and the accent is readable on the glass. As in
//! `ui_tests`, nothing is drawn on screen.

use std::{path::PathBuf, time::Duration};

use gpui::{
    Entity, Modifiers, Rgba, ScrollHandle, TestAppContext, VisualTestContext, WindowAppearance,
    point, px, size,
};

use crate::{
    app::{OtmApp, Page},
    holidays::HolidayMode,
    scrollbar,
    settings::{Appearance, Settings},
    sysops::{FolderSize, Scan},
    theme::Theme,
    ui_tests::{ENV_LOCK, bounds, drag},
};

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("otm-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: called under ENV_LOCK before the app reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };
    dir
}

/// The scroll handle and bar selector of a page whose body can overflow.
fn scroll_of(app: &OtmApp, page: Page) -> Option<(ScrollHandle, &'static str)> {
    Some(match page {
        Page::Overview => (app.overview_scroll.clone(), "overview-vbar"),
        Page::Disk => (app.disk_scroll.clone(), "disk-vbar"),
        Page::Power => (app.power_scroll.clone(), "power-vbar"),
        Page::Benchmarks => (app.bench_scroll.clone(), "bench-vbar"),
        Page::Recorder => (app.rec_scroll.clone(), "rec-vbar"),
        Page::Settings => (app.settings_scroll.clone(), "settings-vbar"),
        _ => return None,
    })
}

fn scrolled(handle: &ScrollHandle) -> f32 {
    -f32::from(handle.offset().y)
}

fn overflow(handle: &ScrollHandle) -> f32 {
    f32::from(handle.max_offset().height)
}

/// Shows `page` and renders it twice, so the bar sees the layout the first frame measured.
fn show(view: &Entity<OtmApp>, cx: &mut VisualTestContext, page: Page) {
    view.update(cx, |app, cx| {
        app.page = page;
        cx.notify();
    });
    cx.run_until_parked();
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

/// Disk Space with more folders than fit shows its bar; a click below the thumb pages down and a
/// drag moves the thumb with the pointer. With nothing to scroll there is no bar.
#[gpui::test]
fn disk_space_bar_appears_on_overflow_and_scrolls(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("page-scroll-disk");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    cx.simulate_resize(size(px(860.), px(520.)));
    view.update(cx, |app, _| {
        app.scans = vec![Scan {
            at: 1_700_000_000,
            items: (0..80)
                .map(|i| FolderSize {
                    path: format!("/Users/someone/folder-{i:02}"),
                    bytes: (80 - i) as u64 * 1_000_000,
                })
                .collect(),
        }];
    });
    show(&view, cx, Page::Disk);

    let track = bounds(cx, "disk-vbar");
    let track_len = f32::from(track.size.height);
    let handle = view.read_with(cx, |app, _| app.disk_scroll.clone());
    let max = overflow(&handle);
    assert!(max > 0., "the folder list overflows");

    let start = scrolled(&handle);
    cx.simulate_click(
        point(track.center().x, track.bottom() - px(4.)),
        Modifiers::none(),
    );
    cx.run_until_parked();
    let paged = scrolled(&handle);
    let expected = (start + track_len).min(max);
    assert!(
        (paged - expected).abs() < 1.,
        "a click below the thumb pages down: {start} -> {paged}, expected {expected}"
    );

    let now = scrolled(&handle);
    let th = scrollbar::thumb(track_len, max, now).unwrap();
    let from = point(
        track.center().x,
        track.origin.y + px(th.start + th.len / 2.),
    );
    let to = point(from.x, from.y + px(40.));
    drag(cx, from, to);
    let dragged = scrolled(&handle);
    let wanted = scrollbar::scrolled_at(track_len, max, th.start + 40.);
    assert!(
        (dragged - wanted).abs() < 1.,
        "the thumb follows the pointer: {dragged} vs {wanted}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

/// Disk Space with nothing to scroll, on a window tall enough for it, has no bar.
#[gpui::test]
fn short_disk_space_shows_no_bar(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("page-scroll-short");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    cx.simulate_resize(size(px(1200.), px(900.)));
    show(&view, cx, Page::Disk);
    assert!(
        cx.debug_bounds("disk-vbar").is_none(),
        "a page that fits shows no bar"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// At the 860×520 minimum window each scrolling page shows its bar exactly when its content
/// overflows, and Settings, which has the most rows, does overflow.
#[gpui::test]
fn scrolling_pages_show_their_bar_only_when_they_overflow(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("page-scroll-all");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    cx.simulate_resize(size(px(860.), px(520.)));

    for page in [
        Page::Overview,
        Page::Power,
        Page::Benchmarks,
        Page::Recorder,
        Page::Settings,
        Page::Disk,
    ] {
        show(&view, cx, page);
        let (handle, bar) = view.read_with(cx, |app, _| scroll_of(app, page).unwrap());
        let overflows = overflow(&handle) > 0.;
        assert_eq!(
            cx.debug_bounds(bar).is_some(),
            overflows,
            "{}: bar shown {} but overflow is {}",
            page.label(),
            !overflows,
            overflow(&handle)
        );
        if page == Page::Settings {
            assert!(overflows, "Settings overflows the minimum window");
        }
    }
    std::fs::remove_dir_all(&dir).ok();
}

fn luminance(c: Rgba) -> f32 {
    let lin = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG 2.x contrast ratio.
fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

/// `top` source-over `base`, keeping `base`'s colour and alpha.
fn over(base: Rgba, top: Rgba) -> Rgba {
    let a = top.a;
    Rgba {
        r: top.r * a + base.r * (1. - a),
        g: top.g * a + base.g * (1. - a),
        b: top.b * a + base.b * (1. - a),
        a: 1.,
    }
}

/// The base accent is the text colour on the glass in both appearances: each glass layer is
/// composited over the opaque window tint, and the accent must reach 4.5:1 on it.
#[test]
fn base_accent_meets_contrast_on_glass_in_light_and_dark() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let settings = Settings {
            appearance,
            holiday_theme: HolidayMode::Off,
            ..Default::default()
        };
        let t = Theme::resolve(WindowAppearance::Light, &settings);
        assert_eq!(t.holiday, None);
        let backdrop = Rgba {
            a: 1.,
            ..t.window_tint
        };
        for (name, layer) in [
            ("glass", t.glass),
            ("glass_strong", t.glass_strong),
            ("window_tint", t.window_tint),
        ] {
            let ratio = contrast(t.accent, over(backdrop, layer));
            assert!(
                ratio >= 4.5,
                "{appearance:?}: accent text on {name} is {ratio:.2}:1"
            );
        }
    }
}
