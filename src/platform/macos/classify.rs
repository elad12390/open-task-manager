//! Which group a macOS process belongs to: macOS's own (by subsystem), an app bundle, or a tool.

use super::CAPS;
use crate::{categories::OTHER_SYSTEM, metrics::ProcessInfo};

const TOOLS: &str = "Command-line & Other";

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
pub fn classify(p: &ProcessInfo) -> (&'static str, String) {
    if p.path.is_empty() {
        // No path readable: system accounts and the kernel are macOS; anything else is unknown.
        return if p.pid == 0 || p.user == "root" || p.user.starts_with('_') {
            (CAPS.system_group, subsystem(&p.name).to_string())
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
        return (CAPS.system_group, subsystem(&p.name).to_string());
    }
    match app_name(&p.path) {
        Some(app) => (CAPS.apps_group, app),
        None => (TOOLS, "Command-line tools".to_string()),
    }
}
