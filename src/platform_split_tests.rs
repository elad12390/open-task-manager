//! The platform split: UI code never names an operating system, shortcuts use the platform's
//! modifier, and each platform classifies processes and draws glass the way its window can show.

use std::path::Path;

use gpui::WindowAppearance;

#[cfg(target_os = "linux")]
use crate::categories::{OS_OWNER, categorize};
#[cfg(target_os = "linux")]
const TOOLS: &str = "Command-line & Other";
#[cfg(target_os = "linux")]
use crate::metrics::ProcessInfo;
use crate::{
    categories::{APPS, SYSTEM_GROUP, TOPS},
    menus::key,
    platform::CAPS,
    settings::Settings,
    theme::Theme,
};

#[cfg(target_os = "linux")]
fn process(pid: u32, ppid: Option<u32>, name: &str, user: &str, path: &str) -> ProcessInfo {
    ProcessInfo {
        pid,
        ppid,
        name: name.into(),
        user: user.into(),
        path: path.into(),
        ..Default::default()
    }
}

#[test]
fn ui_code_names_no_operating_system() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&src).unwrap().flatten() {
        let path = entry.path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if path.extension().and_then(|x| x.to_str()) != Some("rs") || name.ends_with("_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains("target_os") {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "OS checks belong in src/platform/, not in {offenders:?}"
    );
}

#[test]
fn the_top_groups_start_with_the_system_group() {
    assert_eq!(TOPS.first(), Some(&SYSTEM_GROUP));
    assert!(TOPS.contains(&APPS));
    assert!(CAPS.groups.contains(&CAPS.system_group));
}

#[test]
fn shortcuts_use_the_platform_modifier() {
    let m = CAPS.mod_key;
    assert_eq!(key("cmd-q"), format!("{m}-q"));
    assert_eq!(key("alt-cmd-q"), format!("alt-{m}-q"));
    assert_eq!(key("shift-cmd-p"), format!("shift-{m}-p"));
    assert_eq!(key("cmd-="), format!("{m}-="));
}

#[test]
fn an_opaque_window_gets_a_solid_tint_under_the_glass() {
    let theme = Theme::resolve(WindowAppearance::Light, &Settings::default());
    assert!(
        CAPS.glass_blur || theme.window_tint.a == 1.0,
        "without a blurred window the tint must be opaque, not {}",
        theme.window_tint.a
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_splits_kernel_and_services_from_your_processes() {
    let procs = vec![
        process(2, Some(0), "kthreadd", "root", ""),
        process(1, Some(0), "systemd", "root", "/usr/lib/systemd/systemd"),
        process(4242, Some(1), "bash", "elad", "/usr/bin/bash"),
        process(4300, Some(4242), "sleep", "elad", "/usr/bin/sleep"),
    ];
    let cats = categorize(&procs);
    assert_eq!(cats[&2], (SYSTEM_GROUP, "Kernel threads".to_string()));
    assert_eq!(cats[&1], (SYSTEM_GROUP, "Services".to_string()));
    assert_eq!(cats[&4242], (TOOLS, "bash".to_string()));
    assert_eq!(cats[&4300], (TOOLS, "sleep".to_string()));
    let owners = crate::categories::owners(&procs);
    assert_eq!(owners[&1], OS_OWNER);
    assert_eq!(
        owners[&4300], "bash",
        "a tool is owned by the outermost tool that runs it"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn linux_reads_this_process_from_proc() {
    let me = std::process::id();
    let table = crate::platform::process_table();
    let mine = table.get(&me).expect("this process is in /proc");
    assert!(
        mine.memory > 0,
        "resident memory is read from /proc/<pid>/status"
    );
    assert!(
        mine.path.ends_with(
            std::env::current_exe()
                .unwrap()
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        ),
        "the executable path comes from /proc/<pid>/exe, got {:?}",
        mine.path
    );
    assert_eq!(
        crate::platform::process_state(me)
            .as_deref()
            .map(|s| s.len()),
        Some(1),
        "the state is one letter from /proc/<pid>/stat"
    );
}
