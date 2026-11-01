//! Multi-selection: clicks with ⌘ and Shift, Shift-arrows, ⌘A, Esc, right-click menus, copy,
//! bulk buttons on the listing pages, and selections that outlive a refresh.

use std::rc::Rc;

use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use crate::{
    app::{OtmApp, Page},
    columns::GroupBy,
    menus::{CopySelected, SelectAll},
    pages::{Action, ActionButton, ListRow, Listing},
    processes::{Row, RowId},
    ui_tests::{ENV_LOCK, bounds, center},
};

fn cmd() -> Modifiers {
    Modifiers {
        platform: true,
        ..Modifiers::none()
    }
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::none()
    }
}

fn click_with(cx: &mut VisualTestContext, selector: String, modifiers: Modifiers) {
    let selector: &'static str = Box::leak(selector.into_boxed_str());
    let at = center(bounds(cx, selector));
    cx.simulate_click(at, modifiers);
    cx.run_until_parked();
}

fn right_click(cx: &mut VisualTestContext, selector: String) {
    let selector: &'static str = Box::leak(selector.into_boxed_str());
    let at = center(bounds(cx, selector));
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    name: &'static str,
) -> (Entity<OtmApp>, &'a mut VisualTestContext) {
    let dir = std::env::temp_dir().join(format!("otm-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: the ENV_LOCK guard held by each test serialises the variable's use.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.layout.group_by = GroupBy::None;
        app.rebuild_rows();
        cx.notify();
    });
    cx.run_until_parked();
    (view, cx)
}

fn process_pids(app: &OtmApp) -> Vec<u32> {
    app.rows
        .iter()
        .filter_map(|r| match r {
            Row::Process { info, .. } => Some(info.pid),
            Row::Group { .. } => None,
        })
        .collect()
}

fn selection(view: &Entity<OtmApp>, cx: &mut VisualTestContext) -> Vec<u32> {
    let mut pids: Vec<u32> = view.read_with(cx, |app, _| {
        app.selection
            .iter()
            .filter_map(|id| match id {
                RowId::Pid(pid) => Some(*pid),
                RowId::Group(_) => None,
            })
            .collect()
    });
    pids.sort();
    pids
}

fn sorted(mut pids: Vec<u32>) -> Vec<u32> {
    pids.sort();
    pids
}

fn list_row(key: &str, cell: &str) -> ListRow {
    ListRow {
        key: key.to_string(),
        cells: vec![cell.to_string().into()],
        keys: Vec::new(),
        dim: false,
        actions: Vec::new(),
        icon: None,
    }
}

fn set_listing(view: &Entity<OtmApp>, cx: &mut VisualTestContext, page: Page, rows: Vec<ListRow>) {
    view.update(cx, |app, cx| {
        app.page = page;
        let total = rows.len();
        app.listing = Listing {
            headers: Vec::new(),
            rows: Rc::new(rows),
            total,
            cells: Vec::new(),
            icons: false,
        };
        cx.notify();
    });
    cx.run_until_parked();
}

fn list_selection(view: &Entity<OtmApp>, cx: &mut VisualTestContext) -> Vec<String> {
    let mut keys: Vec<String> =
        view.read_with(cx, |app, _| app.list_selection.iter().cloned().collect());
    keys.sort();
    keys
}

#[gpui::test]
fn process_clicks_toggle_and_extend_by_range(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-clicks");
    let pids = view.read_with(cx, |app, _| process_pids(app));
    assert!(pids.len() >= 4, "the sample has at least four processes");
    let (a, b, c, d) = (pids[0], pids[1], pids[2], pids[3]);

    click_with(cx, format!("proc-row-{a}"), Modifiers::none());
    assert_eq!(
        selection(&view, cx),
        vec![a],
        "a plain click selects one row"
    );

    click_with(cx, format!("proc-row-{c}"), cmd());
    assert_eq!(
        sorted(vec![a, c]),
        selection(&view, cx),
        "⌘-click adds a row"
    );

    click_with(cx, format!("proc-row-{a}"), cmd());
    assert_eq!(
        vec![c],
        selection(&view, cx),
        "⌘-click on a selected row removes it"
    );

    click_with(cx, format!("proc-row-{a}"), cmd());
    click_with(cx, format!("proc-row-{d}"), shift());
    assert_eq!(
        sorted(vec![a, b, c, d]),
        selection(&view, cx),
        "Shift-click selects the rows from the anchor to the clicked one"
    );

    click_with(cx, format!("proc-row-{b}"), Modifiers::none());
    assert_eq!(vec![b], selection(&view, cx), "a plain click starts over");
}

