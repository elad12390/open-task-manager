//! Installed apps from their desktop entries: Flatpak, Snap, the system's packages and the user's own
//! launchers. An app's source comes from where its desktop file lives, the packages that own the
//! system's desktop files are asked in one batch, and uninstalling runs the matching tool.

use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    path::{Path, PathBuf},
    sync::OnceLock,
};

use super::host;
use crate::sysops::{InstalledApp, home};

/// A desktop entry that is not hidden. `no_display` entries stay listed for matching processes.
#[derive(Clone, Debug, PartialEq)]
pub struct Desktop {
    pub path: PathBuf,
    pub name: String,
    pub icon: Option<String>,
    pub exec: Option<String>,
    pub no_display: bool,
}

/// Read the `[Desktop Entry]` group of a desktop file. `locales` lists the languages to prefer for
/// `Name[…]`, most specific first. Hidden entries and entries that are not applications give None.
pub fn parse_desktop(path: &Path, text: &str, locales: &[String]) -> Option<Desktop> {
    let (mut in_entry, mut kind, mut name, mut icon, mut exec) = (false, None, None, None, None);
    let (mut no_display, mut hidden) = (false, false);
    let mut localized: HashMap<&str, &str> = HashMap::new();
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Type" => kind = Some(value),
            "Name" => name = Some(value),
            "Icon" => icon = Some(value),
            "Exec" => exec = Some(value),
            "NoDisplay" => no_display = value == "true",
            "Hidden" => hidden = value == "true",
            key => {
                if let Some(locale) = key.strip_prefix("Name[").and_then(|k| k.strip_suffix(']')) {
                    localized.insert(locale, value);
                }
            }
        }
    }
    if hidden || kind != Some("Application") {
        return None;
    }
    let name = locales
        .iter()
        .find_map(|l| localized.get(l.as_str()).copied())
        .or(name)?;
    Some(Desktop {
        path: path.to_path_buf(),
        name: name.to_string(),
        icon: icon.filter(|s| !s.is_empty()).map(str::to_string),
        exec: exec.map(str::to_string),
        no_display,
    })
}

/// Language names for `Name[…]` from `LC_MESSAGES` or `LANG`.
pub fn locales() -> Vec<String> {
    let lang = ["LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    locale_names(&lang)
}

/// `de_DE.UTF-8` → `de_DE`, `de`. The C and POSIX locales name no language.
pub fn locale_names(lang: &str) -> Vec<String> {
    let base = lang.split(['.', '@']).next().unwrap_or("");
    if base.is_empty() || base == "C" || base == "POSIX" {
        return Vec::new();
    }
    let mut names = vec![base.to_string()];
    if let Some((language, _)) = base.split_once('_') {
        names.push(language.to_string());
    }
    names
}

/// Where desktop entries live, the first file per desktop ID winning: the user's own, then the
/// host's copies when running inside a Flatpak sandbox, then the system's and the sandbox's exports.
pub fn desktop_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
        PathBuf::from("/run/host/usr/local/share/applications"),
        PathBuf::from("/run/host/usr/share/applications"),
        PathBuf::from("/run/host/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/run/host/var/lib/snapd/desktop/applications"),
        PathBuf::from("/usr/local/share/applications"),
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        PathBuf::from("/var/lib/snapd/desktop/applications"),
    ]
}

/// The desktop entries in `dirs`. A file shadows later files with the same name, even when it is
/// hidden, as the desktop entry spec says.
pub fn scan_desktop(dirs: &[PathBuf], locales: &[String]) -> Vec<Desktop> {
    let mut seen: HashSet<OsString> = HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = host::read_dir(dir) else {
            continue;
        };
        let mut files: Vec<PathBuf> = entries
            .into_iter()
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("desktop"))
            .collect();
        files.sort();
        for path in files {
            let Some(id) = path.file_name().map(OsString::from) else {
                continue;
            };
            if !seen.insert(id) {
                continue;
            }
            let Ok(text) = host::read(&path) else {
                continue;
            };
            if let Some(desktop) = parse_desktop(&path, &text, locales) {
                out.push(desktop);
            }
        }
    }
    out
}

/// Where a desktop file was installed from, from its path alone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    Flatpak { user: bool },
    Snap,
    Home,
    System,
}

