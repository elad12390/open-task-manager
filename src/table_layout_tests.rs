//! Headless UI tests for the listing and Columns gestures: header drag and edge resize, the
//! double-click fit, the Columns menu (show, hide, drag to reorder), the saved layout and the
//! horizontal scroll. As in `ui_tests`, nothing is drawn on screen.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use gpui::{
    Entity, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext, VisualTestContext, point,
};

use crate::{
    app::{OtmApp, Page},
    columns::ColumnId,
    sysops::InstalledApp,
    ui_tests::{ENV_LOCK, bounds, center, click, drag},
};

const MB: u64 = 1024 * 1024;

fn data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("otm-layout-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: called under ENV_LOCK before the app reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };
    dir
}

fn app_rows(list: &[(&str, &str, u64)]) -> Vec<InstalledApp> {
    list.iter()
        .map(|&(name, version, mb)| InstalledApp {
            name: name.into(),
            version: version.into(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bytes: Some(mb * MB),
        })
        .collect()
}

const APPS: [(&str, &str, u64); 3] = [
    ("Beta", "1.10", 9),
    ("alpha", "1.9", 10),
    ("Gamma", "2.0", 512),
];

/// Opens the window on the Installed Apps listing with `apps`.
fn open_apps<'a>(
    cx: &'a mut TestAppContext,
    apps: &[(&str, &str, u64)],
) -> (Entity<OtmApp>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(OtmApp::new);
    view.update(cx, |app, _| app.icons_enabled = false);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    let apps = app_rows(apps);
    view.update(cx, |app, cx| {
        app.page = Page::Apps;
        app.apps = apps;
        app.rebuild_listing();
        cx.notify();
    });
    (view, cx)
}

fn titles(view: &Entity<OtmApp>, cx: &mut VisualTestContext) -> Vec<&'static str> {
    view.read_with(cx, |app, _| {
        app.table_layouts
            .get("apps")
            .unwrap()
            .columns
            .iter()
            .map(|c| c.title)
            .collect()
    })
}

fn width_of(view: &Entity<OtmApp>, cx: &mut VisualTestContext, title: &str) -> f32 {
    view.read_with(cx, |app, _| {
        app.table_layouts
            .get("apps")
            .unwrap()
            .columns
            .iter()
            .find(|c| c.title == title)
            .unwrap()
            .width
    })
}

fn saved(dir: &Path) -> serde_json::Value {
    let bytes = std::fs::read(dir.join("table-layouts.json")).expect("layouts were saved");
    serde_json::from_slice(&bytes).unwrap()
}

