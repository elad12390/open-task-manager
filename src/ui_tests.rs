//! Headless UI tests: GPUI's test platform renders the real `OtmApp` into an in-memory window
//! (nothing appears on screen) and drives it with simulated mouse events.

use gpui::{
    Bounds, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};

use crate::{
    app::OtmApp,
    columns::{ColumnId, GroupBy, TableLayout},
};

/// Both tests point OTM_DATA_DIR at their own folder, so they must not overlap.
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) fn center(b: Bounds<Pixels>) -> Point<Pixels> {
    b.center()
}

pub(crate) fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.run_until_parked();
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("element {selector} was not rendered"))
}

pub(crate) fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let at = center(bounds(cx, selector));
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

/// Opens the Group menu and picks one of its items.
pub(crate) fn pick_group(cx: &mut VisualTestContext, item: &'static str) {
    click(cx, "group-btn");
    click(cx, item);
}

/// Press at `from`, move there in steps with the button held, release at `to`.
pub(crate) fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    for i in 1..=8 {
        let f = i as f32 / 8.;
        let at = point(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
}

fn order(app: &OtmApp) -> Vec<ColumnId> {
    app.layout.visible().map(|(_, c)| c.id).collect()
}

fn width(app: &OtmApp, id: ColumnId) -> f32 {
    app.layout
        .columns
        .iter()
        .find(|c| c.id == id)
        .unwrap()
        .width
}

#[gpui::test]
fn column_gestures(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-ui-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    let before = view.read_with(cx, |app, _| order(app));
    println!("start order: {before:?}");

    // 1. Drag the PID header onto the Name header: PID should move in front of Name.
    let pid = center(bounds(cx, "header-Pid"));
    let name = center(bounds(cx, "header-Name"));
    drag(cx, pid, name);
    let after_drag = view.read_with(cx, |app, _| order(app));
    println!("after header drag: {after_drag:?}");
    assert_eq!(
        after_drag[0],
        ColumnId::Pid,
        "PID header should now be first"
    );
    assert_eq!(after_drag[1], ColumnId::Name);

    // 2. Drag the right edge of the CPU header 60 px to the right: CPU gets 60 px wider.
    let cpu_before = view.read_with(cx, |app, _| width(app, ColumnId::Cpu));
    let edge = center(bounds(cx, "resize-Cpu"));
    drag(cx, edge, point(edge.x + px(60.), edge.y));
    let cpu_after = view.read_with(cx, |app, _| width(app, ColumnId::Cpu));
    println!("CPU width {cpu_before} -> {cpu_after}");
    assert!(
        (cpu_after - cpu_before - 60.).abs() < 1.5,
        "CPU column should be ~60 px wider"
    );

    // 3. Columns menu: open it, show Path, hide User.
    click(cx, "columns-btn");
    assert!(
        view.read_with(cx, |app, _| app.column_menu),
        "Columns menu should be open"
    );
    click(cx, "col-check-Path");
    click(cx, "col-check-User");
    let after_menu = view.read_with(cx, |app, _| order(app));
    println!("after Columns menu: {after_menu:?}");
    assert!(after_menu.contains(&ColumnId::Path), "Path should be shown");
    assert!(
        !after_menu.contains(&ColumnId::User),
        "User should be hidden"
    );

    // 4. Group by Owner from the toolbar.
    view.update(cx, |app, cx| {
        app.column_menu = false;
        cx.notify();
    });
    pick_group(cx, "group-User");
    let (group, has_groups) = view.read_with(cx, |app, _| {
        (
            app.layout.group_by,
            app.rows
                .iter()
                .any(|r| matches!(r, crate::processes::Row::Group { .. })),
        )
    });
    println!("group by: {group:?}, group rows present: {has_groups}");
    assert_eq!(group, GroupBy::User);

    // 5. Everything above was saved: a fresh load from disk matches.
    let saved = TableLayout::load();
    let saved_order: Vec<ColumnId> = saved.visible().map(|(_, c)| c.id).collect();
    let saved_cpu = saved
        .columns
        .iter()
        .find(|c| c.id == ColumnId::Cpu)
        .unwrap()
        .width;
    println!(
        "saved to {}: order {saved_order:?}, CPU width {saved_cpu}, group {:?}",
        dir.display(),
        saved.group_by
    );
    assert_eq!(saved_order, after_menu);
    assert!((saved_cpu - cpu_after).abs() < 0.01);
    assert_eq!(saved.group_by, GroupBy::User);

    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn process_tree_twisty(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-tree-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    pick_group(cx, "group-Parent");
    let count = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| {
            app.rows
                .iter()
                .filter(|r| matches!(r, crate::processes::Row::Process { .. }))
                .count()
        })
    };
    let expanded = count(cx);
    let launchd = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| {
            app.rows.iter().find_map(|r| match r {
                crate::processes::Row::Process {
                    info,
                    depth,
                    descendants,
                    collapsed,
                    ..
                } if info.pid == 1 => Some((*depth, *descendants, *collapsed)),
                _ => None,
            })
        })
    };
    let (depth, descendants, _) = launchd(cx).expect("launchd row");
    println!("tree: {expanded} rows; launchd at depth {depth} with {descendants} descendants");
    assert_eq!(depth, 0);

    click(cx, "twisty-1");
    let folded = count(cx);
    let (_, hidden, now_collapsed) = launchd(cx).expect("launchd row");
    println!(
        "after clicking launchd's ▾: {folded} rows, launchd collapsed={now_collapsed} hiding {hidden}"
    );
    assert!(now_collapsed);
    assert_eq!(
        folded,
        expanded - hidden,
        "exactly launchd's descendants are hidden"
    );

    click(cx, "twisty-1");
    let sections: Vec<(String, bool)> = view.read_with(cx, |app, _| {
        app.rows
            .iter()
            .filter_map(|r| match r {
                crate::processes::Row::Group {
                    label, collapsed, ..
                } => Some((label.clone(), *collapsed)),
                _ => None,
            })
            .collect()
    });
    println!("after clicking ▸ again: launchd shows {sections:?}");
    assert_eq!(
        sections,
        [
            ("Operating System".to_string(), true),
            ("Your Processes".to_string(), true)
        ],
        "one level opens: launchd's two sections, each still folded"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn keyboard_navigation(cx: &mut TestAppContext) {
    use crate::processes::{Row, RowId};
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-keys-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update_in(cx, |app, window, cx| {
        window.focus(&app.focus);
        cx.notify();
    });
    pick_group(cx, "group-Parent");

    // (cursor, its row index, every row's (id, depth, label))
    let state = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| {
            let rows: Vec<(RowId, usize, String)> = app
                .rows
                .iter()
                .map(|r| {
                    let label = match r {
                        Row::Process { info, .. } => info.name.clone(),
                        Row::Group { label, .. } => format!("[{label}]"),
                    };
                    (r.id(), r.depth(), label)
                })
                .collect();
            let pos = app
                .cursor
                .as_ref()
                .and_then(|c| rows.iter().position(|r| &r.0 == c));
            (app.cursor.clone(), pos, rows)
        })
    };
    let key = |cx: &mut VisualTestContext, keys: &str| {
        cx.simulate_keystrokes(keys);
        cx.run_until_parked();
    };
    let label = |cx: &mut VisualTestContext| {
        let (_, pos, rows) = state(cx);
        pos.map(|p| rows[p].2.clone()).unwrap_or_default()
    };

    key(cx, "down");
    let (cur, _, rows) = state(cx);
    println!(
        "tree starts: {:?}",
        rows.iter().take(4).map(|r| (&r.2, r.1)).collect::<Vec<_>>()
    );
    println!("down -> {:?}", label(cx));
    assert_eq!(cur, Some(RowId::Pid(1)), "launchd is the first row");
    assert_eq!(
        rows[1].2, "[Operating System]",
        "the Operating System section is a node right under launchd"
    );
    assert_eq!(rows[1].1, 1, "…indented one level, like a child of launchd");

    // The cursor stops on group rows too.
    key(cx, "down");
    println!("down -> {:?}", label(cx));
    assert_eq!(
        state(cx).0,
        Some(RowId::Group("split:0".into())),
        "Down lands on the Operating System section"
    );
    key(cx, "down down");
    assert_eq!(state(cx).1, Some(3));
    key(cx, "up");
    assert_eq!(state(cx).1, Some(2));
    let (cur, _, rows) = state(cx);
    println!(
        "up -> {:?} (a subsystem node inside Operating System, depth {})",
        label(cx),
        rows[2].1
    );
    assert!(
        matches!(cur, Some(RowId::Group(ref k)) if k.starts_with("split:0/")),
        "row 2 is a subsystem node"
    );

    // Left on the open subsystem node folds it; Left again jumps up to the section; Left there folds it; Right opens it.
    key(cx, "left");
    key(cx, "left");
    println!("left, left -> {:?}", label(cx));
    assert_eq!(state(cx).0, Some(RowId::Group("split:0".into())));
    view.update(cx, |app, cx| {
        app.collapsed.clear();
        app.rebuild_rows();
        cx.notify();
    });
    let before = state(cx).2.len();
    key(cx, "left");
    let folded_rows = state(cx).2;
    let folded = folded_rows.len();
    println!("left on the section -> folded, {before} -> {folded} rows");
    // Folded: fewer rows, and nothing inside the section follows it (independent of how many
    // processes this Mac happens to run).
    assert!(folded < before);
    let at = folded_rows
        .iter()
        .position(|r| r.0 == RowId::Group("split:0".into()))
        .unwrap();
    assert!(
        folded_rows
            .get(at + 1)
            .is_none_or(|next| next.1 <= folded_rows[at].1)
    );
    key(cx, "right");
    let one_level = state(cx).2.len();
    assert!(
        one_level > folded && one_level <= before,
        "Right opens the section one level"
    );
    key(cx, "space");
    assert_eq!(state(cx).2.len(), folded, "Space folds the section");
    key(cx, "space");
    assert_eq!(
        state(cx).2.len(),
        one_level,
        "Space opens it one level again"
    );
    println!(
        "right / space / space on the section -> {one_level}, {folded}, {one_level} rows (fully open: {before})"
    );

    // Right on an open section steps onto its first subsystem node; Left walks back up to launchd.
    key(cx, "right");
    let (cur, pos, rows) = state(cx);
    println!(
        "right on the open section -> {:?} (depth {})",
        label(cx),
        rows[pos.unwrap()].1
    );
    assert!(matches!(cur, Some(RowId::Group(_))));
    assert_eq!(rows[pos.unwrap()].1, 2);
    let mut presses = 0;
    while state(cx).0 != Some(RowId::Pid(1)) && presses < 8 {
        key(cx, "left");
        presses += 1;
    }
    println!("left x{presses} (folding on the way) -> {:?}", label(cx));
    assert_eq!(
        state(cx).0,
        Some(RowId::Pid(1)),
        "Left walks up through the section to launchd"
    );
    view.update(cx, |app, cx| {
        app.collapsed.clear();
        app.rebuild_rows();
        cx.notify();
    });
    assert_eq!(state(cx).2.len(), before);

    // End / Home / Page keys cover every row, scrolling as they go.
    key(cx, "end");
    let (_, pos, rows) = state(cx);
    let offset_end = view.read_with(cx, |app, _| {
        f32::from(app.scroll.0.borrow().base_handle.offset().y)
    });
    println!(
        "end -> row {pos:?} of {}; list scrolled to y={offset_end}",
        rows.len()
    );
    assert_eq!(pos, Some(rows.len() - 1));
    assert!(offset_end < -100.);
    key(cx, "home");
    assert_eq!(state(cx).1, Some(0));
    key(cx, "pagedown");
    let paged = state(cx).1.unwrap();
    println!("home, pagedown -> row {paged}");
    assert!(paged > 5);
    key(cx, "pageup");
    assert_eq!(state(cx).1, Some(0));

    // Space on launchd folds the whole tree to one row and reopens one level.
    let total = state(cx).2.len();
    key(cx, "space");
    assert_eq!(state(cx).2.len(), 1);
    key(cx, "space");
    let reopened = state(cx).2.len();
    println!("space, space on launchd -> {total} rows to 1, then {reopened}");
    assert!(reopened > 2 && reopened < total);
    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn folding_folds_the_whole_batch(cx: &mut TestAppContext) {
    use crate::processes::{Row, branch_descendants, node_key};
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-batch-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update_in(cx, |app, window, cx| {
        window.focus(&app.focus);
        cx.notify();
    });
    pick_group(cx, "group-Parent");
    // (rows shown, max depth shown, branches shown folded, branches shown unfolded)
    let shape = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| {
            let mut out = (0, 0, 0, 0);
            for r in app.rows.iter() {
                // Groups (sections, owner and subsystem nodes) are tree nodes too.
                let (depth, foldable, collapsed) = match r {
                    Row::Process {
                        depth,
                        descendants,
                        collapsed,
                        ..
                    } => (*depth, *descendants > 0, *collapsed),
                    Row::Group {
                        depth, collapsed, ..
                    } => (*depth, true, *collapsed),
                };
                out.0 += 1;
                out.1 = out.1.max(depth);
                if foldable {
                    if collapsed { out.2 += 1 } else { out.3 += 1 }
                }
            }
            out
        })
    };
    let total = shape(cx).0;

    // Fold launchd: every branch below it is folded too.
    click(cx, "twisty-1");
    let (below, folded_below) = view.read_with(cx, |app, _| {
        let below = branch_descendants(&app.snapshot.processes, 1);
        let folded = below
            .iter()
            .filter(|&&p| app.collapsed.contains(&node_key(p)))
            .count();
        (below.len(), folded)
    });
    println!(
        "fold launchd: {} rows shown; {folded_below} of {below} branches under it are folded",
        shape(cx).0
    );
    assert_eq!(shape(cx).0, 1);
    assert!(below > 5);
    assert_eq!(
        folded_below, below,
        "every branch below launchd must be folded with it"
    );

    // Unfold launchd once: only the next level appears (its two sections), each still folded.
    click(cx, "twisty-1");
    let (rows, depth, folded, open) = shape(cx);
    println!(
        "unfold launchd: {rows} rows, deepest shown depth {depth}, {folded} nodes folded, {open} open (launchd)"
    );
    assert_eq!(
        (rows, depth, folded, open),
        (3, 1, 2, 1),
        "launchd + its two folded sections"
    );

    // Option-click unfolds the whole batch at once.
    click(cx, "twisty-1"); // fold again
    let at = center(bounds(cx, "twisty-1"));
    cx.simulate_click(
        at,
        Modifiers {
            alt: true,
            ..Modifiers::none()
        },
    );
    cx.run_until_parked();
    let (rows, _, folded, _) = shape(cx);
    println!("option-click launchd's ▸: {rows} rows, {folded} folded branches");
    assert_eq!((rows, folded), (total, 0));

    // Keyboard: Space folds the batch, Option-→ unfolds everything.
    view.update(cx, |app, cx| {
        app.selected = Some(1);
        cx.notify();
    });
    cx.simulate_keystrokes("space");
    cx.run_until_parked();
    let after_space = shape(cx).0;
    cx.simulate_keystrokes("alt-right");
    cx.run_until_parked();
    let (rows, _, folded, _) = shape(cx);
    println!("space -> {after_space} row; option-right -> {rows} rows, {folded} folded");
    assert_eq!(after_space, 1);
    assert_eq!((rows, folded), (total, 0));
    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn category_grouping(cx: &mut TestAppContext) {
    use crate::processes::Row;
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-cat-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    pick_group(cx, "group-Category");
    let heads = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| {
            let groups: Vec<(usize, String, usize, Option<&'static str>)> = app
                .rows
                .iter()
                .filter_map(|r| match r {
                    Row::Group {
                        depth,
                        label,
                        count,
                        note,
                        ..
                    } => Some((*depth, label.clone(), *count, *note)),
                    _ => None,
                })
                .collect();
            (groups, app.rows.len())
        })
    };
    let (groups, total_rows) = heads(cx);
    let tops: Vec<&(usize, String, usize, Option<&'static str>)> =
        groups.iter().filter(|g| g.0 == 0).collect();
    println!(
        "Category view: {total_rows} rows; top groups {:?}",
        tops.iter().map(|g| (&g.1, g.2, g.3)).collect::<Vec<_>>()
    );
    assert_eq!(tops[0].1, "macOS");
    assert_eq!(tops[0].3, Some("built into the system"));
    let macos_sections = groups.iter().skip(1).take_while(|g| g.0 == 1).count();
    assert!(macos_sections >= 6, "macOS should be split into sections");

    // Clicking the macOS header folds the whole macOS group.
    click(cx, "group-twisty-cat:macOS");
    let (after, rows_after) = heads(cx);
    let hidden = total_rows - rows_after;
    println!(
        "click the macOS ▾: {total_rows} -> {rows_after} rows ({hidden} hidden, {macos_sections} sections inside)"
    );
    assert!(
        hidden > tops[0].2,
        "the macOS group's sections and processes are hidden"
    );
    assert_eq!(after[0].0, 0);
    assert_eq!(after[1].1, "Apps", "the next row is the Apps group");
    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn folding_a_group_folds_everything_inside(cx: &mut TestAppContext) {
    use crate::processes::{Row, RowId};
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-groupfold-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update_in(cx, |app, window, cx| {
        window.focus(&app.focus);
        cx.notify();
    });
    pick_group(cx, "group-Parent");
    let os = RowId::Group("split:0".into());
    // Rows inside the Operating System section: (depth, has children, folded)
    let inside = |cx: &mut VisualTestContext, key: &RowId| {
        view.read_with(cx, |app, _| {
            let start = app.rows.iter().position(|r| &r.id() == key).unwrap();
            let base = app.rows[start].depth();
            app.rows[start + 1..]
                .iter()
                .take_while(|r| r.depth() > base)
                .map(|r| match r {
                    Row::Process {
                        depth,
                        descendants,
                        collapsed,
                        ..
                    } => (*depth, *descendants > 0, *collapsed),
                    Row::Group {
                        depth, collapsed, ..
                    } => (*depth, true, *collapsed),
                })
                .collect::<Vec<_>>()
        })
    };
    let total = view.read_with(cx, |app, _| app.rows.len());
    let open_before = inside(cx, &os);
    let deepest_before = open_before.iter().map(|r| r.0).max().unwrap_or(0);

    view.update(cx, |app, cx| {
        app.set_cursor(Some(os.clone()));
        cx.notify();
    });
    cx.simulate_keystrokes("space"); // fold the section
    cx.run_until_parked();
    let folded_rows = view.read_with(cx, |app, _| app.rows.len());
    cx.simulate_keystrokes("space"); // unfold one level
    cx.run_until_parked();
    let after = inside(cx, &os);
    let branches = after.iter().filter(|r| r.1).count();
    let still_folded = after.iter().filter(|r| r.1 && r.2).count();
    let deepest_after = after.iter().map(|r| r.0).max().unwrap_or(0);
    println!(
        "Operating System: {} rows inside, down to depth {deepest_before} -> fold -> {folded_rows} rows in the list -> unfold -> {} rows inside, deepest depth {deepest_after}, {still_folded} of {branches} branches still folded",
        open_before.len(),
        after.len()
    );
    assert!(deepest_before >= 3, "the section starts fully open");
    assert_eq!(
        deepest_after, 2,
        "unfolding opens one level: only the section's own processes"
    );
    assert!(
        branches >= 1 && still_folded == branches,
        "every branch inside was folded with the section"
    );

    cx.simulate_keystrokes("alt-right"); // open everything inside
    cx.run_until_parked();
    let reopened = view.read_with(cx, |app, _| app.rows.len());
    println!("option-right on the section -> {reopened} rows (was {total})");
    assert_eq!(reopened, total);

    // Category view: folding macOS folds its sections; unfolding shows them, each folded.
    pick_group(cx, "group-Category");
    let macos = RowId::Group("cat:macOS".into());
    click(cx, "group-twisty-cat:macOS");
    click(cx, "group-twisty-cat:macOS");
    let sections = inside(cx, &macos);
    println!(
        "macOS folded and unfolded -> {} rows inside: {} sections, all folded: {}",
        sections.len(),
        sections.iter().filter(|r| r.0 == 1).count(),
        sections.iter().all(|r| r.0 == 1 && r.2)
    );
    assert!(sections.len() >= 6);
    assert!(
        sections.iter().all(|r| r.0 == 1 && r.2),
        "only the sections show, each folded"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn table_scrolls_sideways_to_every_column(cx: &mut TestAppContext) {
    use gpui::{ScrollDelta, ScrollWheelEvent};
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-hscroll-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    // Show every column, so the table is wider than the window.
    view.update(cx, |app, cx| {
        for c in app.layout.columns.iter_mut() {
            c.visible = true;
        }
        app.layout_changed(cx);
    });
    let table = bounds(cx, "process-table");
    let width = view.read_with(cx, |app, _| app.table_width());
    let name_before = bounds(cx, "header-Name");
    let path_before = bounds(cx, "header-Path");
    println!(
        "table area {:.0} px wide, columns need {width:.0} px; Path header starts at x={:.0} (area ends at {:.0})",
        f32::from(table.size.width),
        f32::from(path_before.origin.x),
        f32::from(table.right())
    );
    assert!(
        width > f32::from(table.size.width),
        "all columns are wider than the window"
    );
    assert!(
        path_before.right() > table.right(),
        "the last column is cut off before scrolling"
    );

    let wheel = |cx: &mut VisualTestContext, dx: f32, dy: f32| {
        cx.simulate_event(ScrollWheelEvent {
            position: table.center(),
            delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
            ..Default::default()
        });
        cx.run_until_parked();
    };
    let list_y = |cx: &mut VisualTestContext| {
        view.read_with(cx, |app, _| {
            f32::from(app.scroll.0.borrow().base_handle.offset().y)
        })
    };
    let table_x =
        |cx: &mut VisualTestContext| view.read_with(cx, |app, _| f32::from(app.hscroll.offset().x));

    // Swipe left (content moves left) far enough to reach the end.
    let y0 = list_y(cx);
    wheel(cx, -5000., 0.);
    let x = table_x(cx);
    let name_after = bounds(cx, "header-Name");
    let path_after = bounds(cx, "header-Path");
    println!(
        "sideways swipe: table scrolled to x={x:.0}; Name header moved {:.0} -> {:.0}; Path header now {:.0}..{:.0}; rows' vertical offset {y0} -> {}",
        f32::from(name_before.origin.x),
        f32::from(name_after.origin.x),
        f32::from(path_after.origin.x),
        f32::from(path_after.right()),
        list_y(cx)
    );
    assert!(x < -10., "the table scrolled sideways");
    assert!(
        name_after.origin.x < name_before.origin.x,
        "the header moved with it"
    );
    assert!(
        path_after.right() <= table.right() + px(1.),
        "the last column is fully visible"
    );
    assert_eq!(
        list_y(cx),
        y0,
        "a sideways swipe does not scroll the rows vertically"
    );

    // A vertical scroll moves the rows, not the table sideways.
    wheel(cx, 0., -400.);
    println!(
        "vertical scroll: rows' offset {y0} -> {}; table x stays {}",
        list_y(cx),
        table_x(cx)
    );
    assert!(list_y(cx) < y0, "rows scroll vertically");
    assert_eq!(
        table_x(cx),
        x,
        "a vertical scroll does not move the table sideways"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[gpui::test]
fn right_click_menu_and_icons(cx: &mut TestAppContext) {
    use crate::processes::RowId;
    use std::time::Duration;
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-ctx-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    // A disposable process to act on; killed when the test ends, pass or fail.
    struct Disposable(std::process::Child);
    impl Drop for Disposable {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut victim = Disposable(
        std::process::Command::new("/bin/sleep")
            .arg("600")
            .spawn()
            .unwrap(),
    );
    let pid = victim.0.id();

    let (view, cx) = cx.add_window_view(OtmApp::new);
    view.update(cx, |app, _| app.icons_enabled = true);
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.layout.group_by = crate::columns::GroupBy::None;
        // Only the disposable process, whatever else is running.
        app.query = pid.to_string();
        app.rebuild_rows();
        cx.notify();
    });
    let row: &'static str = Box::leak(format!("proc-row-{pid}").into_boxed_str());
    let resample = |cx: &mut VisualTestContext| {
        view.update(cx, |app, _| app.update_now = true);
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
    };
    let open_menu = |cx: &mut VisualTestContext| {
        let at = center(bounds(cx, row));
        cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
        cx.run_until_parked();
    };

    // Right-click opens the menu on that process; Escape closes it.
    open_menu(cx);
    let (menu, labels) = view.read_with(cx, |app, _| {
        let menu = app.context_menu.as_ref().map(|m| m.0.clone());
        let labels: Vec<String> = app
            .context_items(&RowId::Pid(pid))
            .into_iter()
            .map(|i| i.label)
            .filter(|l| !l.is_empty())
            .collect();
        (menu, labels)
    });
    println!("right-click sleep {pid}: menu for {menu:?}: {labels:?}");
    assert_eq!(menu, Some(RowId::Pid(pid)));
    assert!(
        cx.debug_bounds("context-menu").is_some(),
        "the menu is drawn"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        view.read_with(cx, |app, _| app.context_menu.is_none()),
        "Escape closes the menu"
    );

    // Suspend, then Resume.
    open_menu(cx);
    click(cx, "ctx-Suspend");
    let suspended = crate::sysops::process_state(pid).unwrap_or_default();
    resample(cx);
    open_menu(cx);
    click(cx, "ctx-Resume");
    let resumed = crate::sysops::process_state(pid).unwrap_or_default();
    println!("Suspend -> ps state {suspended:?}; Resume -> {resumed:?}");
    assert!(suspended.contains('T'), "Suspend stops the process");
    assert!(!resumed.contains('T'), "Resume continues it");

    // Copy PID.
    resample(cx);
    open_menu(cx);
    click(cx, "ctx-Copy PID");
    let copied = cx.read_from_clipboard().and_then(|c| c.text());
    println!("Copy PID -> clipboard {copied:?}");
    assert_eq!(copied, Some(pid.to_string()));

    // Force quit asks first: Esc (or Return, the default Cancel) leaves the process running.
    open_menu(cx);
    click(cx, "ctx-Force Quit…");
    assert!(
        cx.debug_bounds("confirm").is_some(),
        "the confirmation is shown"
    );
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |app, _| app.confirm.is_none()));
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        victim.0.try_wait().unwrap().is_none(),
        "cancelled: still running"
    );

    // Confirming it ends the process.
    open_menu(cx);
    click(cx, "ctx-Force Quit…");
    click(cx, "confirm-force-quit");
    let mut status = None;
    for _ in 0..50 {
        status = victim.0.try_wait().unwrap();
        if status.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let status = status.expect("Force Quit should end the process within 5 s");
    let notice = view.read_with(cx, |app, _| app.notice.clone());
    println!("Force Quit -> sleep exited with {status:?}; notice {notice:?}");
    assert!(!status.success());
    assert_eq!(notice, Some(("Force quit 1 process".into(), false)));

    // macOS's own group cannot be mass-killed from the menu: those items are not offered at all.
    let os_items = view.read_with(cx, |app, _| {
        app.context_items(&RowId::Group(crate::categories::OS_OWNER.into()))
            .into_iter()
            .map(|i| i.label)
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
    });
    println!("Operating System group menu: {os_items:?}");
    assert!(!os_items.is_empty(), "the group still has a menu");
    assert!(
        os_items.iter().all(|l| !l.contains("Quit")),
        "no Quit All for the OS"
    );

    // App icons were rendered to 32x32 PNGs.
    let icons = view.read_with(cx, |app, _| app.icons.clone());
    let pngs = icons
        .values()
        .filter(|p| {
            std::fs::read(p)
                .is_ok_and(|b| b.starts_with(b"\x89PNG") && b[16..24] == [0, 0, 0, 32, 0, 0, 0, 32])
        })
        .count();
    println!(
        "icons: {} app bundles, {pngs} valid 32x32 PNGs, e.g. {:?}",
        icons.len(),
        icons.keys().take(4).collect::<Vec<_>>()
    );
    assert!(icons.len() >= 3 && pngs == icons.len());
    std::fs::remove_dir_all(&dir).ok();
}

fn labels(state: &crate::menus::MenuState) -> Vec<String> {
    fn walk(items: &[gpui::MenuItem], out: &mut Vec<String>) {
        for item in items {
            match item {
                gpui::MenuItem::Action { name, .. } => out.push(name.to_string()),
                gpui::MenuItem::Submenu(m) => walk(&m.items, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for menu in crate::menus::build(state) {
        out.push(format!("[{}]", menu.name));
        walk(&menu.items, &mut out);
    }
    out
}

#[gpui::test]
fn menu_bar_actions(cx: &mut TestAppContext) {
    use crate::menus::*;
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-menu-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    let tops: Vec<String> = labels(&view.read_with(cx, |app, _| app.menu_state.clone().unwrap()))
        .into_iter()
        .filter(|l| l.starts_with('['))
        .collect();
    println!("menu bar: {tops:?}");
    assert_eq!(
        tops,
        [
            "[Open Task Manager]",
            "[File]",
            "[Edit]",
            "[View]",
            "[Update]",
            "[Window]",
            "[Help]"
        ]
    );

    cx.dispatch_action(Every5s);
    cx.dispatch_action(GroupOwner);
    cx.dispatch_action(AppearanceDark);
    cx.dispatch_action(TextBigger);
    cx.dispatch_action(ShowUsers);
    cx.run_until_parked();
    let (secs, group, appearance, scale, page) = view.read_with(cx, |app, _| {
        (
            app.settings.refresh_secs,
            app.layout.group_by,
            app.settings.appearance,
            app.settings.text_scale,
            app.page,
        )
    });
    println!(
        "after menu actions: every {secs}s, group {group:?}, appearance {appearance:?}, text {scale}%, page {page:?}"
    );
    assert_eq!(
        (secs, group, appearance, scale, page),
        (
            5,
            GroupBy::App,
            crate::settings::Appearance::Dark,
            115,
            crate::app::Page::Users
        )
    );

    cx.dispatch_action(TogglePause);
    cx.run_until_parked();
    let shown = labels(&view.read_with(cx, |app, _| app.menu_state.clone().unwrap()));
    let checked: Vec<&String> = shown.iter().filter(|l| l.starts_with('✓')).collect();
    println!("checked items: {checked:?}");
    assert!(view.read_with(cx, |app, _| app.paused));
    assert!(shown.contains(&"✓ Pause Updates".to_string()));
    assert!(shown.contains(&"✓ Owner".to_string()));
    assert!(shown.contains(&"✓ Dark".to_string()));
    assert!(shown.contains(&"✓ Users".to_string()));
    assert!(
        !shown.iter().any(|l| l.starts_with("✓ Every")),
        "no interval is ticked while paused"
    );

    cx.dispatch_action(ToggleRecording);
    cx.run_until_parked();
    let shown = labels(&view.read_with(cx, |app, _| app.menu_state.clone().unwrap()));
    assert!(
        shown.contains(&"Stop and Save".to_string()),
        "File menu offers Stop while recording"
    );

    let saved = crate::settings::Settings::load();
    println!(
        "saved settings: every {}s, text {}%",
        saved.refresh_secs, saved.text_scale
    );
    assert_eq!((saved.refresh_secs, saved.text_scale), (5, 115));
    std::fs::remove_dir_all(&dir).ok();
}

/// The filter field: a caret when focused, suggestions while typing, chips from Enter, ⌫ and Esc,
/// and a toolbar that fits the narrowest window.
#[gpui::test]
fn filter_field_chips_and_toolbar(cx: &mut TestAppContext) {
    use crate::filter::Chip;
    use crate::processes::Row;
    use std::time::Duration;

    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-ui-filter-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.layout.group_by = GroupBy::None;
        app.rebuild_rows();
        cx.notify();
    });
    let total = view.read_with(cx, |app, _| app.snapshot.processes.len());
    assert!(
        total > 10,
        "the live snapshot is loaded ({total} processes)"
    );

    // Clicking the field focuses it: caret, focus ring, and the "filter by" suggestions.
    click(cx, "filter-field");
    assert!(view.read_with(cx, |app, _| app.filter_focused && app.suggest_open));
    assert!(
        cx.debug_bounds("filter-suggestions").is_some(),
        "suggestions are drawn"
    );
    let blink = view.read_with(cx, |app, _| app.caret_on);
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    assert_ne!(
        view.read_with(cx, |app, _| app.caret_on),
        blink,
        "the caret blinks"
    );

    // Typing narrows the suggestions; Enter turns the first one into a chip.
    cx.simulate_input("root");
    let first = view.read_with(cx, |app, _| app.suggestions()[0].chip.clone());
    println!("first suggestion for \"root\": {first:?}");
    assert_eq!(first, Chip::User("root".into()));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let (chips, query, users) = view.read_with(cx, |app, _| {
        let users: std::collections::HashSet<String> = app
            .rows
            .iter()
            .filter_map(|r| match r {
                Row::Process { info, .. } => Some(info.user.clone()),
                _ => None,
            })
            .collect();
        (app.chips.clone(), app.query.clone(), users)
    });
    println!("chips {chips:?}, rows by users {users:?}");
    assert_eq!(chips, vec![Chip::User("root".into())]);
    assert!(query.is_empty(), "the typed text became the chip");
    assert_eq!(
        users.into_iter().collect::<Vec<_>>(),
        vec!["root".to_string()]
    );
    assert!(
        cx.debug_bounds("filter-chip-0").is_some(),
        "the chip is drawn"
    );

    // ⌫ on an empty field removes the last chip.
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert!(view.read_with(cx, |app, _| app.chips.is_empty()));

    // Numeric filters: "cpu" offers thresholds.
    cx.simulate_input("cpu");
    let first = view.read_with(cx, |app, _| app.suggestions()[0].chip.clone());
    assert_eq!(first, Chip::CpuAbove(1.));

    // Esc closes the panel, then clears the text, then leaves the field.
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |app, _| !app.suggest_open && app.query == "cpu"));
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |app, _| app.query.is_empty() && app.filter_focused));
    cx.simulate_keystrokes("escape");
    assert!(view.read_with(cx, |app, _| !app.filter_focused));

    // A filter that matches nothing shows the empty state, and its button clears the filter.
    cx.simulate_input("zzzz-no-such-process");
    assert!(view.read_with(cx, |app, _| app.rows.is_empty()));
    view.update(cx, |app, cx| {
        app.suggest_open = false;
        cx.notify();
    });
    click(cx, "empty-clear");
    assert!(view.read_with(cx, |app, _| !app.filter_active() && !app.rows.is_empty()));

    // Clicking a row leaves the field.
    click(cx, "filter-field");
    view.update(cx, |app, cx| {
        app.suggest_open = false;
        cx.notify();
    });
    let pid = view.read_with(cx, |app, _| match &app.rows[0] {
        Row::Process { info, .. } => info.pid,
        _ => unreachable!(),
    });
    let row: &'static str = Box::leak(format!("proc-row-{pid}").into_boxed_str());
    click(cx, row);
    assert!(view.read_with(cx, |app, _| !app.filter_focused));

    // The Columns button toggles its menu, and a click anywhere else closes it.
    click(cx, "columns-btn");
    assert!(view.read_with(cx, |app, _| app.column_menu));
    click(cx, "columns-btn");
    assert!(
        view.read_with(cx, |app, _| !app.column_menu),
        "the button closes it again"
    );
    click(cx, "columns-btn");
    click(cx, row);
    assert!(
        view.read_with(cx, |app, _| !app.column_menu),
        "a click outside closes it"
    );

    // At the minimum window size the toolbar stays inside the window.
    cx.simulate_resize(gpui::size(px(860.), px(560.)));
    cx.run_until_parked();
    for selector in ["columns-btn", "group-btn", "filter-field"] {
        let b = bounds(cx, selector);
        println!("{selector}: {:?}..{:?}", b.left(), b.right());
        assert!(b.right() <= px(860.), "{selector} overflows the window");
    }
    std::fs::remove_dir_all(&dir).ok();
}