#[gpui::test]
fn keyboard_shift_arrows_extend_and_cmd_a_and_esc_collapse(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-keys");
    let pids = view.read_with(cx, |app, _| process_pids(app));
    let all = pids.clone();
    click_with(cx, format!("proc-row-{}", pids[0]), Modifiers::none());

    cx.simulate_keystrokes("shift-down shift-down");
    cx.run_until_parked();
    assert_eq!(
        sorted(pids[..3].to_vec()),
        selection(&view, cx),
        "Shift-↓ extends from the anchor"
    );

    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(
        vec![pids[3]],
        selection(&view, cx),
        "a plain arrow selects one row"
    );

    cx.dispatch_action(SelectAll);
    cx.run_until_parked();
    assert_eq!(
        sorted(all),
        selection(&view, cx),
        "⌘A selects every process row"
    );

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        vec![pids[3]],
        selection(&view, cx),
        "Esc collapses to the cursor row"
    );
}

#[gpui::test]
fn right_click_inside_selection_keeps_it_and_menu_covers_it(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-menu");
    let pids = view.read_with(cx, |app, _| process_pids(app));
    let (a, b, c) = (pids[0], pids[1], pids[2]);

    right_click(cx, format!("proc-row-{c}"));
    assert_eq!(
        vec![c],
        selection(&view, cx),
        "right-click on an unselected row selects it"
    );
    view.update(cx, |app, cx| {
        app.context_menu = None;
        cx.notify();
    });

    click_with(cx, format!("proc-row-{a}"), Modifiers::none());
    click_with(cx, format!("proc-row-{b}"), cmd());
    right_click(cx, format!("proc-row-{b}"));
    assert_eq!(
        sorted(vec![a, b]),
        selection(&view, cx),
        "right-click inside the selection keeps it"
    );
    let labels: Vec<String> = view.read_with(cx, |app, _| {
        app.context_items(&RowId::Pid(b))
            .into_iter()
            .map(|i| i.label)
            .collect()
    });
    for expected in [
        "Quit 2 Processes",
        "Force Quit 2 Processes…",
        "Copy 2 Rows",
        "Copy PIDs",
    ] {
        assert!(
            labels.iter().any(|l| l == expected),
            "menu has {expected:?}, got {labels:?}"
        );
    }
}

#[gpui::test]
fn copy_writes_one_line_per_selected_row(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-copy");
    let pids = view.read_with(cx, |app, _| process_pids(app));
    click_with(cx, format!("proc-row-{}", pids[0]), Modifiers::none());
    click_with(cx, format!("proc-row-{}", pids[1]), cmd());
    cx.dispatch_action(CopySelected);
    cx.run_until_parked();

    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .expect("clipboard holds text");
    let (first, second) = view.read_with(cx, |app, _| {
        (app.process_lines(&[pids[0]]), app.process_lines(&[pids[1]]))
    });
    assert_eq!(text, format!("{first}\n{second}"));
}

#[gpui::test]
fn toolbar_force_quit_asks_about_the_whole_selection(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-force");
    let pids = view.read_with(cx, |app, _| process_pids(app));
    click_with(cx, format!("proc-row-{}", pids[0]), Modifiers::none());
    click_with(cx, format!("proc-row-{}", pids[2]), cmd());
    click_with(cx, "force-quit".to_string(), Modifiers::none());

    let asked = view.read_with(cx, |app, _| app.confirm.as_ref().map(|c| c.pids.clone()));
    assert_eq!(
        Some(sorted(vec![pids[0], pids[2]])),
        asked.map(sorted),
        "Force Quit asks about every selected process, and nothing is signalled before confirming"
    );
}

#[gpui::test]
fn selection_survives_refresh_and_drops_exited_processes(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-refresh");
    let pids = view.read_with(cx, |app, _| process_pids(app));
    click_with(cx, format!("proc-row-{}", pids[0]), Modifiers::none());
    click_with(cx, format!("proc-row-{}", pids[1]), cmd());

    view.update(cx, |app, cx| {
        app.rebuild_rows();
        app.drop_exited();
        cx.notify();
    });
    assert_eq!(
        sorted(vec![pids[0], pids[1]]),
        selection(&view, cx),
        "a refresh keeps the selection"
    );

    view.update(cx, |app, cx| {
        app.selection.insert(RowId::Pid(u32::MAX));
        app.drop_exited();
        cx.notify();
    });
    assert_eq!(
        sorted(vec![pids[0], pids[1]]),
        selection(&view, cx),
        "a process that exited leaves the selection, the others stay"
    );
}

