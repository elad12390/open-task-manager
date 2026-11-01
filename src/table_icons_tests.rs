//! Icons in the process and listing tables: icon keys, rendering, and which icon each row carries.

#[cfg(target_os = "macos")]
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::path::Path;
use std::path::PathBuf;

use gpui::TestAppContext;

#[cfg(target_os = "macos")]
use crate::icons;
use crate::{
    app::{OtmApp, Page},
    categories::OS_OWNER,
    metrics::ProcessInfo,
    sysops::{Connection, InstalledApp},
    ui_tests::ENV_LOCK,
};

#[cfg(target_os = "macos")]
use crate::platform::plist_program;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("otm-{name}-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn use_data_dir(name: &str) {
    // SAFETY: tests that set this hold ENV_LOCK, so no other test reads it meanwhile.
    unsafe { std::env::set_var("OTM_DATA_DIR", temp_dir(name)) };
}

fn process(pid: u32, name: &str, path: &str, owner: &str) -> ProcessInfo {
    ProcessInfo {
        pid,
        name: name.into(),
        path: path.into(),
        owner_app: owner.into(),
        user: "elad".into(),
        ..Default::default()
    }
}

#[cfg(target_os = "macos")]
#[test]
fn bundleless_executable_renders_a_png_icon() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use_data_dir("exec-icon");
    let key = icons::process_key("ls", "/bin/ls", OS_OWNER, &HashMap::new()).unwrap();
    assert_eq!(key, "/bin/ls");
    let found = icons::render(std::slice::from_ref(&key));
    let png = std::fs::read(found.get(&key).expect("an icon was rendered")).unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 32);
    assert_eq!(u32::from_be_bytes(png[20..24].try_into().unwrap()), 32);
}

#[cfg(target_os = "macos")]
#[test]
fn process_icon_prefers_own_bundle_then_owner_then_system_then_executable() {
    let owners = HashMap::from([("Safari".to_string(), "/Applications/Safari.app".to_string())]);
    let key = |name, path, owner| icons::process_key(name, path, owner, &owners);
    assert_eq!(
        key(
            "Safari Helper",
            "/Applications/Safari.app/Contents/MacOS/Safari",
            "Safari"
        ),
        Some("/Applications/Safari.app".into())
    );
    assert_eq!(
        key("helper", "/usr/local/bin/helper", "Safari"),
        Some("/Applications/Safari.app".into())
    );
    assert_eq!(
        key("helper", "/usr/local/bin/helper", OS_OWNER),
        Some("/usr/local/bin/helper".into())
    );
    let gear = key("kernel_task", "", OS_OWNER).expect("kernel_task has a system icon");
    assert!(Path::new(&gear).exists(), "{gear} exists");
    assert_eq!(key("ghost", "", "Nobody"), None);
}

#[cfg(target_os = "macos")]
#[test]
fn launch_agent_program_comes_from_the_plist() {
    let dir = temp_dir("plist");
    let plist = |body: &str| {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>{body}</dict></plist>"#
        )
    };
    let args = dir.join("args.plist");
    std::fs::write(
        &args,
        plist("<key>ProgramArguments</key><array><string>/Applications/Foo.app/Contents/MacOS/foo</string><string>--quiet</string></array>"),
    )
    .unwrap();
    assert_eq!(
        plist_program(&args),
        Some(PathBuf::from("/Applications/Foo.app/Contents/MacOS/foo"))
    );
    let both = dir.join("both.plist");
    std::fs::write(
        &both,
        plist("<key>Program</key><string>/usr/bin/true</string><key>ProgramArguments</key><array><string>/bin/false</string></array>"),
    )
    .unwrap();
    assert_eq!(plist_program(&both), Some(PathBuf::from("/usr/bin/true")));
}

#[gpui::test]
fn apps_rows_carry_their_bundle_icon(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use_data_dir("apps-icons");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update(cx, |app, _| {
        app.apps = vec![InstalledApp {
            name: "Safari".into(),
            version: "18".into(),
            path: "/Applications/Safari.app".into(),
            bytes: None,
            source: "Application",
        }];
        app.page = Page::Apps;
        app.rebuild_listing();
    });
    let (icons_flag, rows) = view.read_with(cx, |app, _| {
        (
            app.listing.icons,
            app.listing
                .rows
                .iter()
                .map(|r| r.icon.clone())
                .collect::<Vec<_>>(),
        )
    });
    assert!(icons_flag);
    assert_eq!(rows, vec![Some("/Applications/Safari.app".to_string())]);
}

#[gpui::test]
fn connection_rows_use_their_process_icon(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use_data_dir("connection-icons");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update(cx, |app, _| {
        app.snapshot.processes = vec![process(42, "tool", "/usr/local/bin/tool", OS_OWNER)];
        app.connections = vec![Connection {
            pid: 42,
            process: "tool".into(),
            proto: "TCP".into(),
            endpoint: "*:443".into(),
            state: "LISTEN".into(),
        }];
        app.page = Page::Connections;
        app.rebuild_listing();
    });
    let icon = view.read_with(cx, |app, _| app.listing.rows[0].icon.clone());
    assert_eq!(icon, Some("/usr/local/bin/tool".to_string()));
}

#[gpui::test]
#[cfg(target_os = "macos")]
fn user_rows_get_the_generic_user_icon(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    use_data_dir("user-icons");
    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update(cx, |app, _| {
        app.snapshot.processes = vec![process(7, "tool", "/usr/local/bin/tool", OS_OWNER)];
        app.page = Page::Users;
        app.rebuild_listing();
    });
    let icon = view.read_with(cx, |app, _| app.listing.rows[0].icon.clone());
    assert_eq!(icon.as_deref(), Some(icons::USER_ICON));
    assert!(Path::new(icons::USER_ICON).exists());
}
