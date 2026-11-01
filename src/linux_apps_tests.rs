//! Linux apps and icons: desktop entries, icon theme lookup, app sources, uninstall commands and the
//! process-to-app matching. The parsing and lookup code only exists on Linux, so the file does too.

#![cfg(target_os = "linux")]

use std::{
    fs,
    path::{Path, PathBuf},
};

use gpui::TestAppContext;

use crate::{
    app::{OtmApp, Page},
    platform::linux::{
        apps::{
            Desktop, ExeIndex, Location, Owner, Removal, Source, exec_program, locale_names,
            location_of, parse_desktop, parse_dpkg_s, parse_flatpak_list, parse_package_info,
            parse_rpm_qf, parse_snap_list, removal_argv, removal_for, scan_desktop, trash_app,
        },
        icons::{Theme, exe_icon_name, folder_icon_name, system_icon, write_png},
    },
    sysops::InstalledApp,
    ui_tests::ENV_LOCK,
};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("otm-apps-{name}-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn put(root: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

fn use_data_dir(name: &str) {
    // SAFETY: tests that set this hold ENV_LOCK, so no other test reads it meanwhile.
    unsafe { std::env::set_var("OTM_DATA_DIR", temp_dir(name)) };
}

const EDITOR: &str = "[Desktop Entry]\nType=Application\nName=Editor\nName[de]=Redaktor\n\
    Icon=editor-icon\nExec=env GTK_USE_PORTAL=1 /usr/bin/editor %U\n";

#[test]
fn desktop_entry_gives_the_localized_name_icon_and_exec() {
    let path = Path::new("/usr/share/applications/editor.desktop");
    let de = parse_desktop(path, EDITOR, &locale_names("de_DE.UTF-8")).unwrap();
    assert_eq!(de.name, "Redaktor");
    assert_eq!(de.icon.as_deref(), Some("editor-icon"));
    assert_eq!(
        de.exec.as_deref(),
        Some("env GTK_USE_PORTAL=1 /usr/bin/editor %U")
    );
    assert!(!de.no_display);
    let en = parse_desktop(path, EDITOR, &[]).unwrap();
    assert_eq!(en.name, "Editor");
}

#[test]
fn hidden_and_non_application_entries_are_dropped_but_no_display_stays() {
    let path = Path::new("/x/a.desktop");
    let hidden = "[Desktop Entry]\nType=Application\nName=A\nHidden=true\n";
    assert_eq!(parse_desktop(path, hidden, &[]), None);
    let link = "[Desktop Entry]\nType=Link\nName=B\n";
    assert_eq!(parse_desktop(path, link, &[]), None);
    let other_group = "[Desktop Action New]\nName=Only an action\n";
    assert_eq!(parse_desktop(path, other_group, &[]), None);
    let helper = "[Desktop Entry]\nType=Application\nName=Helper\nNoDisplay=true\n";
    let helper = parse_desktop(path, helper, &[]).unwrap();
    assert!(helper.no_display);
}

#[test]
fn locale_names_come_most_specific_first() {
    assert_eq!(locale_names("de_DE.UTF-8"), ["de_DE", "de"]);
    assert_eq!(locale_names("fr@euro"), ["fr"]);
    assert!(locale_names("C").is_empty());
    assert!(locale_names("").is_empty());
}

#[test]
fn a_user_desktop_file_shadows_the_system_one_even_when_hidden() {
    let root = temp_dir("shadow");
    let user = root.join("user");
    let system = root.join("system");
    put(
        &user,
        "x.desktop",
        "[Desktop Entry]\nType=Application\nName=X\nHidden=true\n",
    );
    put(
        &system,
        "x.desktop",
        "[Desktop Entry]\nType=Application\nName=X system\n",
    );
    put(
        &system,
        "y.desktop",
        "[Desktop Entry]\nType=Application\nName=Y\n",
    );
    let found = scan_desktop(&[user, system], &[]);
    let names: Vec<&str> = found.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["Y"]);
}