fn saved_titles(dir: &Path, table: &str) -> Vec<String> {
    saved(dir)[table]["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["title"].as_str().unwrap().to_string())
        .collect()
}

/// A double click on `selector`, as the header's edge sees it (the second press counts 2).
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

#[gpui::test]
fn listing_header_drag_reorders_and_saves(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("header-drag");
    let (view, cx) = open_apps(cx, &APPS);
    assert_eq!(titles(&view, cx), ["Name", "Version", "Size", "Location"]);

    let from = center(bounds(cx, "sort-apps-0"));
    let to = center(bounds(cx, "sort-apps-2"));
    drag(cx, from, to);

    assert_eq!(titles(&view, cx), ["Version", "Size", "Name", "Location"]);
    assert_eq!(
        saved_titles(&dir, "apps"),
        ["Version", "Size", "Name", "Location"],
        "the new order is saved"
    );
}

#[gpui::test]
fn listing_edge_drag_resizes_and_saves(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("edge-drag");
    let (view, cx) = open_apps(cx, &APPS);
    let before = width_of(&view, cx, "Size");

    let from = center(bounds(cx, "resize-apps-2"));
    drag(cx, from, point(from.x + gpui::px(40.), from.y));

    let after = width_of(&view, cx, "Size");
    assert!((after - (before + 40.)).abs() < 1., "{before} -> {after}");
    let stored = saved(&dir)["apps"]["columns"][2]["width"].as_f64().unwrap();
    assert!((stored as f32 - after).abs() < 1., "the width is saved");
}

#[gpui::test]
fn listing_double_click_edge_fits_the_widest_cell(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("edge-fit");
    let (view, cx) = open_apps(cx, &APPS);

    // The Name column grows to a long name, and the Version column shrinks to its short values.
    let mut rows = APPS.to_vec();
    rows.push(("A very long application name for fit testing", "1.0", 1));
    let apps = app_rows(&rows);
    view.update(cx, |app, cx| {
        app.apps = apps;
        app.rebuild_listing();
        cx.notify();
    });
    let name_before = width_of(&view, cx, "Name");
    double_click(cx, "resize-apps-0");
    let name_after = width_of(&view, cx, "Name");
    assert!(name_after > name_before, "{name_before} -> {name_after}");

    let version_before = width_of(&view, cx, "Version");
    double_click(cx, "resize-apps-1");
    let version_after = width_of(&view, cx, "Version");
    assert!(
        version_after < version_before,
        "{version_before} -> {version_after}"
    );
    assert_eq!(
        saved(&dir)["apps"]["columns"][1]["width"].as_f64().unwrap() as f32,
        version_after,
        "the fitted width is saved"
    );
}

#[gpui::test]
fn listing_columns_menu_hides_a_column(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("menu-hide");
    let (view, cx) = open_apps(cx, &APPS);
    let size_before = f32::from(bounds(cx, "sort-apps-2").origin.x);

    click(cx, "columns-apps");
    click(cx, "col-check-Version");

    // Size slides left into the place of the hidden Version column (its 120 px default).
    let shift = size_before - f32::from(bounds(cx, "sort-apps-2").origin.x);
    assert!((shift - 120.).abs() < 1., "Size moved left by {shift} px");
    assert_eq!(titles(&view, cx), ["Name", "Version", "Size", "Location"]);
    assert_eq!(saved(&dir)["apps"]["columns"][1]["visible"], false);
}

#[gpui::test]
fn listing_columns_menu_rows_drag_to_reorder(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("menu-drag");
    let (view, cx) = open_apps(cx, &APPS);

    click(cx, "columns-apps");
    let from = center(bounds(cx, "col-row-0"));
    // Drop in the bottom half of the Location row: Name goes after it.
    let to = center(bounds(cx, "col-drop-3-bottom"));
    drag(cx, from, to);

    assert_eq!(titles(&view, cx), ["Version", "Size", "Location", "Name"]);
    assert_eq!(
        saved_titles(&dir, "apps"),
        ["Version", "Size", "Location", "Name"]
    );
}

#[gpui::test]
fn process_columns_menu_rows_drag_to_reorder(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _dir = data_dir("process-menu-drag");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    let first: Vec<ColumnId> = view.read_with(cx, |app, _| {
        app.layout.columns.iter().map(|c| c.id).collect()
    });
    assert_eq!(first[0], ColumnId::Name);

    click(cx, "columns-btn");
    let from = center(bounds(cx, "col-row-0"));
    let to = center(bounds(cx, "col-drop-2-bottom"));
    drag(cx, from, to);

    let order: Vec<ColumnId> = view.read_with(cx, |app, _| {
        app.layout.columns.iter().map(|c| c.id).collect()
    });
    assert_eq!(order[0], ColumnId::Owner);
    assert_eq!(order[2], ColumnId::Name);
}

#[gpui::test]
fn listing_layout_reloads_from_disk_in_a_fresh_app(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _dir = data_dir("reload");
    let (view, cx) = open_apps(cx, &APPS);
    view.update(cx, |app, cx| {
        app.table_layouts.get_mut("apps").unwrap().move_slot(0, 4);
        app.save_table_layouts();
        cx.notify();
    });
    let changed = titles(&view, cx);

    let (fresh, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    let apps = app_rows(&APPS);
    fresh.update(cx, |app, cx| {
        app.page = Page::Apps;
        app.apps = apps;
        app.rebuild_listing();
        cx.notify();
    });
    assert_eq!(titles(&fresh, cx), changed);
}

#[gpui::test]
fn listing_scrollbar_appears_when_columns_overflow(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _dir = data_dir("overflow");
    let (view, cx) = open_apps(cx, &APPS);
    view.update(cx, |app, cx| {
        let layout = app.table_layouts.get_mut("apps").unwrap();
        layout.set_width(0, 800.);
        layout.set_width(3, 800.);
        cx.notify();
    });
    // The overflow is measured on the frame after the widths change, so draw a few frames.
    for _ in 0..3 {
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
    }
    println!(
        "DIAG max {:?}",
        view.read_with(cx, |app, _| app.listing_hscroll.max_offset())
    );
    println!("DIAG bounds {:?}", cx.debug_bounds("listing-table"));
    println!("DIAG vbar {:?}", cx.debug_bounds("listing-vbar"));
    let bar = bounds(cx, "listing-hbar");
    assert!(bar.size.width > gpui::px(0.), "the horizontal bar is drawn");
}

/// Opens the window on `page` with no data of its own (the listing's rows come from the app).
fn open_page(cx: &mut TestAppContext, page: Page) -> (Entity<OtmApp>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(OtmApp::new);
    view.update(cx, |app, _| app.icons_enabled = false);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.page = page;
        app.rebuild_listing();
        cx.notify();
    });
    (view, cx)
}

