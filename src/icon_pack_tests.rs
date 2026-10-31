//! Icons and the logo: every icon the code names is embedded, no emoji or Unicode glyph icons are
//! left in the UI, and the sidebar shows the app logo beside the title.

use std::{fs, path::Path};

use gpui::TestAppContext;

use crate::{
    app::{OtmApp, Page},
    assets::ICONS,
    ui_tests::ENV_LOCK,
};

fn src_files() -> Vec<(String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<(String, String)> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "rs"))
        .map(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read_to_string(&p).unwrap())
        })
        .collect();
    files.sort();
    files
}

/// Names passed to `icon(`, `icon_text(` and `pill_icon(` as string literals.
fn literal_icon_names(src: &str) -> Vec<String> {
    let mut names = Vec::new();
    for marker in ["icon(", "icon_text("] {
        let mut rest = src;
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let arg = rest.trim_start();
            if let Some(body) = arg.strip_prefix('"')
                && let Some(end) = body.find('"')
            {
                names.push(body[..end].to_string());
            }
        }
    }
    names
}

fn embedded(name: &str) -> bool {
    ICONS
        .iter()
        .any(|(path, _)| *path == format!("icons/{name}.svg"))
}

#[test]
fn every_icon_name_in_code_is_embedded() {
    let mut used = Vec::new();
    for (file, src) in src_files() {
        if file.ends_with("_tests.rs") || file == "assets.rs" {
            continue;
        }
        used.extend(
            literal_icon_names(&src)
                .into_iter()
                .map(|n| (file.clone(), n)),
        );
    }
    // Names chosen by an if/else or a match rather than passed as a literal.
    for name in [
        "arrow-up",
        "arrow-down",
        "chevron-right",
        "chevron-down",
        "check",
        "triangle-alert",
        "circle",
        "square",
        "refresh-cw",
    ] {
        used.push(("(conditional)".into(), name.into()));
    }
    assert!(
        used.len() > 15,
        "the scan found too few icon names: {used:?}"
    );
    let missing: Vec<_> = used.iter().filter(|(_, n)| !embedded(n)).collect();
    assert!(missing.is_empty(), "icons not embedded: {missing:?}");
}

#[test]
fn every_sidebar_page_has_an_embedded_icon() {
    for (_, pages) in Page::SECTIONS {
        for page in pages.iter() {
            assert!(
                embedded(page.icon()),
                "{} has no embedded icon {:?}",
                page.label(),
                page.icon()
            );
        }
    }
}

#[test]
fn embedded_icons_match_the_files_on_disk() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    let mut on_disk: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".svg"))
        .map(|n| format!("icons/{n}"))
        .collect();
    on_disk.sort();
    let mut embedded_names: Vec<String> = ICONS.iter().map(|(p, _)| p.to_string()).collect();
    embedded_names.sort();
    assert_eq!(on_disk, embedded_names);
    assert!(dir.join("LICENSE-lucide.txt").exists());
}

#[test]
fn asset_source_serves_icons_and_logo() {
    use gpui::AssetSource;

    let svg = crate::assets::Assets
        .load("icons/search.svg")
        .unwrap()
        .expect("search icon is embedded");
    assert!(String::from_utf8_lossy(&svg).contains("<svg"));
    assert!(
        crate::assets::Assets
            .load("icons/nope.svg")
            .unwrap()
            .is_none()
    );
    let logo = crate::assets::logo();
    assert_eq!(logo.format(), gpui::ImageFormat::Png);
    assert!(logo.bytes().starts_with(b"\x89PNG"));
}

/// Glyphs the icon pack replaces. Native menu titles (menus.rs) keep their plain-text checkmarks,
/// and the shortcut and key-hint symbols (⌘, ⌥, ⏎, ⌫, ↑↓) stay as text.
const REPLACED: &str = "◎☰◉⏻⚙⇅▣◔⚡⏱●✦↻⚠✕⌕▾▸■✓";
const REPLACED_TEXT: &[&str] = &["\"×\"", "\"↓ {", "\"↑ {", " ↓\"", " ↑\""];

#[test]
fn no_replaced_glyphs_remain_in_ui_code() {
    let mut found = Vec::new();
    for (file, src) in src_files() {
        if file.ends_with("_tests.rs") || file == "menus.rs" {
            continue;
        }
        for (n, line) in src.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            let hit = code
                .chars()
                .find(|c| REPLACED.contains(*c))
                .map(|c| c.to_string());
            let hit = hit.or_else(|| {
                REPLACED_TEXT
                    .iter()
                    .find(|p| code.contains(*p))
                    .map(|p| p.to_string())
            });
            if let Some(hit) = hit {
                found.push(format!("{file}:{} {hit:?}: {}", n + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "glyph icons left in UI code:\n{}",
        found.join("\n")
    );
}

#[gpui::test]
fn sidebar_shows_the_logo_beside_the_title(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-icon-test-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (_view, cx) = cx.add_window_view(OtmApp::new);
    let logo = crate::ui_tests::bounds(cx, "app-logo");
    println!("logo {logo:?}");
    assert_eq!(logo.size.width, gpui::px(28.), "logo is 28 px square");
    assert_eq!(logo.size.height, gpui::px(28.), "logo is 28 px square");
}