/// A theme with inheritance (`Parent`), size choice, an SVG beside a PNG, and a pixmap fallback.
fn fixture_roots(name: &str) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let root = temp_dir(name);
    let icons = root.join("icons");
    put(
        &icons,
        "Fixture/index.theme",
        "[Icon Theme]\nName=Fixture\nInherits=Parent\n\
         Directories=24x24/apps,48x48/apps,scalable/apps\n\
         [24x24/apps]\nSize=24\nType=Threshold\n\
         [48x48/apps]\nSize=48\nType=Threshold\n\
         [scalable/apps]\nSize=48\nType=Scalable\n",
    );
    put(&icons, "Fixture/24x24/apps/size.png", "24");
    put(&icons, "Fixture/24x24/apps/small.png", "small");
    put(&icons, "Fixture/48x48/apps/size.png", "48");
    put(&icons, "Fixture/48x48/apps/both.png", "png");
    put(&icons, "Fixture/scalable/apps/both.svg", "<svg/>");
    put(
        &icons,
        "Parent/index.theme",
        "[Icon Theme]\nName=Parent\nDirectories=16x16/apps\n[16x16/apps]\nSize=16\nType=Threshold\n",
    );
    put(&icons, "Parent/16x16/apps/inherited.png", "parent");
    let pixmaps = root.join("pixmaps");
    put(&pixmaps, "legacy.xpm", "xpm");
    put(&pixmaps, "legacy.png", "legacy");
    (vec![icons], vec![pixmaps])
}

#[test]
fn theme_lookup_inherits_from_the_parent_theme() {
    let (roots, pixmaps) = fixture_roots("inherit");
    let theme = Theme::new(&roots, &pixmaps, "Fixture");
    assert_eq!(
        theme.lookup("inherited"),
        Some(roots[0].join("Parent/16x16/apps/inherited.png"))
    );
}

#[test]
fn theme_lookup_picks_the_smallest_size_at_least_32() {
    let (roots, pixmaps) = fixture_roots("sizes");
    let theme = Theme::new(&roots, &pixmaps, "Fixture");
    assert_eq!(
        theme.lookup("size"),
        Some(roots[0].join("Fixture/48x48/apps/size.png"))
    );
    assert_eq!(
        theme.lookup("small"),
        Some(roots[0].join("Fixture/24x24/apps/small.png"))
    );
}

#[test]
fn theme_lookup_prefers_svg_over_png_and_strips_extensions() {
    let (roots, pixmaps) = fixture_roots("svgpng");
    let theme = Theme::new(&roots, &pixmaps, "Fixture");
    assert_eq!(
        theme.lookup("both"),
        Some(roots[0].join("Fixture/scalable/apps/both.svg"))
    );
    assert_eq!(theme.lookup("size.png"), theme.lookup("size"));
}

#[test]
fn theme_lookup_falls_back_to_pixmaps_and_absolute_paths() {
    let (roots, pixmaps) = fixture_roots("pixmaps");
    let theme = Theme::new(&roots, &pixmaps, "Fixture");
    assert_eq!(theme.lookup("legacy"), Some(pixmaps[0].join("legacy.png")));
    assert_eq!(theme.lookup("missing-icon"), None);
    let absolute = pixmaps[0].join("legacy.png");
    assert_eq!(
        theme.lookup(absolute.to_str().unwrap()),
        Some(absolute.clone())
    );
}

#[test]
fn a_theme_without_index_is_scanned_by_size_folder() {
    let root = temp_dir("scan");
    put(&root, "Scanned/48x48/apps/tool.png", "tool");
    put(&root, "Scanned/scalable/apps/vector.svg", "<svg/>");
    let theme = Theme::new(std::slice::from_ref(&root), &[], "Scanned");
    assert_eq!(
        theme.lookup("tool"),
        Some(root.join("Scanned/48x48/apps/tool.png"))
    );
    assert_eq!(
        theme.lookup("vector"),
        Some(root.join("Scanned/scalable/apps/vector.svg"))
    );
}

#[test]
fn svg_icons_are_rasterized_to_32_pixels_and_pngs_copied() {
    let root = temp_dir("raster");
    let svg = put(
        &root,
        "icon.svg",
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="48" height="24"><rect width="48" height="24" fill="red"/></svg>"#,
    );
    let out = root.join("out-svg.png");
    assert!(write_png(&svg, &out));
    let png = fs::read(&out).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 32);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 32);

    let png_src = root.join("src.png");
    fs::write(&png_src, b"\x89PNG\r\n\x1a\nbody").unwrap();
    let copied = root.join("out-png.png");
    assert!(write_png(&png_src, &copied));
    assert_eq!(fs::read(&copied).unwrap(), fs::read(&png_src).unwrap());

    assert!(!write_png(&root.join("icon.xpm"), &root.join("nope.png")));
}

