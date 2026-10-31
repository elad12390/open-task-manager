//! "Category" grouping: macOS's own processes (split into related subsystems) versus apps
//! (one section per app bundle) and command-line tools.

use std::collections::HashMap;

use crate::metrics::ProcessInfo;

pub const MACOS: &str = "macOS";
pub const APPS: &str = "Apps";
pub const TOOLS: &str = "Command-line & Other";

/// Top-level groups in display order.
pub const TOPS: [&str; 3] = [MACOS, APPS, TOOLS];

/// macOS subsystems, matched against the lowercased process name in this order.
const SUBSYSTEMS: &[(&str, &[&str])] = &[
    (
        "Core & Kernel",
        &[
            "kernel_task",
            "launchd",
            "logd",
            "syslogd",
            "configd",
            "notifyd",
            "distnoted",
            "cfprefsd",
            "opendirectoryd",
            "diskarbitrationd",
            "fseventsd",
            "kernelmanagerd",
            "watchdogd",
            "usereventagent",
            "runningboardd",
            "coreservicesd",
            "lsd",
            "launchservicesd",
            "containermanagerd",
            "xpcproxy",
            "logind",
            "dasd",
            "aslmanager",
            "mobileassetd",
            "diagnostic",
            "spindump",
            "reportcrash",
            "analytics",
            "contextstored",
            "backgroundtaskmanagement",
            "extensionkit",
            "pboard",
            "coreduetd",
            "loginwindow",
            "smd",
            "apfsd",
            "autofsd",
            "automountd",
            "deleted",
            "storagekitd",
            "filecoordinationd",
            "sharedfilelistd",
            "sysmond",
            "systemstats",
            "systemstatusd",
            "usermanagerd",
            "crashreporter",
            "submitdiag",
            "coresymbolication",
            "corerepaird",
            "backupd",
            "containermetadata",
            "pluginlibrary",
            "appleeventsd",
            "oahd",
            "mobileactivation",
            "eligibilityd",
            "triald",
            "adid",
            "kerneleventagent",
            "suhelperd",
            "pbs",
            "mdwrite",
            "homed",
            "hiservices",
        ],
    ),
    (
        "Windows & Interface",
        &[
            "windowserver",
            "dock",
            "finder",
            "controlcenter",
            "notificationcenter",
            "systemuiserver",
            "windowmanager",
            "textinput",
            "universalaccess",
            "cursoruiviewservice",
            "axvisualsupport",
            "axassets",
            "wallpaper",
            "viewbridge",
            "talagent",
            "chronod",
            "widget",
            "menu",
            "spaces",
            "screentime",
            "loginitem",
            "appkit",
            "uiservice",
            "accessibility",
            "inputmethod",
            "emoji",
            "keyboard",
            "fontd",
            "fontworker",
            "quicklook",
            "thumbnail",
            "iconservices",
            "usernotification",
            "coreservicesui",
            "uikitsystem",
            "shortcuts",
            "donotdisturb",
            "usernoted",
            "liveactivities",
            "statuskit",
            "imklaunchagent",
            "applespell",
            "tipsd",
            "recentsd",
            "feedbackd",
            "usagetracking",
            "menotification",
            "pah_extension",
        ],
    ),
    (
        "Audio & Media",
        &[
            "audio",
            "media",
            "airplay",
            "avconference",
            "photo",
            "music",
            "camera",
            "vdc",
            "cmio",
            "coremedia",
            "videotoolbox",
            "speech",
            "soundanalysis",
            "tvd",
            "podcast",
            "vision",
            "synthesizer",
            "macintalk",
            "ausp",
            "vtdecoder",
            "vtencoder",
            "replayd",
            "systemsoundserver",
            "heard",
            "rcd",
        ],
    ),
    (
        "Networking & Sharing",
        &[
            "mdnsresponder",
            "network",
            "symptomsd",
            "nesession",
            "wifi",
            "airport",
            "bluetooth",
            "sharingd",
            "rapportd",
            "nearbyd",
            "identityservices",
            "apsd",
            "airdrop",
            "nfc",
            "vpn",
            "nsurl",
            "netbios",
            "wireless",
            "continuity",
            "handoff",
            "callservices",
            "telephony",
            "facetime",
            "socket",
            "commcenter",
            "neagent",
            "nehelper",
            "remoted",
            "rtcreporting",
            "sociallayer",
            "contentlinking",
            "linkd",
            "swcd",
            "assetcache",
            "ampdevicediscovery",
            "useractivityd",
            "imdpersistence",
            "imagent",
        ],
    ),
    (
        "Security & Privacy",
        &[
            "secur",
            "trust",
            "secd",
            "tccd",
            "syspolicy",
            "endpointsecurity",
            "xprotect",
            "mrt",
            "amfid",
            "sandbox",
            "keybag",
            "authd",
            "biometric",
            "keychain",
            "policy",
            "codesign",
            "gatekeeper",
            "akd",
            "passd",
            "lockdown",
            "privacy",
            "appsso",
            "credential",
            "ctkd",
            "ctkahp",
            "seputil",
            "devicecheck",
            "transparency",
            "lockout",
            "online-auth",
            "assessment",
            "taskgated",
            "secinitd",
            "familycircle",
            "dmd",
            "pkd",
        ],
    ),
    (
        "Spotlight & Search",
        &[
            "mds",
            "mdworker",
            "mdbulkimport",
            "spotlight",
            "corespotlight",
            "suggest",
            "knowledge",
            "searchparty",
        ],
    ),
    (
        "iCloud & Sync",
        &[
            "cloud",
            "bird",
            "sync",
            "fileprovider",
            "accountsd",
            "findmy",
            "fmf",
            "remindd",
            "calendar",
            "calaccessd",
            "contactsd",
            "maild",
        ],
    ),
    (
        "Siri & Intelligence",
        &[
            "siri",
            "assistant",
            "intelligen",
            "generative",
            "proactive",
            "duet",
            "biome",
            "ml",
            "model",
            "translation",
            "ane",
            "naturallanguage",
            "contextservice",
            "contextstore",
            "categoriesservice",
            "promotedcontent",
        ],
    ),
    (
        "Location & Time",
        &[
            "locationd",
            "corelocation",
            "geod",
            "routined",
            "timed",
            "weatherd",
        ],
    ),
    (
        "Web & Virtualization",
        &["webkit", "virtualization", "safariplatform", "webcontent"],
    ),
    (
        "Power & Hardware",
        &[
            "powerd",
            "power",
            "thermal",
            "battery",
            "usb",
            "hid",
            "display",
            "smc",
            "driverkit",
            "accessory",
            "hardware",
            "iokit",
            "sensor",
            "gpu",
            "metal",
            "mtl",
            "corebrightness",
            "backlight",
            "iomfb",
            "icdd",
            "cvmserver",
        ],
    ),
    (
        "Updates & Installs",
        &[
            "softwareupdate",
            "install",
            "appstore",
            "commerce",
            "storekit",
            "update",
            "mobileasset",
        ],
    ),
];