pub fn location_of(path: &Path, home: &Path) -> Location {
    let path = match path.strip_prefix("/run/host") {
        Ok(rest) => Path::new("/").join(rest),
        Err(_) => path.to_path_buf(),
    };
    if path.starts_with(home.join(".local/share/flatpak")) {
        Location::Flatpak { user: true }
    } else if path.starts_with("/var/lib/flatpak") {
        Location::Flatpak { user: false }
    } else if path.starts_with("/var/lib/snapd") {
        Location::Snap
    } else if !home.as_os_str().is_empty() && path.starts_with(home) {
        Location::Home
    } else {
        Location::System
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Flatpak,
    Snap,
    Deb,
    Rpm,
    Manual,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Flatpak => "Flatpak",
            Source::Snap => "Snap",
            Source::Deb => "deb",
            Source::Rpm => "rpm",
            Source::Manual => "Manual",
        }
    }
}

/// The package that installed a system desktop file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    Deb(String),
    Rpm(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Manager {
    Dpkg,
    Rpm,
    Unknown,
}

fn manager() -> Manager {
    if host::exists("/var/lib/dpkg/status") {
        Manager::Dpkg
    } else if host::exists("/var/lib/rpm") {
        Manager::Rpm
    } else {
        Manager::Unknown
    }
}

/// The stdout of a host command, whatever its exit status (`dpkg -S` exits 1 when one file is unknown).
fn output(cmd: &str, args: &[&str]) -> String {
    let argv: Vec<&str> = std::iter::once(cmd).chain(args.iter().copied()).collect();
    host::stdout(&argv)
}

/// The package owning each path, asked in one batch.
pub fn owners_of(paths: &[&Path]) -> HashMap<PathBuf, Owner> {
    if paths.is_empty() {
        return HashMap::new();
    }
    let files: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    match manager() {
        Manager::Dpkg => {
            let mut args = vec!["-S"];
            args.extend(files.iter().map(String::as_str));
            parse_dpkg_s(&output("dpkg", &args))
        }
        Manager::Rpm => {
            let mut args = vec!["-qf", "--queryformat", "%{NAME}\\n"];
            args.extend(files.iter().map(String::as_str));
            parse_rpm_qf(paths, &output("rpm", &args))
        }
        Manager::Unknown => HashMap::new(),
    }
}

/// `dpkg -S` lines look like `firefox-esr:amd64: /usr/share/applications/firefox-esr.desktop`.
pub fn parse_dpkg_s(out: &str) -> HashMap<PathBuf, Owner> {
    out.lines()
        .filter_map(|line| {
            let (packages, path) = line.split_once(": /")?;
            let package = packages.split(',').next()?.trim().split(':').next()?;
            Some((
                PathBuf::from(format!("/{path}")),
                Owner::Deb(package.to_string()),
            ))
        })
        .collect()
}

/// `rpm -qf` prints one line per file, in order: the package, or a "not owned" message.
pub fn parse_rpm_qf(paths: &[&Path], out: &str) -> HashMap<PathBuf, Owner> {
    let lines: Vec<&str> = out.lines().collect();
    if lines.len() != paths.len() {
        return HashMap::new();
    }
    paths
        .iter()
        .zip(lines)
        .filter(|(_, line)| !line.trim().is_empty() && !line.contains("not owned"))
        .map(|(path, line)| (path.to_path_buf(), Owner::Rpm(line.trim().to_string())))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageInfo {
    pub version: String,
    pub bytes: Option<u64>,
}

/// `name<TAB>version<TAB>size` lines; `scale` turns the size column into bytes (dpkg reports KiB).
pub fn parse_package_info(out: &str, scale: u64) -> HashMap<String, PackageInfo> {
    out.lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let name = cols.next()?.trim();
            let version = cols.next()?.trim();
            let bytes = cols
                .next()
                .and_then(|s| s.trim().parse::<u64>().ok())
                .map(|n| n * scale);
            (!name.is_empty()).then(|| {
                (
                    name.to_string(),
                    PackageInfo {
                        version: version.to_string(),
                        bytes,
                    },
                )
            })
        })
        .collect()
}

fn package_info(manager: Manager) -> HashMap<String, PackageInfo> {
    match manager {
        Manager::Dpkg => parse_package_info(
            &output(
                "dpkg-query",
                &["-W", "-f", "${Package}\t${Version}\t${Installed-Size}\n"],
            ),
            1024,
        ),
        Manager::Rpm => parse_package_info(
            &output(
                "rpm",
                &[
                    "-qa",
                    "--queryformat",
                    "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{SIZE}\\n",
                ],
            ),
            1,
        ),
        Manager::Unknown => HashMap::new(),
    }
}