#[test]
fn folders_terminals_and_system_processes_get_the_generic_icons() {
    let home = Path::new("/home/elad");
    assert_eq!(folder_icon_name(home, home), "user-home");
    assert_eq!(
        folder_icon_name(&home.join("Downloads"), home),
        "folder-download"
    );
    assert_eq!(folder_icon_name(&home.join("Projects"), home), "folder");
    assert_eq!(exe_icon_name(Path::new("/bin/bash")), "utilities-terminal");
    assert_eq!(
        exe_icon_name(Path::new("/usr/lib/xorg/Xorg")),
        "applications-system"
    );
    assert_eq!(
        exe_icon_name(Path::new("/usr/bin/vim")),
        "application-x-executable"
    );
    assert_eq!(
        system_icon("kworker/0:1").as_deref(),
        Some("icon-name:applications-system")
    );
    assert_eq!(system_icon("firefox"), None);
}

#[test]
fn exec_lines_name_their_program_past_env_and_variables() {
    assert_eq!(
        exec_program("env GTK_USE_PORTAL=1 /usr/bin/editor %U"),
        Some("/usr/bin/editor")
    );
    assert_eq!(exec_program("firefox %u"), Some("firefox"));
}

#[test]
fn a_process_matches_its_app_by_path_or_by_program_name() {
    let entry = |path: &str, exec: &str| Desktop {
        path: PathBuf::from(path),
        name: path.into(),
        icon: None,
        exec: Some(exec.into()),
        no_display: false,
    };
    let index = ExeIndex::new(vec![
        entry(
            "/usr/share/applications/editor.desktop",
            "env X=1 /opt/editor/bin/editor %U",
        ),
        entry("/usr/share/applications/sh-app.desktop", "bash -c launch"),
        entry("/usr/share/applications/browser.desktop", "browser %u"),
    ]);
    let app = |exe: &str| {
        index
            .desktop_for_exe(Path::new(exe))
            .map(|d| d.name.clone())
    };
    assert_eq!(
        app("/opt/editor/bin/editor").as_deref(),
        Some("/usr/share/applications/editor.desktop")
    );
    assert_eq!(
        app("/usr/lib/other/browser").as_deref(),
        Some("/usr/share/applications/browser.desktop")
    );
    assert_eq!(app("/usr/bin/bash"), None, "generic programs name no app");
    assert_eq!(app("/usr/bin/unknown"), None);
}

#[test]
fn install_location_comes_from_the_path() {
    let home = Path::new("/home/elad");
    assert_eq!(
        location_of(
            Path::new("/var/lib/flatpak/exports/share/applications/a.desktop"),
            home
        ),
        Location::Flatpak { user: false }
    );
    assert_eq!(
        location_of(
            Path::new("/home/elad/.local/share/flatpak/exports/share/applications/a.desktop"),
            home
        ),
        Location::Flatpak { user: true }
    );
    assert_eq!(
        location_of(
            Path::new("/var/lib/snapd/desktop/applications/firefox_firefox.desktop"),
            home
        ),
        Location::Snap
    );
    assert_eq!(
        location_of(
            Path::new("/home/elad/.local/share/applications/mine.desktop"),
            home
        ),
        Location::Home
    );
    assert_eq!(
        location_of(Path::new("/usr/share/applications/x.desktop"), home),
        Location::System
    );
    assert_eq!(
        location_of(
            Path::new("/run/host/usr/share/applications/x.desktop"),
            home
        ),
        Location::System
    );
    assert_eq!(Source::Flatpak.label(), "Flatpak");
    assert_eq!(Source::Deb.label(), "deb");
}