fn table_titles(
    view: &Entity<OtmApp>,
    cx: &mut VisualTestContext,
    table: &'static str,
) -> Vec<&'static str> {
    view.read_with(cx, |app, _| {
        app.table_layouts
            .get(table)
            .unwrap()
            .columns
            .iter()
            .map(|c| c.title)
            .collect()
    })
}

fn table_width(
    view: &Entity<OtmApp>,
    cx: &mut VisualTestContext,
    table: &'static str,
    title: &str,
) -> f32 {
    view.read_with(cx, |app, _| {
        app.table_layouts
            .get(table)
            .unwrap()
            .columns
            .iter()
            .find(|c| c.title == title)
            .unwrap()
            .width
    })
}

#[gpui::test]
fn services_header_drag_reorders_and_edge_resize_saves(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("services");
    let (view, cx) = open_page(cx, Page::Services);
    assert_eq!(
        table_titles(&view, cx, "services"),
        ["Label", "PID", "Last Exit", "State"]
    );

    let from = center(bounds(cx, "sort-services-0"));
    let to = center(bounds(cx, "sort-services-2"));
    drag(cx, from, to);
    assert_eq!(
        table_titles(&view, cx, "services"),
        ["PID", "Last Exit", "Label", "State"]
    );
    assert_eq!(
        saved_titles(&dir, "services"),
        ["PID", "Last Exit", "Label", "State"]
    );

    let from = center(bounds(cx, "resize-services-3"));
    drag(cx, from, point(from.x + gpui::px(30.), from.y));
    let state = table_width(&view, cx, "services", "State");
    assert!((state - 130.).abs() < 1., "State {state}");
    let stored = saved(&dir)["services"]["columns"][3]["width"]
        .as_f64()
        .unwrap() as f32;
    assert!((stored - state).abs() < 1., "the width is saved");
}

#[gpui::test]
fn users_edge_drag_resizes_and_double_click_fits_saves(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("users");
    let (view, cx) = open_page(cx, Page::Users);
    let before = table_width(&view, cx, "users", "User");

    let from = center(bounds(cx, "resize-users-0"));
    drag(cx, from, point(from.x + gpui::px(40.), from.y));
    let after = table_width(&view, cx, "users", "User");
    assert!((after - (before + 40.)).abs() < 1., "{before} -> {after}");
    assert!(
        (saved(&dir)["users"]["columns"][0]["width"]
            .as_f64()
            .unwrap() as f32
            - after)
            .abs()
            < 1.
    );

    // No process rows here: the fit measures the header alone, which is far narrower than 260 px.
    double_click(cx, "resize-users-0");
    let fitted = table_width(&view, cx, "users", "User");
    assert!(fitted < after, "{after} -> {fitted}");
    assert!(
        (saved(&dir)["users"]["columns"][0]["width"]
            .as_f64()
            .unwrap() as f32
            - fitted)
            .abs()
            < 1.
    );
}

#[gpui::test]
fn disk_folders_hide_and_reorder(cx: &mut TestAppContext) {
    use crate::sysops::{FolderSize, Scan};

    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = data_dir("disk-folders");
    let (view, cx) = open_page(cx, Page::Disk);
    view.update(cx, |app, cx| {
        app.scans = vec![Scan {
            at: 1,
            items: vec![
                FolderSize {
                    path: "/Users/tester/Movies".into(),
                    bytes: 5 * 1024 * MB,
                },
                FolderSize {
                    path: "/Users/tester/Music".into(),
                    bytes: 1024,
                },
            ],
        }];
        cx.notify();
    });

    click(cx, "columns-folders");
    click(cx, "col-check-Size");
    let shown = view.read_with(cx, |app, _| {
        app.table_layouts.get("folders").unwrap().shown().count()
    });
    assert_eq!(shown, 2, "Size is hidden");
    assert_eq!(saved(&dir)["folders"]["columns"][1]["visible"], false);
    // Close the menu again so it does not cover the header.
    click(cx, "columns-folders");

    // Dragging Folder onto Share puts Folder after it; the hidden Size keeps its place.
    let from = center(bounds(cx, "sort-folders-0"));
    let to = center(bounds(cx, "sort-folders-2"));
    drag(cx, from, to);
    assert_eq!(
        table_titles(&view, cx, "folders"),
        ["Size", "Share", "Folder"]
    );
    assert_eq!(saved_titles(&dir, "folders"), ["Size", "Share", "Folder"]);
}