/// `flatpak list --app --columns=application,version`: installed Flatpak apps and their versions.
pub fn parse_flatpak_list(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            Some((cols.next()?.to_string(), cols.next()?.to_string()))
        })
        .collect()
}

/// `snap list`: a header, then name, version, revision… per snap.
pub fn parse_snap_list(out: &str) -> HashMap<String, String> {
    out.lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let name = cols.next()?;
            let version = cols.next()?;
            (name != "Name").then(|| (name.to_string(), version.to_string()))
        })
        .collect()
}

/// Bytes on disk under a folder, following symlinks (Flatpak and Snap install through them).
fn dir_bytes(path: &Path) -> Option<u64> {
    host::run(&["du", "-sLk", &path.to_string_lossy()])
        .ok()?
        .split_whitespace()
        .next()?
        .parse::<u64>()
        .ok()
        .map(|k| k * 1024)
}

fn file_bytes(path: &Path) -> Option<u64> {
    host::size(path)
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// A Snap's desktop files are named `<snap>_<app>.desktop`.
fn snap_name(path: &Path) -> String {
    let stem = stem(path);
    stem.split('_').next().unwrap_or_default().to_string()
}

fn flatpak_install(id: &str, user: bool, home: &Path) -> PathBuf {
    let root = if user {
        home.join(".local/share/flatpak")
    } else {
        PathBuf::from("/var/lib/flatpak")
    };
    root.join("app").join(id).join("current/active")
}

pub fn installed_apps() -> Vec<InstalledApp> {
    let home = home();
    let desktops: Vec<Desktop> = scan_desktop(&desktop_dirs(&home), &locales())
        .into_iter()
        .filter(|d| !d.no_display)
        .collect();
    let system: Vec<&Path> = desktops
        .iter()
        .map(|d| d.path.as_path())
        .filter(|p| location_of(p, &home) == Location::System)
        .collect();
    let owners = owners_of(&system);
    let manager = if owners.is_empty() {
        Manager::Unknown
    } else {
        manager()
    };
    let packages = package_info(manager);
    let flatpak = parse_flatpak_list(&output(
        "flatpak",
        &["list", "--app", "--columns=application,version"],
    ));
    let snaps = parse_snap_list(&output("snap", &["list"]));

    let mut apps: Vec<InstalledApp> = desktops
        .into_iter()
        .map(|d| {
            let (source, version, bytes) = match location_of(&d.path, &home) {
                Location::Flatpak { user } => {
                    let id = stem(&d.path);
                    let bytes = dir_bytes(&flatpak_install(&id, user, &home));
                    (Source::Flatpak, flatpak.get(&id).cloned(), bytes)
                }
                Location::Snap => {
                    let name = snap_name(&d.path);
                    let bytes = dir_bytes(&Path::new("/snap").join(&name).join("current"));
                    (Source::Snap, snaps.get(&name).cloned(), bytes)
                }
                Location::System => match owners.get(&d.path) {
                    Some(Owner::Deb(pkg)) => {
                        let info = packages.get(pkg);
                        (
                            Source::Deb,
                            info.map(|i| i.version.clone()),
                            info.and_then(|i| i.bytes),
                        )
                    }
                    Some(Owner::Rpm(pkg)) => {
                        let info = packages.get(pkg);
                        (
                            Source::Rpm,
                            info.map(|i| i.version.clone()),
                            info.and_then(|i| i.bytes),
                        )
                    }
                    None => (Source::Manual, None, file_bytes(&d.path)),
                },
                Location::Home => (Source::Manual, None, file_bytes(&d.path)),
            };
            InstalledApp {
                name: d.name,
                version: version.unwrap_or_else(|| "—".into()),
                path: d.path,
                bytes,
                source: source.label(),
            }
        })
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// What uninstalling an app runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Removal {
    Flatpak { id: String, user: bool },
    Snap(String),
    Deb(String),
    Rpm(String),
    Trash,
}

pub fn removal_for(path: &Path, home: &Path, owner: Option<&Owner>) -> Removal {
    match location_of(path, home) {
        Location::Flatpak { user } => Removal::Flatpak {
            id: stem(path),
            user,
        },
        Location::Snap => Removal::Snap(snap_name(path)),
        Location::System => match owner {
            Some(Owner::Deb(pkg)) => Removal::Deb(pkg.clone()),
            Some(Owner::Rpm(pkg)) => Removal::Rpm(pkg.clone()),
            None => Removal::Trash,
        },
        Location::Home => Removal::Trash,
    }
}

fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| w.to_string()).collect()
}