#[test]
fn package_manager_output_is_read_per_file_and_per_package() {
    let owned = parse_dpkg_s(
        "firefox-esr:amd64: /usr/share/applications/firefox-esr.desktop\n\
         libc6:amd64, libc6-dev:amd64: /usr/include/x.h\n\
         dpkg-query: no path found matching pattern /usr/share/applications/gone.desktop\n",
    );
    assert_eq!(
        owned.get(Path::new("/usr/share/applications/firefox-esr.desktop")),
        Some(&Owner::Deb("firefox-esr".into()))
    );
    assert_eq!(
        owned.get(Path::new("/usr/include/x.h")),
        Some(&Owner::Deb("libc6".into()))
    );
    assert_eq!(owned.len(), 2);

    let a = Path::new("/usr/share/applications/a.desktop");
    let b = Path::new("/usr/share/applications/b.desktop");
    let rpm = parse_rpm_qf(
        &[a, b],
        "libreoffice-core\nfile /usr/share/applications/b.desktop is not owned by any package\n",
    );
    assert_eq!(rpm.get(a), Some(&Owner::Rpm("libreoffice-core".into())));
    assert_eq!(rpm.get(b), None);
    assert!(parse_rpm_qf(&[a, b], "only one line\n").is_empty());

    let info = parse_package_info("firefox\t120.0\t2048\n", 1024);
    assert_eq!(info["firefox"].version, "120.0");
    assert_eq!(info["firefox"].bytes, Some(2048 * 1024));

    let flatpak = parse_flatpak_list("org.gnome.Calculator\t46.1\n");
    assert_eq!(flatpak["org.gnome.Calculator"], "46.1");
    let snaps = parse_snap_list("Name      Version  Rev\nfirefox   121.0    3000\n");
    assert_eq!(snaps["firefox"], "121.0");
    assert!(!snaps.contains_key("Name"));
}

#[test]
fn uninstall_runs_the_package_manager_that_owns_the_app() {
    let home = Path::new("/home/elad");
    let flatpak = Path::new("/var/lib/flatpak/exports/share/applications/org.x.Y.desktop");
    let removal = removal_for(flatpak, home, None);
    assert_eq!(
        removal,
        Removal::Flatpak {
            id: "org.x.Y".into(),
            user: false
        }
    );
    assert_eq!(
        removal_argv(&removal, flatpak),
        ["flatpak", "uninstall", "-y", "--system", "org.x.Y"]
    );

    let user_flatpak =
        Path::new("/home/elad/.local/share/flatpak/exports/share/applications/org.a.B.desktop");
    let removal = removal_for(user_flatpak, home, None);
    assert_eq!(removal_argv(&removal, user_flatpak)[3], "--user");

    let snap = Path::new("/var/lib/snapd/desktop/applications/firefox_firefox.desktop");
    assert_eq!(
        removal_argv(&removal_for(snap, home, None), snap),
        ["pkexec", "snap", "remove", "firefox"]
    );

    let deb = Path::new("/usr/share/applications/firefox-esr.desktop");
    let owner = Owner::Deb("firefox-esr".into());
    assert_eq!(
        removal_argv(&removal_for(deb, home, Some(&owner)), deb),
        ["pkexec", "apt-get", "remove", "-y", "firefox-esr"]
    );

    let rpm_owner = Owner::Rpm("libreoffice-core".into());
    let rpm = Path::new("/usr/share/applications/libreoffice-base.desktop");
    assert_eq!(
        removal_argv(&removal_for(rpm, home, Some(&rpm_owner)), rpm),
        ["pkexec", "dnf", "remove", "-y", "libreoffice-core"]
    );

    let manual = Path::new("/home/elad/.local/share/applications/mine.desktop");
    assert_eq!(
        removal_argv(&removal_for(manual, home, None), manual),
        [
            "gio",
            "trash",
            "/home/elad/.local/share/applications/mine.desktop"
        ]
    );
    let unowned = Path::new("/usr/share/applications/handmade.desktop");
    assert_eq!(removal_for(unowned, home, None), Removal::Trash);
}

#[test]
fn trash_refuses_anything_that_is_not_a_listed_desktop_file() {
    assert!(trash_app(Path::new("/usr/bin/ls")).is_err());
    assert!(trash_app(Path::new("/tmp/elsewhere/app.desktop")).is_err());
    assert!(trash_app(Path::new("/usr/share/applications/../../etc/x.desktop")).is_err());
}

#[gpui::test]
fn the_apps_list_shows_each_apps_source_on_linux(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use_data_dir("apps-source");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update(cx, |app, _| {
        app.apps = vec![InstalledApp {
            name: "Calculator".into(),
            version: "46.1".into(),
            path: "/var/lib/flatpak/exports/share/applications/org.gnome.Calculator.desktop".into(),
            bytes: Some(1024),
            source: "Flatpak",
        }];
        app.page = Page::Apps;
        app.rebuild_listing();
    });
    let (titles, cells) = view.read_with(cx, |app, _| {
        (
            app.listing
                .headers
                .iter()
                .map(|h| h.title)
                .collect::<Vec<_>>(),
            app.listing.cells.clone(),
        )
    });
    assert_eq!(titles.last(), Some(&"Source"));
    assert_eq!(
        cells[0].last().map(|s| s.to_string()),
        Some("Flatpak".to_string())
    );
}
