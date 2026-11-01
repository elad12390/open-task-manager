//! Hardware and system facts (sysctl, sw_vers, pmset, ioreg), sockets (lsof), installed apps and
//! moving an app to the Trash through Finder.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::sysops::{Connection, InstalledApp, Power, SystemInfo, home, run};

fn sysctl(key: &str) -> String {
    run("sysctl", &["-n", key])
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn system_info() -> SystemInfo {
    let booted = sysctl("kern.boottime")
        .split("sec = ")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    SystemInfo {
        model: sysctl("hw.model"),
        chip: sysctl("machdep.cpu.brand_string"),
        os: format!(
            "{} {}",
            run("sw_vers", &["-productName"]).unwrap_or_default().trim(),
            run("sw_vers", &["-productVersion"])
                .unwrap_or_default()
                .trim()
        ),
        build: run("sw_vers", &["-buildVersion"])
            .unwrap_or_default()
            .trim()
            .to_string(),
        arch: run("uname", &["-m"]).unwrap_or_default().trim().to_string(),
        memory: sysctl("hw.memsize").parse().unwrap_or(0),
        physical_cores: sysctl("hw.physicalcpu").parse().unwrap_or(0),
        performance_cores: sysctl("hw.perflevel0.physicalcpu").parse().unwrap_or(0),
        efficiency_cores: sysctl("hw.perflevel1.physicalcpu").parse().unwrap_or(0),
        booted,
    }
}

pub fn connections() -> Vec<Connection> {
    let out = run("lsof", &["-nP", "-i", "-F", "pcPn"]).unwrap_or_default();
    let (mut pid, mut cmd, mut proto) = (0, String::new(), String::new());
    let mut rows = Vec::new();
    for line in out.lines() {
        let Some(tag) = line.chars().next() else {
            continue;
        };
        let val = &line[1..];
        match tag {
            'p' => pid = val.parse().unwrap_or(0),
            'c' => cmd = val.to_string(),
            'P' => proto = val.to_string(),
            'n' => {
                let (endpoint, state) = match val.rfind(" (") {
                    Some(i) if val.ends_with(')') => {
                        (val[..i].to_string(), val[i + 2..val.len() - 1].to_string())
                    }
                    _ => (val.to_string(), String::new()),
                };
                rows.push(Connection {
                    pid,
                    process: cmd.clone(),
                    proto: proto.clone(),
                    endpoint,
                    state,
                });
            }
            _ => {}
        }
    }
    rows
}

fn plist_string(info: &Path, key: &str) -> Option<String> {
    run(
        "plutil",
        &["-extract", key, "raw", "-o", "-", &info.to_string_lossy()],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
}

pub fn installed_apps() -> Vec<InstalledApp> {
    let mut apps = Vec::new();
    for dir in [PathBuf::from("/Applications"), home().join("Applications")] {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) != Some("app") {
                continue;
            }
            let info = path.join("Contents/Info.plist");
            apps.push(InstalledApp {
                name: path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                version: plist_string(&info, "CFBundleShortVersionString")
                    .unwrap_or_else(|| "—".into()),
                path,
                bytes: None,
                source: "Application",
            });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

/// Move an app bundle to the Trash through Finder. Limited to /Applications and ~/Applications.
pub fn trash_app(path: &Path) -> Result<(), String> {
    let s = path.to_string_lossy();
    let roots = [PathBuf::from("/Applications"), home().join("Applications")];
    if path.extension().and_then(|x| x.to_str()) != Some("app")
        || !roots.iter().any(|r| path.parent() == Some(r.as_path()))
        || s.contains("..")
    {
        return Err("Only apps in Applications can be moved to the Trash".into());
    }
    if !path.exists() {
        return Err("App not found".into());
    }
    let script = format!(
        "tell application \"Finder\" to delete (POSIX file \"{}\" as alias)",
        s.replace('\\', "\\\\").replace('"', "\\\"")
    );
    run("osascript", &["-e", &script]).map(|_| ())
}

pub fn power() -> Power {
    let batt = run("pmset", &["-g", "batt"]).unwrap_or_default();
    let ioreg = run("ioreg", &["-rn", "AppleSmartBattery", "-w0"]).unwrap_or_default();
    let num = |k: &str| -> Option<i64> {
        let needle = format!("\"{k}\" = ");
        let i = ioreg.find(&needle)? + needle.len();
        let digits: String = ioreg[i..]
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '-')
            .collect();
        let v: i128 = digits.parse().ok()?;
        Some(if v > 9_000_000_000_000_000_000 {
            (v - (1i128 << 64)) as i64
        } else {
            v as i64
        })
    };
    let voltage = num("Voltage");
    let amperage = num("Amperage");
    let design = num("DesignCapacity");
    let max = num("NominalChargeCapacity").or_else(|| num("AppleRawMaxCapacity"));
    let percent = batt
        .split(['\t', ';'])
        .find_map(|p| p.trim().strip_suffix('%')?.parse().ok());
    let status = batt
        .split(';')
        .nth(1)
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".into());
    let pm = run("pmset", &["-g"]).unwrap_or_default();
    let therm = run("pmset", &["-g", "therm"]).unwrap_or_default();
    Power {
        has_battery: batt.contains("InternalBattery"),
        source: batt.split('\'').nth(1).unwrap_or("unknown").to_string(),
        percent,
        status,
        watts: voltage.zip(amperage).map(|(v, a)| (v * a) as f64 / 1e6),
        voltage: voltage.map(|v| v as f64 / 1000.),
        cycles: num("CycleCount"),
        health: design
            .zip(max)
            .filter(|(d, _)| *d > 0)
            .map(|(d, m)| m as f64 / d as f64 * 100.),
        temperature: num("Temperature").map(|t| t as f64 / 100.),
        low_power_mode: pm
            .lines()
            .any(|l| l.trim_start().starts_with("lowpowermode") && l.trim_end().ends_with('1')),
        thermal: if therm.contains("No thermal warning level has been recorded") {
            "Nominal".into()
        } else {
            therm.lines().last().unwrap_or("unknown").trim().to_string()
        },
    }
}