#[gpui::test]
fn listing_clicks_keys_and_bulk_copy(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-listing");
    let rows: Vec<ListRow> = (0..4)
        .map(|i| list_row(&format!("k{i}"), &format!("row {i}")))
        .collect();
    set_listing(&view, cx, Page::Users, rows);

    click_with(cx, "list-row-0".to_string(), Modifiers::none());
    assert_eq!(vec!["k0".to_string()], list_selection(&view, cx));
    assert!(
        cx.debug_bounds("bulk-copy").is_none(),
        "bulk buttons need two selected rows"
    );

    click_with(cx, "list-row-2".to_string(), shift());
    click_with(cx, "list-row-3".to_string(), cmd());
    click_with(cx, "list-row-1".to_string(), cmd());
    assert_eq!(
        vec!["k0".to_string(), "k2".to_string(), "k3".to_string()],
        list_selection(&view, cx),
        "Shift-click ranges, ⌘-click toggles"
    );
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("bulk-copy").is_some(),
        "bulk Copy appears for several rows"
    );

    cx.dispatch_action(SelectAll);
    cx.run_until_parked();
    assert_eq!(4, list_selection(&view, cx).len(), "⌘A selects every row");

    cx.dispatch_action(CopySelected);
    cx.run_until_parked();
    let text = cx
        .read_from_clipboard()
        .and_then(|c| c.text())
        .expect("clipboard holds text");
    assert_eq!("row 0\nrow 1\nrow 2\nrow 3", text, "one line per row");

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        1,
        list_selection(&view, cx).len(),
        "Esc collapses the listing too"
    );
}

#[gpui::test]
fn listing_arrows_move_and_extend(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-listing-keys");
    let rows: Vec<ListRow> = (0..4)
        .map(|i| list_row(&format!("k{i}"), &format!("row {i}")))
        .collect();
    set_listing(&view, cx, Page::Users, rows);

    click_with(cx, "list-row-0".to_string(), Modifiers::none());
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    assert_eq!(vec!["k1".to_string()], list_selection(&view, cx));

    cx.simulate_keystrokes("shift-down");
    cx.run_until_parked();
    assert_eq!(
        vec!["k1".to_string(), "k2".to_string()],
        list_selection(&view, cx),
        "Shift-↓ extends the listing selection"
    );
}

#[gpui::test]
fn bulk_buttons_group_each_rows_own_actions(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-bulk");
    let rows: Vec<ListRow> = (0..2)
        .map(|i| ListRow {
            actions: vec![ActionButton {
                label: "Restart",
                action: Action::Restart(format!("svc{i}")),
                danger: false,
            }],
            ..list_row(&format!("svc{i}"), &format!("svc{i}"))
        })
        .collect();
    set_listing(&view, cx, Page::Services, rows);

    click_with(cx, "list-row-0".to_string(), Modifiers::none());
    click_with(cx, "list-row-1".to_string(), shift());
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("bulk-Restart").is_some(),
        "two rows that can restart get one Restart 2 button"
    );
    assert!(
        cx.debug_bounds("bulk-Stop").is_none(),
        "no bulk Stop when no selected row offers it"
    );
}

#[gpui::test]
fn apps_bulk_trash_confirms_once_and_cancel_keeps_the_apps(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "multi-trash");
    let rows: Vec<ListRow> = ["A", "B", "C"]
        .iter()
        .map(|n| list_row(&format!("/Applications/{n}.app"), n))
        .collect();
    set_listing(&view, cx, Page::Apps, rows);

    click_with(cx, "list-row-0".to_string(), Modifiers::none());
    click_with(cx, "list-row-2".to_string(), cmd());
    click_with(cx, "bulk-trash".to_string(), Modifiers::none());

    let asked = view.read_with(cx, |app, _| app.bulk_trash.clone());
    assert_eq!(
        Some(vec![
            std::path::PathBuf::from("/Applications/A.app"),
            std::path::PathBuf::from("/Applications/C.app"),
        ]),
        asked,
        "one confirmation for the selected apps"
    );
    assert!(
        cx.debug_bounds("trash-sheet").is_some(),
        "the sheet is shown"
    );

    click_with(cx, "trash-cancel".to_string(), Modifiers::none());
    assert!(
        view.read_with(cx, |app, _| app.bulk_trash.is_none()),
        "Cancel closes the sheet without trashing anything"
    );
}