pub fn removal_argv(removal: &Removal, desktop: &Path) -> Vec<String> {
    let desktop = desktop.to_string_lossy().into_owned();
    match removal {
        Removal::Flatpak { id, user } => argv(&[
            "flatpak",
            "uninstall",
            "-y",
            if *user { "--user" } else { "--system" },
            id.as_str(),
        ]),
        Removal::Snap(name) => argv(&["pkexec", "snap", "remove", name.as_str()]),
        Removal::Deb(pkg) => argv(&["pkexec", "apt-get", "remove", "-y", pkg.as_str()]),
        Removal::Rpm(pkg) => argv(&["pkexec", "dnf", "remove", "-y", pkg.as_str()]),
        Removal::Trash => argv(&["gio", "trash", desktop.as_str()]),
    }
}

/// Uninstall an app listed by `installed_apps`: its package manager removes it, or a manual
/// launcher goes to the Trash. A cancelled authorization prompt is reported as such.
pub fn trash_app(path: &Path) -> Result<(), String> {
    let home = home();
    let listed = path.extension().and_then(|x| x.to_str()) == Some("desktop")
        && path
            .parent()
            .is_some_and(|dir| desktop_dirs(&home).iter().any(|d| d == dir))
        && !path.to_string_lossy().contains("..");
    if !listed {
        return Err("Only installed apps can be uninstalled".into());
    }
    if !host::exists(path) {
        return Err("App not found".into());
    }
    let owner = (location_of(path, &home) == Location::System)
        .then(|| owners_of(&[path]).remove(path))
        .flatten();
    let argv = removal_argv(&removal_for(path, &home, owner.as_ref()), path);
    let words: Vec<&str> = argv.iter().map(String::as_str).collect();
    let out = host::output(&words).map_err(|e| format!("{}: {e}", argv[0]))?;
    if out.status.success() {
        return Ok(());
    }
    if out.status.code() == Some(126) {
        return Err("Authorization was cancelled".into());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    Err(if err.is_empty() {
        format!("{} exited with {}", argv[0], out.status)
    } else {
        err
    })
}

/// Desktop entries by the program they start, for matching a running process to its app.
pub struct ExeIndex {
    entries: Vec<Desktop>,
    by_path: HashMap<PathBuf, usize>,
    by_name: HashMap<String, usize>,
}

/// Programs that many apps start, which name no particular app.
const GENERIC_PROGRAMS: &[&str] = &[
    "env", "sh", "bash", "zsh", "python", "python3", "node", "java", "flatpak", "snap", "pkexec",
    "sudo", "gio", "xdg-open",
];

/// The program a desktop entry's `Exec=` starts, past `env` and `VAR=value` words.
pub fn exec_program(exec: &str) -> Option<&str> {
    exec.split_whitespace()
        .find(|w| *w != "env" && !w.contains('='))
        .map(|w| w.trim_matches('"'))
}

impl ExeIndex {
    pub fn new(entries: Vec<Desktop>) -> Self {
        let mut by_path = HashMap::new();
        let mut by_name = HashMap::new();
        for (i, d) in entries.iter().enumerate() {
            let Some(program) = d.exec.as_deref().and_then(exec_program) else {
                continue;
            };
            let program = PathBuf::from(program);
            if program.is_absolute() {
                by_path.entry(program.clone()).or_insert(i);
            }
            if let Some(name) = program.file_name().and_then(|n| n.to_str())
                && !GENERIC_PROGRAMS.contains(&name)
            {
                by_name.entry(name.to_string()).or_insert(i);
            }
        }
        Self {
            entries,
            by_path,
            by_name,
        }
    }

    pub fn desktop_for_exe(&self, exe: &Path) -> Option<&Desktop> {
        let i = self.by_path.get(exe).or_else(|| {
            exe.file_name()
                .and_then(|n| n.to_str())
                .and_then(|n| self.by_name.get(n))
        })?;
        self.entries.get(*i)
    }
}

static EXE_INDEX: OnceLock<ExeIndex> = OnceLock::new();

/// The process-wide index, read the first time a process is matched.
pub fn exe_index() -> &'static ExeIndex {
    EXE_INDEX.get_or_init(|| ExeIndex::new(scan_desktop(&desktop_dirs(&home()), &locales())))
}
