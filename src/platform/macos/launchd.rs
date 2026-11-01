//! launchd: the services list, the user's launch agents, and restart/stop through `launchctl`.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::sysops::{Service, ServiceAction, StartupItem, run};

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

fn valid_label(label: &str) -> Result<(), String> {
    if !label.is_empty()
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        Ok(())
    } else {
        Err("Invalid service name".into())
    }
}

pub(super) fn user_agents_dir() -> PathBuf {
    crate::sysops::home().join("Library/LaunchAgents")
}

fn launch_dirs() -> [(PathBuf, &'static str); 3] {
    [
        (user_agents_dir(), "User agent"),
        (PathBuf::from("/Library/LaunchAgents"), "System agent"),
        (PathBuf::from("/Library/LaunchDaemons"), "System daemon"),
    ]
}

fn launchctl_services() -> Vec<Service> {
    let out = run("launchctl", &["list"]).unwrap_or_default();
    out.lines()
        .skip(1)
        .filter_map(|l| {
            let mut f = l.split('\t');
            let pid = f.next()?;
            let status = f.next()?;
            let label = f.next()?.trim().to_string();
            Some(Service {
                label,
                pid: pid.parse().ok(),
                status: status.parse().unwrap_or(0),
                program: None,
                scope: "",
            })
        })
        .collect()
}

fn plist_paths(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("plist"))
        .collect()
}

/// The program a launch agent or daemon runs: its `Program`, else the first of its `ProgramArguments`.
pub fn plist_program(file: &Path) -> Option<PathBuf> {
    let json = run(
        "plutil",
        &["-convert", "json", "-o", "-", &file.to_string_lossy()],
    )
    .ok()?;
    let plist: serde_json::Value = serde_json::from_str(&json).ok()?;
    let program = plist
        .get("Program")
        .and_then(|p| p.as_str())
        .map(str::to_string)
        .or_else(|| {
            plist
                .get("ProgramArguments")?
                .get(0)?
                .as_str()
                .map(str::to_string)
        })?;
    program.starts_with('/').then(|| PathBuf::from(program))
}

pub fn services() -> Vec<Service> {
    let mut list = launchctl_services();
    let plists: HashMap<String, PathBuf> = launch_dirs()
        .iter()
        .flat_map(|(dir, _)| plist_paths(dir))
        .map(|file| {
            let label = file
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            (label, file)
        })
        .collect();
    for s in list.iter_mut().filter(|s| s.pid.is_none()) {
        s.program = plists.get(&s.label).and_then(|file| plist_program(file));
    }
    list
}

pub fn startup_items() -> Vec<StartupItem> {
    let loaded: HashSet<String> = launchctl_services().into_iter().map(|s| s.label).collect();
    let mut items = Vec::new();
    for (dir, scope) in launch_dirs() {
        for path in plist_paths(&dir) {
            let label = path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            items.push(StartupItem {
                loaded: loaded.contains(&label),
                editable: scope == "User agent",
                label,
                scope,
                program: plist_program(&path),
                file: path,
            });
        }
    }
    items.sort_by(|a, b| {
        a.scope
            .cmp(b.scope)
            .then(a.label.to_lowercase().cmp(&b.label.to_lowercase()))
    });
    items
}

/// Load or unload one of the user's own launch agents. System agents are refused.
pub fn toggle_startup(file: &Path, enable: bool) -> Result<(), String> {
    let dir = user_agents_dir();
    let ok = file.parent() == Some(dir.as_path())
        && file.extension().and_then(|x| x.to_str()) == Some("plist")
        && !file.to_string_lossy().contains("..");
    if !ok {
        return Err("Only your own startup items can be changed".into());
    }
    if !file.exists() {
        return Err("Startup item not found".into());
    }
    let path = file.to_string_lossy();
    run(
        "launchctl",
        &[if enable { "load" } else { "unload" }, "-w", &path],
    )
    .map(|_| ())
}

pub fn control_service(label: &str, action: ServiceAction) -> Result<(), String> {
    valid_label(label)?;
    let uid = uid();
    let target = format!("gui/{uid}/{label}");
    let plist = user_agents_dir().join(format!("{label}.plist"));
    match action {
        ServiceAction::Restart if plist.exists() => {
            let _ = run("launchctl", &["bootout", &target]);
            run(
                "launchctl",
                &["bootstrap", &format!("gui/{uid}"), &plist.to_string_lossy()],
            )?;
            run("launchctl", &["kickstart", &target]).map(|_| ())
        }
        ServiceAction::Restart => run("launchctl", &["kickstart", "-k", &target]).map(|_| ()),
        ServiceAction::Stop => run("launchctl", &["kill", "TERM", &target]).map(|_| ()),
    }
}