/// Every listing table sorts by clicking a header (numbers by value, biggest first; a second click
/// reverses) and its filter searches every column.
#[gpui::test]
fn listing_tables_sort_and_filter(cx: &mut TestAppContext) {
    use crate::app::Page;
    use crate::sysops::InstalledApp;
    use std::path::PathBuf;
    use std::time::Duration;

    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-ui-tables-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();

    // Installed apps with sizes that sort differently as text ("9.0 MB" > "10.0 MB") and as numbers.
    const MB: u64 = 1024 * 1024;
    let names = |app: &OtmApp, _: &gpui::App| -> Vec<String> {
        app.listing
            .rows
            .iter()
            .map(|r| r.cells[0].to_string())
            .collect()
    };
    view.update(cx, |app, cx| {
        app.page = Page::Apps;
        app.apps = [
            ("Beta", "1.10", 9),
            ("alpha", "1.9", 10),
            ("Gamma", "2.0", 512),
        ]
        .into_iter()
        .map(|(name, version, mb)| InstalledApp {
            name: name.into(),
            version: version.into(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bytes: Some(mb * MB),
        })
        .collect();
        app.rebuild_listing();
        cx.notify();
    });

    click(cx, "sort-apps-2");
    let by_size = view.read_with(cx, names);
    println!("Size, first click: {by_size:?}");
    assert_eq!(
        by_size,
        ["Gamma", "alpha", "Beta"],
        "biggest first, by bytes not text"
    );
    click(cx, "sort-apps-2");
    assert_eq!(
        view.read_with(cx, names),
        ["Beta", "alpha", "Gamma"],
        "second click reverses"
    );

    click(cx, "sort-apps-0");
    assert_eq!(
        view.read_with(cx, names),
        ["alpha", "Beta", "Gamma"],
        "names A to Z, any case"
    );
    click(cx, "sort-apps-1");
    assert_eq!(
        view.read_with(cx, names),
        ["alpha", "Beta", "Gamma"],
        "versions 1.9 < 1.10 < 2.0"
    );

    // The filter searches every column: here the Location column.
    view.update(cx, |app, cx| {
        app.query = "gamma.app".into();
        app.rebuild_listing();
        cx.notify();
    });
    assert_eq!(view.read_with(cx, names), ["Gamma"]);

    // Users is a live table too: sorting by process count keeps the biggest account first.
    view.update(cx, |app, cx| {
        app.query.clear();
        app.open_page(Page::Users, cx);
        cx.notify();
    });
    click(cx, "sort-users-1");
    let counts: Vec<usize> = view.read_with(cx, |app, _| {
        app.listing
            .rows
            .iter()
            .map(|r| r.cells[1].parse().unwrap())
            .collect()
    });
    println!("users by process count: {counts:?}");
    assert!(counts.len() >= 2 && counts.windows(2).all(|w| w[0] >= w[1]));
    std::fs::remove_dir_all(&dir).ok();
}

/// The filter field edits at a caret, Space types, a chip turns back into text, a threshold with no
/// number shows a hint, and toolbar Force Quit does nothing without a selection.
#[gpui::test]
fn filter_caret_chip_edit_and_disabled_toolbar(cx: &mut TestAppContext) {
    use crate::filter::Chip;
    use std::time::Duration;

    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-ui-caret-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();

    // Typing at the caret: "abc", two steps left, "X" -> "aXbc"; Space types a space.
    click(cx, "filter-field");
    cx.simulate_input("abc");
    cx.simulate_keystrokes("left left");
    cx.simulate_input("X");
    assert_eq!(view.read_with(cx, |app, _| app.query.clone()), "aXbc");
    cx.simulate_keystrokes("end space");
    assert_eq!(view.read_with(cx, |app, _| app.query.clone()), "aXbc ");
    cx.simulate_keystrokes("backspace backspace");
    assert_eq!(view.read_with(cx, |app, _| app.query.clone()), "aXb");

    // A threshold with no number yet says what to type.
    view.update(cx, |app, cx| {
        app.clear_filter();
        cx.notify();
    });
    cx.simulate_input("cpu>abc");
    assert!(
        cx.debug_bounds("filter-hint").is_some(),
        "the hint is shown"
    );

    // Clicking a chip puts it back in the field as text, ready to change.
    view.update(cx, |app, cx| {
        app.clear_filter();
        app.add_chip(Chip::CpuAbove(5.));
        app.focus_filter();
        cx.notify();
    });
    click(cx, "filter-chip-0");
    let (chips, query) = view.read_with(cx, |app, _| (app.chips.clone(), app.query.clone()));
    assert!(chips.is_empty());
    assert_eq!(query, "cpu>5");

    // Without a selection the toolbar's Force Quit is inert (it stays, dimmed, so nothing shifts).
    view.update(cx, |app, cx| {
        app.clear_filter();
        app.blur_filter();
        app.selected = None;
        cx.notify();
    });
    click(cx, "force-quit");
    assert!(view.read_with(cx, |app, _| app.confirm.is_none()));
    std::fs::remove_dir_all(&dir).ok();
}