pub const OTHER_SYSTEM: &str = "Other System Services";

/// Is this executable part of macOS itself (not an app you installed or a tool you added)?
fn native_path(path: &str) -> bool {
    let apple_app = path.contains("/System/Applications/") || path.contains("/Cryptexes/App/");
    !apple_app
        && (path.starts_with("/System/")
            || path.starts_with("/usr/") && !path.starts_with("/usr/local/")
            || path.starts_with("/bin/")
            || path.starts_with("/sbin/")
            || path.starts_with("/Library/Apple/")
            || path.starts_with("/private/var/db/com.apple"))
}

/// The app a path belongs to: the outermost `.app` bundle (".../Google Chrome.app/.../Helper.app/..."
/// → "Google Chrome"), or, for bundles with another extension (".../Steam.AppBundle/Steam/Contents/
/// MacOS/ipcserver"), the folder that holds `Contents/MacOS`. Safari's system bundle (the "App"
/// cryptex) counts as Safari.
fn app_name(path: &str) -> Option<String> {
    if let Some(app) = path.split('/').find_map(|part| part.strip_suffix(".app")) {
        return Some(app.to_string());
    }
    if path.contains("/Cryptexes/App/") {
        return Some("Safari".to_string());
    }
    let parts: Vec<&str> = path.split('/').collect();
    parts
        .windows(3)
        .find(|w| w[1] == "Contents" && w[2] == "MacOS")
        .map(|w| w[0].split('.').next().unwrap_or(w[0]).to_string())
}

