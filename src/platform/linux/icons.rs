//! Icons from the freedesktop icon themes. An app's icon is its desktop entry's `Icon=`, a process
//! shows the app that runs it, and system processes, terminals and folders get the theme's generic
//! icons. Keys are a desktop file, an executable or folder path, or `icon-name:<name>`.

use std::path::{Path, PathBuf};

use super::{apps, host};
use crate::sysops::home;

/// The generic user icon (accounts' pictures are not read yet).
pub const USER_ICON: &str = "icon-name:avatar-default";

const NAME_PREFIX: &str = "icon-name:";
const SIZE: u32 = 32;
const SHELLS: &[&str] = &["bash", "zsh", "sh", "fish", "dash", "ksh", "tcsh", "nu"];
const SYSTEM_DIRS: &[&str] = &[
    "/usr/lib/",
    "/usr/libexec/",
    "/usr/sbin/",
    "/sbin/",
    "/lib/",
    "/usr/lib64/",
];
const KERNEL_THREADS: &[&str] = &[
    "kworker",
    "ksoftirqd",
    "kthreadd",
    "kswapd",
    "rcu_",
    "migration/",
    "cpuhp/",
    "irq/",
    "kcompactd",
    "khugepaged",
    "jbd2/",
    "watchdog/",
    "kauditd",
    "kdevtmpfs",
];

/// The desktop file of the app an executable belongs to.
pub fn bundle_of(path: &str) -> Option<String> {
    if !path.starts_with('/') || path.ends_with(".desktop") {
        return None;
    }
    apps::exe_index()
        .desktop_for_exe(Path::new(path))
        .map(|d| d.path.to_string_lossy().into_owned())
}

/// The gear icon of a kernel thread (its name is all there is to go by).
pub fn system_icon(name: &str) -> Option<String> {
    KERNEL_THREADS
        .iter()
        .any(|p| name.starts_with(p))
        .then(|| format!("{NAME_PREFIX}applications-system"))
}

/// The theme icon of a folder: the home folder and the XDG user folders have their own.
pub fn folder_icon_name(path: &Path, home: &Path) -> &'static str {
    if path == home {
        return "user-home";
    }
    match path.file_name().and_then(|n| n.to_str()) {
        Some("Desktop") => "user-desktop",
        Some("Documents") => "folder-documents",
        Some("Downloads") => "folder-download",
        Some("Music") => "folder-music",
        Some("Pictures") => "folder-pictures",
        Some("Videos") => "folder-videos",
        Some("Public") => "folder-publicshare",
        Some("Templates") => "folder-templates",
        _ => "folder",
    }
}

/// Terminals, system programs and everything else that runs without an app.
pub fn exe_icon_name(path: &Path) -> &'static str {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if SHELLS.contains(&name) {
        "utilities-terminal"
    } else if SYSTEM_DIRS.iter().any(|d| path.starts_with(d)) {
        "applications-system"
    } else {
        "application-x-executable"
    }
}

/// The icon directories of the freedesktop spec (user, system, Flatpak and Snap exports, and the
/// host's when running inside a Flatpak sandbox), keeping those that exist.
fn icon_roots(home: &Path) -> Vec<PathBuf> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    let mut roots = vec![data_home.join("icons"), home.join(".icons")];
    roots.extend(
        data_dirs
            .split(':')
            .filter(|d| !d.is_empty())
            .map(|d| PathBuf::from(d).join("icons")),
    );
    roots.extend(
        [
            "/var/lib/flatpak/exports/share/icons",
            "/var/lib/snapd/desktop/icons",
            "/run/host/usr/local/share/icons",
            "/run/host/usr/share/icons",
            "/run/host/var/lib/flatpak/exports/share/icons",
        ]
        .map(PathBuf::from),
    );
    roots.push(home.join(".local/share/flatpak/exports/share/icons"));
    roots.into_iter().filter(|p| host::is_dir(p)).collect()
}

fn pixmap_dirs() -> Vec<PathBuf> {
    ["/usr/share/pixmaps", "/run/host/usr/share/pixmaps"]
        .iter()
        .map(PathBuf::from)
        .filter(|p| host::is_dir(p))
        .collect()
}

