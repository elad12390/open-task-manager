//! Selecting any row, group rows included: ⌘/Shift-click and Shift-arrows over both kinds, actions
//! on the union of processes, selections that survive a refresh, and folding and selecting while
//! a filter is active.

use std::collections::HashSet;

use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};

use crate::{
    app::OtmApp,
    categories::OS_OWNER,
    columns::GroupBy,
    menus::SelectAll,
    metrics::ProcessInfo,
    processes::{Row, RowId, node_key},
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

fn click_with(cx: &mut VisualTestContext, selector: &str, modifiers: Modifiers) {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    let at = center(bounds(cx, selector));
    cx.simulate_click(at, modifiers);
    cx.run_until_parked();
}

fn right_click(cx: &mut VisualTestContext, selector: &str) {
    let selector: &'static str = Box::leak(selector.to_string().into_boxed_str());
    let at = center(bounds(cx, selector));
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.run_until_parked();
}

fn proc(pid: u32, name: &str, owner: &str, ppid: Option<u32>) -> ProcessInfo {
    ProcessInfo {
        pid,
        name: name.into(),
        path: format!("/usr/bin/{name}"),
        owner_app: owner.into(),
        user: "elad".into(),
        ppid,
        ..Default::default()
    }
}

/// Two macOS processes, a Safari tree and a lone Code process, grouped by app.
fn fixture() -> Vec<ProcessInfo> {
    vec![
        proc(1, "launchd", OS_OWNER, None),
        proc(2, "WindowServer", OS_OWNER, Some(1)),
        proc(10, "Safari", "Safari", Some(1)),
        proc(11, "Safari Web", "Safari", Some(10)),
        proc(12, "Safari Net", "Safari", Some(10)),
        proc(20, "Code", "Code", None),
    ]
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
    view.update_in(cx, |app, window, cx| {
        app.snapshot.processes = fixture();
        app.layout.group_by = GroupBy::App;
        app.rebuild_rows();
        window.focus(&app.focus);
        cx.notify();
    });
    cx.run_until_parked();
    (view, cx)
}

fn safari() -> RowId {
    RowId::Group("Safari".into())
}

fn code() -> RowId {
    RowId::Group("Code".into())
}

fn os() -> RowId {
    RowId::Group(OS_OWNER.into())
}

fn shown(app: &OtmApp) -> HashSet<RowId> {
    app.rows.iter().map(Row::id).collect()
}

fn sorted(mut pids: Vec<u32>) -> Vec<u32> {
    pids.sort();
    pids
}

#[gpui::test]
fn cmd_click_joins_group_rows_to_the_selection(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-cmd-group");
    click_with(cx, "group-row-Safari", Modifiers::none());
    click_with(cx, "group-row-Code", cmd());
    let (selection, pids) =
        view.read_with(cx, |app, _| (app.selection.clone(), app.selected_pids()));
    assert_eq!(selection, HashSet::from([safari(), code()]));
    assert_eq!(
        sorted(pids),
        vec![10, 11, 12, 20],
        "a selected group contributes every process inside it, once"
    );
}

#[gpui::test]
fn shift_click_selects_the_range_across_process_and_group_rows(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-shift-range");
    click_with(cx, "proc-row-2", Modifiers::none());
    click_with(cx, "group-row-Code", shift());
    let (selection, expected) = view.read_with(cx, |app, _| {
        let at = |id: &RowId| app.rows.iter().position(|r| &r.id() == id).unwrap();
        let (a, b) = (at(&RowId::Pid(2)), at(&code()));
        let range: HashSet<RowId> = app.rows[a.min(b)..=a.max(b)].iter().map(Row::id).collect();
        (app.selection.clone(), range)
    });
    assert_eq!(
        selection, expected,
        "Shift-click selects every row between anchor and target"
    );
    assert!(selection.iter().any(|id| matches!(id, RowId::Group(_))));
    assert!(selection.iter().any(|id| matches!(id, RowId::Pid(_))));
    assert!(selection.contains(&RowId::Pid(2)) && selection.contains(&code()));
}

#[gpui::test]
fn actions_use_the_union_of_processes_and_skip_macos_sections(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-union");
    view.update(cx, |app, _| {
        app.set_cursor(Some(RowId::Pid(11)));
        app.click_row(safari(), cmd());
    });
    let pids = view.read_with(cx, |app, _| app.selected_pids());
    assert_eq!(
        sorted(pids.clone()),
        vec![10, 11, 12],
        "a process already inside the selected group is listed once"
    );
    assert_eq!(pids.len(), 3);

    view.update(cx, |app, _| app.set_cursor(Some(os())));
    let (pids, skipped) = view.read_with(cx, |app, _| app.selected_pids_and_skipped());
    assert!(
        pids.is_empty(),
        "a selected macOS section never quits macOS's own processes"
    );
    assert_eq!(skipped, 2);

    view.update(cx, |app, _| app.click_row(code(), cmd()));
    let labels: Vec<String> = view.read_with(cx, |app, _| {
        app.context_items(&code())
            .into_iter()
            .map(|i| i.label)
            .collect()
    });
    assert!(
        labels
            .iter()
            .any(|l| l == "Quit 1 Process · 2 macOS skipped"),
        "the confirmation says what was skipped, got {labels:?}"
    );
    assert!(labels.iter().any(|l| l == "Copy 2 Rows"), "got {labels:?}");
}

#[gpui::test]
fn folding_with_left_while_filtering_removes_children_and_right_brings_them_back(
    cx: &mut TestAppContext,
) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-fold-filter");
    view.update(cx, |app, _| {
        app.query = "safari".into();
        app.query_changed();
        app.set_cursor(Some(RowId::Pid(10)));
    });
    assert!(view.read_with(cx, |app, _| shown(app).contains(&RowId::Pid(11))));

    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    let (rows, collapsed) = view.read_with(cx, |app, _| (shown(app), app.collapsed.clone()));
    assert!(!rows.contains(&RowId::Pid(11)) && !rows.contains(&RowId::Pid(12)));
    assert!(rows.contains(&RowId::Pid(10)));
    assert!(
        !collapsed.contains(&node_key(10)),
        "the fold while filtering does not touch the unfiltered fold state"
    );

    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    let rows = view.read_with(cx, |app, _| shown(app));
    assert!(rows.contains(&RowId::Pid(11)) && rows.contains(&RowId::Pid(12)));

    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    view.update(cx, |app, _| {
        app.query.clear();
        app.query_changed();
    });
    let rows = view.read_with(cx, |app, _| shown(app));
    assert!(
        rows.contains(&RowId::Pid(11)),
        "clearing the filter restores the unfiltered folds, which are all open"
    );
}