fn subsystem(name: &str) -> &'static str {
    let n = name.to_lowercase();
    SUBSYSTEMS
        .iter()
        .find(|(_, keys)| keys.iter().any(|k| n.contains(k)))
        .map(|(sub, _)| *sub)
        .unwrap_or(OTHER_SYSTEM)
}

/// Category of a process from its own executable alone.
fn own_category(p: &ProcessInfo) -> (&'static str, String) {
    if p.path.is_empty() {
        // No path readable: system accounts and the kernel are macOS; anything else is unknown.
        return if p.pid == 0 || p.user == "root" || p.user.starts_with('_') {
            (MACOS, subsystem(&p.name).to_string())
        } else {
            (TOOLS, "Unknown".to_string())
        };
    }
    // Shells and commands from /bin and /usr/bin that you run (not a system account) are your tools.
    let system_account = p.user == "root" || p.user.starts_with('_');
    if !system_account && (p.path.starts_with("/bin/") || p.path.starts_with("/usr/bin/")) {
        return (TOOLS, "Command-line tools".to_string());
    }
    if native_path(&p.path) {
        return (MACOS, subsystem(&p.name).to_string());
    }
    match app_name(&p.path) {
        Some(app) => (APPS, app),
        None => (TOOLS, "Command-line tools".to_string()),
    }
}

/// The owner shown for processes that belong to macOS itself rather than to any app.
pub const OS_OWNER: &str = "Operating System";

/// The app (or tool) each process belongs to: the app bundle for app processes and their helpers,
/// the outermost command-line tool for tool processes, and "Operating System" for macOS's own.
pub fn owners(processes: &[ProcessInfo]) -> HashMap<u32, String> {
    let cat = categorize(processes);
    let by_pid: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, p)).collect();
    processes
        .iter()
        .map(|p| {
            let owner = match cat.get(&p.pid) {
                Some((top, _)) if *top == MACOS => OS_OWNER.to_string(),
                Some((top, app)) if *top == APPS => app.clone(),
                _ => {
                    // A tool: name it after the outermost non-system process it runs under.
                    let mut top = p;
                    let mut hops = 0;
                    while let Some(parent) = top
                        .ppid
                        .filter(|&pp| pp > 1 && pp != top.pid && hops < 64)
                        .and_then(|pp| by_pid.get(&pp).copied())
                        .filter(|pa| cat.get(&pa.pid).is_some_and(|c| c.0 != MACOS))
                    {
                        top = parent;
                        hops += 1;
                    }
                    top.name.clone()
                }
            };
            (p.pid, owner)
        })
        .collect()
}

/// (top group, section) for every process. A system binary started under one of your apps or tools
/// (a shell in a terminal, a helper an app spawned) is filed with it, because that is what it belongs to.
pub fn categorize(processes: &[ProcessInfo]) -> HashMap<u32, (&'static str, String)> {
    let by_pid: HashMap<u32, &ProcessInfo> = processes.iter().map(|p| (p.pid, p)).collect();
    let own: HashMap<u32, (&'static str, String)> =
        processes.iter().map(|p| (p.pid, own_category(p))).collect();
    processes
        .iter()
        .map(|p| {
            let mine = own[&p.pid].clone();
            if mine.0 != MACOS {
                return (p.pid, mine);
            }
            // A system binary started by an app or a tool (a shell in a terminal, `grep` in a script)
            // belongs with that app or tool: the nearest non-macOS ancestor decides. Stop at launchd.
            let mut cur = p.ppid;
            let mut hops = 0;
            while let Some(pp) = cur.filter(|&pp| pp > 1 && pp != p.pid && hops < 64) {
                if let Some(c) = own.get(&pp).filter(|c| c.0 != MACOS) {
                    return (p.pid, c.clone());
                }
                cur = by_pid
                    .get(&pp)
                    .and_then(|a| a.ppid)
                    .filter(|&next| next != pp);
                hops += 1;
            }
            (p.pid, mine)
        })
        .collect()
}