/// The icon theme in use: GNOME's setting, else KDE's, else hicolor.
fn theme_name(home: &Path) -> String {
    let gnome = host::output(&[
        "gsettings",
        "get",
        "org.gnome.desktop.interface",
        "icon-theme",
    ])
    .ok()
    .filter(|o| o.status.success())
    .map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .trim()
            .trim_matches('\'')
            .to_string()
    })
    .filter(|s| !s.is_empty());
    gnome
        .or_else(|| {
            let text = host::read(home.join(".config/kdeglobals")).ok()?;
            ini_get(&text, "Icons", "Theme").map(str::to_string)
        })
        .unwrap_or_else(|| "hicolor".into())
}

/// `key=value` under `[section]` of an INI-style file (index.theme, kdeglobals).
fn ini_get<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut current = "";
    for line in text.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name;
            continue;
        }
        if current != section {
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim());
        }
    }
    None
}

/// A theme and the themes it inherits from, in lookup order, hicolor last.
fn theme_chain(roots: &[PathBuf], theme: &str) -> Vec<String> {
    let mut chain = vec![theme.to_string()];
    let mut i = 0;
    while i < chain.len() {
        let current = chain[i].clone();
        i += 1;
        for root in roots {
            let Ok(text) = host::read(root.join(&current).join("index.theme")) else {
                continue;
            };
            let parents = ini_get(&text, "Icon Theme", "Inherits").unwrap_or("");
            for parent in parents.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                if !chain.iter().any(|t| t == parent) {
                    chain.push(parent.to_string());
                }
            }
        }
    }
    if !chain.iter().any(|t| t == "hicolor") {
        chain.push("hicolor".into());
    }
    chain
}

/// One icon directory of a theme. `size` is None for scalable directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeDir {
    pub path: PathBuf,
    pub size: Option<u32>,
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    host::read_dir(dir)
        .map(|r| r.into_iter().filter(|p| host::is_dir(p)).collect())
        .unwrap_or_default()
}

/// The directories of a theme, from its `index.theme`, or by scanning `<size>/<context>` folders
/// when it has none.
pub fn theme_dirs(base: &Path) -> Vec<ThemeDir> {
    let Ok(text) = host::read(base.join("index.theme")) else {
        return scan_theme_dirs(base);
    };
    let directories = ini_get(&text, "Icon Theme", "Directories").unwrap_or("");
    directories
        .split(',')
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|rel| {
            let scalable = ini_get(&text, rel, "Type") == Some("Scalable");
            let size = if scalable {
                None
            } else {
                ini_get(&text, rel, "Size").and_then(|s| s.parse().ok())
            };
            ThemeDir {
                path: base.join(rel),
                size,
            }
        })
        .filter(|d| host::is_dir(&d.path))
        .collect()
}

fn scan_theme_dirs(base: &Path) -> Vec<ThemeDir> {
    let mut out = Vec::new();
    for dir in subdirs(base) {
        let Some(name) = dir.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let size = if name == "scalable" {
            None
        } else {
            match name.split('x').next().and_then(|w| w.parse::<u32>().ok()) {
                Some(size) => Some(size),
                None => continue,
            }
        };
        out.extend(
            subdirs(&dir)
                .into_iter()
                .map(|path| ThemeDir { path, size }),
        );
    }
    out
}

/// Ranks candidates: an SVG beats any PNG; among PNGs the smallest one at least `SIZE` wins, else
/// the largest one below it.
fn rank(ext: &str, size: Option<u32>) -> (u8, u32) {
    match (ext, size) {
        ("svg", _) => (0, 0),
        (_, Some(s)) if s >= SIZE => (1, s),
        (_, Some(s)) => (2, 1000 - s.min(1000)),
        _ => (3, 0),
    }
}

fn strip_icon_ext(icon: &str) -> &str {
    icon.strip_suffix(".png")
        .or_else(|| icon.strip_suffix(".svg"))
        .or_else(|| icon.strip_suffix(".xpm"))
        .unwrap_or(icon)
}