#[gpui::test]
fn down_from_the_focused_filter_field_moves_the_table_then_left_folds(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-filter-handoff");
    view.update_in(cx, |app, window, _| {
        app.query = "safari".into();
        app.query_changed();
        app.set_cursor(Some(safari()));
        app.focus_filter();
        window.focus(&app.focus);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, _| !app.suggest_open && app.filter_focused));

    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let (focused, query) = view.read_with(cx, |app, _| (app.filter_focused, app.query.clone()));
    assert!(
        !focused,
        "↓ with the suggestions closed hands the keyboard to the table"
    );
    assert_eq!(query, "safari", "the query is kept");

    for _ in 0..4 {
        if view.read_with(cx, |app, _| app.cursor == Some(RowId::Pid(10))) {
            break;
        }
        cx.simulate_keystrokes("down");
        cx.run_until_parked();
    }
    assert_eq!(
        view.read_with(cx, |app, _| app.cursor.clone()),
        Some(RowId::Pid(10))
    );

    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    let rows = view.read_with(cx, |app, _| shown(app));
    assert!(
        !rows.contains(&RowId::Pid(11)),
        "← folds the branch the table cursor is on"
    );
}

#[gpui::test]
fn select_all_while_filtering_selects_only_visible_rows(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-all-filter");
    view.update(cx, |app, _| {
        app.query = "code".into();
        app.query_changed();
    });
    cx.dispatch_action(SelectAll);
    cx.run_until_parked();
    let (selection, rows, pids) = view.read_with(cx, |app, _| {
        (app.selection.clone(), shown(app), app.selected_pids())
    });
    assert_eq!(
        selection, rows,
        "⌘A selects every visible row, groups included"
    );
    assert!(!selection.contains(&os()) && !selection.contains(&RowId::Pid(1)));
    assert_eq!(pids, vec![20]);
}

#[gpui::test]
fn group_selection_survives_refresh(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-refresh");
    view.update(cx, |app, _| {
        app.set_cursor(Some(safari()));
        app.click_row(code(), cmd());
        app.click_row(RowId::Pid(12), cmd());
    });
    view.update(cx, |app, _| {
        app.snapshot
            .processes
            .retain(|p| p.pid != 12 && p.pid != 20);
        app.drop_exited();
        app.rebuild_rows();
    });
    let selection = view.read_with(cx, |app, _| app.selection.clone());
    assert!(
        selection.contains(&safari()),
        "the group keeps its key and its selection"
    );
    assert!(
        !selection.contains(&RowId::Pid(12)),
        "an exited process drops out"
    );
    assert!(
        !selection.contains(&code()),
        "a group with no processes left drops out"
    );
}

#[gpui::test]
fn right_click_on_an_unselected_group_selects_just_it(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let (view, cx) = open(cx, "select-right-click");
    click_with(cx, "group-row-Safari", Modifiers::none());
    right_click(cx, "group-row-Code");
    assert_eq!(
        view.read_with(cx, |app, _| app.selection.clone()),
        HashSet::from([code()])
    );
    assert!(matches!(
        view.read_with(cx, |app, _| app.context_menu.clone()),
        Some((RowId::Group(ref key), _)) if key == "Code"
    ));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    click_with(cx, "group-row-Safari", cmd());
    right_click(cx, "group-row-Code");
    assert_eq!(
        view.read_with(cx, |app, _| app.selection.clone()),
        HashSet::from([code(), safari()]),
        "right-clicking inside the selection keeps it"
    );
}
