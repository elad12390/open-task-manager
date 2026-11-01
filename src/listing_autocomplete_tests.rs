//! Autocomplete and column-value chips on the listing pages.

use gpui::TestAppContext;

use crate::{
    app::{OtmApp, Page},
    filter::{Chip, Suggestion, listing_matches, suggest_rows},
    ui_tests::{ENV_LOCK, click},
};

const TITLES: [&str; 3] = ["State", "Scope", "Label"];

/// Fifteen rows: two states, two scopes, and a label unique to each row.
fn sample() -> Vec<Vec<String>> {
    (0..15)
        .map(|i| {
            vec![
                if i % 2 == 0 { "Running" } else { "Not loaded" }.to_string(),
                if i < 3 { "User agent" } else { "Daemon" }.to_string(),
                format!("label-{i}"),
            ]
        })
        .collect()
}

fn column(title: &'static str, value: &str) -> Chip {
    Chip::Column {
        title,
        value: value.into(),
    }
}

#[test]
fn column_values_are_offered_with_their_row_counts() {
    let rows = sample();
    let s = suggest_rows("", &[], &TITLES, &rows);
    assert!(s.len() <= 8, "the panel is capped");
    assert!(s.contains(&Suggestion {
        chip: column("State", "Running"),
        count: 8,
    }));
    assert!(
        s.iter()
            .all(|s| !matches!(s.chip, Chip::Column { title: "Label", .. })),
        "near-unique columns are not offered until something matches them"
    );
}

#[test]
fn typed_text_reaches_near_unique_columns() {
    let rows = sample();
    let s = suggest_rows("label-1", &[], &TITLES, &rows);
    assert_eq!(s[0].chip, column("Label", "label-1"));
    assert_eq!(s[0].count, 1);
}

#[test]
fn a_column_prefix_restricts_the_suggestions() {
    let rows = sample();
    let s = suggest_rows("scope:da", &[], &TITLES, &rows);
    assert_eq!(
        s,
        vec![Suggestion {
            chip: column("Scope", "Daemon"),
            count: 12,
        }]
    );
}

#[test]
fn chosen_chips_are_not_offered_again() {
    let rows = sample();
    let chosen = [column("State", "Running")];
    let s = suggest_rows("", &chosen, &TITLES, &rows);
    assert!(s.iter().all(|s| s.chip != chosen[0]));
}

#[test]
fn prefix_matches_rank_above_word_matches() {
    let rows = vec![
        vec!["Not running".to_string(), "x".into(), "y".into()],
        vec!["Running".to_string(), "x".into(), "y".into()],
    ];
    let s = suggest_rows("run", &[], &TITLES, &rows);
    assert_eq!(s[0].chip, column("State", "Running"));
    assert_eq!(s[1].chip, column("State", "Not running"));
}

#[test]
fn blank_cells_are_not_offered() {
    let rows = vec![vec!["—".to_string(), "x".into(), "y".into()]];
    let s = suggest_rows("", &[], &TITLES, &rows);
    assert!(s.iter().all(|s| s.chip.value() != "—"));
}

#[test]
fn count_labels_say_rows_or_processes() {
    let one = Suggestion {
        chip: column("State", "Running"),
        count: 1,
    };
    assert_eq!(one.count_label(), "1 row");
    let many = Suggestion {
        chip: Chip::App("Chrome".into()),
        count: 2,
    };
    assert_eq!(many.count_label(), "2 processes");
}

#[test]
fn same_column_chips_widen_and_different_columns_narrow() {
    let row = ["Running", "Daemon", "label-1"];
    let state = |v: &str| column("State", v);
    assert!(listing_matches(
        &[state("Running"), state("Not loaded")],
        "",
        &TITLES,
        &row[..]
    ));
    assert!(!listing_matches(
        &[state("Not loaded")],
        "",
        &TITLES,
        &row[..]
    ));
    assert!(listing_matches(
        &[state("Running"), column("Scope", "Daemon")],
        "",
        &TITLES,
        &row[..]
    ));
    assert!(!listing_matches(
        &[state("Running"), column("Scope", "User agent")],
        "",
        &TITLES,
        &row[..]
    ));
}

#[test]
fn free_text_matches_any_column_alongside_chips() {
    let row = ["Running", "Daemon", "label-1"];
    let running = [column("State", "Running")];
    assert!(listing_matches(&running, "daemon", &TITLES, &row[..]));
    assert!(!listing_matches(&running, "absent", &TITLES, &row[..]));
    let stopped = [column("State", "Not loaded")];
    assert!(!listing_matches(&stopped, "daemon", &TITLES, &row[..]));
}

/// Typing in a listing's filter offers its column values, Enter adds one as a chip that filters
/// the rows, clicking the chip turns it back into text, and leaving the page drops it.
#[gpui::test]
fn listing_filter_autocompletes_column_values(cx: &mut TestAppContext) {
    use crate::sysops::InstalledApp;
    use std::{path::PathBuf, time::Duration};

    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-listing-autocomplete-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.executor().advance_clock(Duration::from_millis(1500));
    cx.run_until_parked();

    const MB: u64 = 1024 * 1024;
    view.update(cx, |app, cx| {
        app.page = Page::Apps;
        app.apps = [
            ("Beta", "1.10", 9),
            ("alpha", "1.9", 10),
            ("Gamma", "2.0", 512),
            ("Delta", "2.0", 1),
        ]
        .into_iter()
        .map(|(name, version, mb)| InstalledApp {
            name: name.into(),
            version: version.into(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
            bytes: Some(mb * MB),
            source: "Application",
        })
        .collect();
        app.rebuild_listing();
        cx.notify();
    });

    click(cx, "page-filter");
    cx.simulate_input("2.0");
    assert!(
        cx.debug_bounds("filter-suggestions").is_some(),
        "the suggestions panel opens while typing"
    );
    let top = view.read_with(cx, |app, _| app.suggestions()[0].clone());
    assert_eq!(top.chip, column("Version", "2.0"));
    assert_eq!(top.count, 2);

    cx.simulate_keystrokes("enter");
    assert_eq!(
        view.read_with(cx, |app, _| app.list_chips.clone()),
        vec![column("Version", "2.0")]
    );
    assert!(view.read_with(cx, |app, _| app.chips.is_empty()));
    let names = view.read_with(cx, |app, _| {
        app.listing
            .rows
            .iter()
            .map(|r| r.cells[0].to_string())
            .collect::<Vec<_>>()
    });
    assert_eq!(names, ["Gamma", "Delta"]);

    click(cx, "filter-chip-0");
    assert!(view.read_with(cx, |app, _| app.list_chips.is_empty()));
    assert_eq!(
        view.read_with(cx, |app, _| app.query.clone()),
        "version:2.0"
    );

    cx.simulate_keystrokes("escape escape");
    assert!(view.read_with(cx, |app, _| app.query.is_empty()));

    view.update(cx, |app, cx| {
        app.add_chip(column("Version", "1.9"));
        app.open_page(Page::Users, cx);
        cx.notify();
    });
    assert!(view.read_with(cx, |app, _| app.list_chips.is_empty()));
    std::fs::remove_dir_all(&dir).ok();
}
