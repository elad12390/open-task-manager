//! Headless UI tests for the table gestures: double-click column fit, the table scrollbars and the
//! Group dropdown. As in `ui_tests`, nothing is drawn on screen.

use std::{path::PathBuf, time::Duration};

use gpui::{Entity, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, VisualTestContext};
#[cfg(target_os = "macos")]
use gpui::{Modifiers, point, px};

use crate::{
    app::OtmApp,
    columns::{ColumnId, GroupBy},
    processes::Row,
    ui,
    ui_tests::{ENV_LOCK, bounds, center, click, pick_group},
};
#[cfg(target_os = "macos")]
use crate::{scrollbar, ui_tests::drag};

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("otm-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: called under ENV_LOCK before the app reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };
    dir
}

fn width_of(view: &Entity<OtmApp>, cx: &mut VisualTestContext, id: ColumnId) -> f32 {
    view.read_with(cx, |app, _| {
        app.layout
            .columns
            .iter()
            .find(|c| c.id == id)
            .unwrap()
            .width
    })
}

fn double_click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = center(bounds(cx, selector));
    for click_count in [1, 2] {
        cx.simulate_event(MouseDownEvent {
            position: at,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
        cx.simulate_event(MouseUpEvent {
            position: at,
            button: MouseButton::Left,
            click_count,
            ..Default::default()
        });
    }
    cx.run_until_parked();
}

/// Double-clicking the Name divider fits the column to the longest name: every name shows in full,
/// the column is not much wider than that, and the double click leaves no resize drag behind.
#[gpui::test]
#[cfg(target_os = "macos")]
fn double_click_divider_fits_the_column(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("ux-fit");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.layout.group_by = GroupBy::None;
        app.rebuild_rows();
        cx.notify();
    });

    let before = width_of(&view, cx, ColumnId::Name);
    double_click(cx, "resize-Name");
    let after = width_of(&view, cx, ColumnId::Name);
    println!("Name column {before} -> {after} px after a double click");
    assert_ne!(after, before, "the double click fitted the column");

    // Name cells: a 14 px app icon in front, then the text, inside 16 px of padding.
    let (names, text_px) = view.read_with(cx, |app, _| {
        let names: Vec<String> = app
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Process { info, .. } => Some(info.name.clone()),
                _ => None,
            })
            .collect();
        (names, app.text_px())
    });
    assert!(!names.is_empty());
    let room = after - 16. - 18.;
    let cut: Vec<String> = view.read_with(cx, |_, cx| {
        names
            .iter()
            .filter(|n| ui::ellipsize(cx, (*n).clone(), room, text_px) != **n)
            .cloned()
            .collect()
    });
    assert!(cut.is_empty(), "names cut off after fitting: {cut:?}");
    let loose = view.read_with(cx, |_, cx| {
        names
            .iter()
            .any(|n| ui::ellipsize(cx, n.clone(), room - 24., text_px) != *n)
    });
    assert!(loose, "the fit is not much wider than the longest name");

    // The double click's second press did not start a drag: moving with the button held changes nothing.
    assert!(view.read_with(cx, |app, _| app.resizing.is_none()));
    let at = center(bounds(cx, "resize-Name"));
    cx.simulate_mouse_move(
        point(at.x + px(80.), at.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert_eq!(
        width_of(&view, cx, ColumnId::Name),
        after,
        "no drag is left after the double click"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// In the tree, the Name cell's indent, twisty and icon come out of the same width.
#[gpui::test]
fn tree_column_fit_leaves_room_for_the_indent(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("ux-fit-tree");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.layout.group_by = GroupBy::Parent;
        app.rebuild_rows();
        cx.notify();
    });
    double_click(cx, "resize-Name");
    let after = width_of(&view, cx, ColumnId::Name);

    let rows: Vec<(String, usize)> = view.read_with(cx, |app, _| {
        app.rows
            .iter()
            .filter_map(|r| match r {
                Row::Process {
                    info,
                    depth,
                    collapsed: false,
                    ..
                } if !info.is_self => Some((info.name.clone(), *depth)),
                _ => None,
            })
            .collect()
    });
    assert!(!rows.is_empty());
    let cut: Vec<String> = view.read_with(cx, |app, cx| {
        rows.iter()
            .filter(|(name, depth)| {
                let lead = *depth as f32 * 14. + 14. + 8. + 18.;
                ui::ellipsize(cx, name.clone(), after - 16. - lead, app.text_px()) != *name
            })
            .map(|(name, _)| name.clone())
            .collect()
    });
    assert!(cut.is_empty(), "tree names cut off after fitting: {cut:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// The process list has a thin bar that appears because the list overflows. Clicking the track
/// below the thumb pages down, and dragging the thumb scrolls the list in step with the pointer.
#[gpui::test]
#[cfg(target_os = "macos")]
fn process_list_scrollbar_pages_and_drags(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("ux-scroll");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.layout.group_by = GroupBy::None;
        app.rebuild_rows();
        cx.notify();
    });

    let track = bounds(cx, "process-vbar");
    let track_len = f32::from(track.size.height);
    let max = view.read_with(cx, |app, _| {
        f32::from(scrollbar::of_list(&app.scroll).max_offset().height)
    });
    assert!(max > 0., "the process list overflows");
    let scrolled = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| {
            -f32::from(scrollbar::of_list(&app.scroll).offset().y)
        })
    };

    let start = scrolled(cx);
    let below_thumb = point(track.center().x, track.bottom() - px(4.));
    cx.simulate_click(below_thumb, Modifiers::none());
    cx.run_until_parked();
    let paged = scrolled(cx);
    let expected = (start + track_len).min(max);
    assert!(
        (paged - expected).abs() < 1.,
        "a click below the thumb pages down: {start} -> {paged}, expected {expected}"
    );

    let now = scrolled(cx);
    let th = scrollbar::thumb(track_len, max, now).unwrap();
    let from = point(
        track.center().x,
        track.origin.y + px(th.start + th.len / 2.),
    );
    let to = point(from.x, from.y + px(40.));
    drag(cx, from, to);
    let dragged = scrolled(cx);
    let wanted = scrollbar::scrolled_at(track_len, max, th.start + 40.);
    println!("thumb drag: {now} -> {dragged} (wanted {wanted})");
    assert!(
        (dragged - wanted).abs() < 1.,
        "the thumb follows the pointer: {dragged} vs {wanted}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// The Group button opens a menu of the five groupings; picking one applies it and closes the menu.
/// Escape and a click outside close it without changing anything.
#[gpui::test]
fn group_dropdown_picks_and_closes(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("ux-group");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    view.update_in(cx, |app, window, _| window.focus(&app.focus));

    let menu_open = |view: &Entity<OtmApp>, cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| app.group_menu)
    };
    let group_by = |view: &Entity<OtmApp>, cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| app.layout.group_by)
    };

    click(cx, "group-btn");
    assert!(menu_open(&view, cx), "the Group button opens its menu");
    assert!(cx.debug_bounds("group-menu").is_some(), "the menu is drawn");
    click(cx, "group-Parent");
    assert_eq!(group_by(&view, cx), GroupBy::Parent, "Tree is picked");
    assert!(!menu_open(&view, cx), "picking an item closes the menu");

    pick_group(cx, "group-Category");
    assert_eq!(group_by(&view, cx), GroupBy::Category);

    click(cx, "group-btn");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!menu_open(&view, cx), "Escape closes the menu");
    assert_eq!(
        group_by(&view, cx),
        GroupBy::Category,
        "Escape changes nothing"
    );

    click(cx, "group-btn");
    assert!(menu_open(&view, cx));
    click(cx, "process-table");
    assert!(!menu_open(&view, cx), "a click outside closes the menu");
    assert_eq!(group_by(&view, cx), GroupBy::Category);

    click(cx, "group-btn");
    click(cx, "group-btn");
    assert!(!menu_open(&view, cx), "the button closes its own menu");
    std::fs::remove_dir_all(&dir).ok();
}