/// The icon theme for lookups: the themes in inheritance order, each with its directories under
/// every icon root, and the pixmap folders as the last resort.
pub struct Theme {
    chain: Vec<Vec<ThemeDir>>,
    pixmaps: Vec<PathBuf>,
}

impl Theme {
    pub fn new(roots: &[PathBuf], pixmaps: &[PathBuf], theme: &str) -> Self {
        let chain = theme_chain(roots, theme)
            .iter()
            .map(|t| {
                roots
                    .iter()
                    .map(|r| r.join(t))
                    .filter(|base| host::is_dir(base))
                    .flat_map(|base| theme_dirs(&base))
                    .collect()
            })
            .collect();
        Self {
            chain,
            pixmaps: pixmaps.to_vec(),
        }
    }

    /// The theme the desktop uses, with its icon folders.
    pub fn current() -> Self {
        let home = home();
        Self::new(&icon_roots(&home), &pixmap_dirs(), &theme_name(&home))
    }

    /// The file of an icon name (extension optional) or an absolute icon path.
    pub fn lookup(&self, icon: &str) -> Option<PathBuf> {
        if icon.starts_with('/') {
            let path = PathBuf::from(icon);
            return host::is_file(&path).then_some(path);
        }
        let name = strip_icon_ext(icon);
        for dirs in &self.chain {
            let best = dirs
                .iter()
                .flat_map(|dir| {
                    ["svg", "png"].into_iter().filter_map(move |ext| {
                        let path = dir.path.join(format!("{name}.{ext}"));
                        host::is_file(&path).then(|| (rank(ext, dir.size), path))
                    })
                })
                .min_by_key(|(rank, _)| *rank);
            if let Some((_, path)) = best {
                return Some(path);
            }
        }
        self.pixmaps
            .iter()
            .flat_map(|dir| ["svg", "png"].map(|ext| dir.join(format!("{name}.{ext}"))))
            .find(|p| host::is_file(p))
    }
}

fn desktop_icon(path: &Path) -> Option<String> {
    let text = host::read(path).ok()?;
    apps::parse_desktop(path, &text, &[])?.icon
}

/// The file that draws an icon key, if the theme has one.
fn icon_file(key: &str, theme: &Theme, home: &Path) -> Option<PathBuf> {
    if let Some(name) = key.strip_prefix(NAME_PREFIX) {
        return theme.lookup(name);
    }
    let path = Path::new(key);
    if !path.is_absolute() {
        return None;
    }
    if host::is_dir(path) {
        return theme.lookup(folder_icon_name(path, home));
    }
    if key.ends_with(".desktop") {
        return desktop_icon(path)
            .and_then(|icon| theme.lookup(&icon))
            .or_else(|| theme.lookup("application-x-executable"));
    }
    theme.lookup(exe_icon_name(path))
}

/// Write a 32×32 PNG of an icon: PNGs are copied (GPUI scales them) and SVGs are rasterized.
pub fn write_png(src: &Path, png: &Path) -> bool {
    match src.extension().and_then(|e| e.to_str()) {
        Some("png") => host::copy(src, png).is_ok(),
        Some("svg") => rasterize(src, png),
        _ => false,
    }
}

fn rasterize(svg: &Path, png: &Path) -> bool {
    let Ok(data) = host::read_bytes(svg) else {
        return false;
    };
    let Ok(tree) = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()) else {
        return false;
    };
    let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(SIZE, SIZE) else {
        return false;
    };
    let size = tree.size();
    let scale = SIZE as f32 / size.width().max(size.height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.save_png(png).is_ok()
}

/// Render the icons of `(key, png)` jobs that the theme has. Returns the `(key, png)` pairs written.
pub fn render_icons(_dir: &Path, jobs: &[(String, PathBuf)]) -> Vec<(String, PathBuf)> {
    let home = home();
    let theme = Theme::current();
    jobs.iter()
        .filter_map(|(key, png)| {
            let src = icon_file(key, &theme, &home)?;
            write_png(&src, png).then(|| (key.clone(), png.clone()))
        })
        .collect()
}
