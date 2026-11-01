//! systemd services: your user units through `systemctl --user`, and system units through `systemctl`,
//! which restarts and stops run through `pkexec` so polkit asks for the password.

use std::{collections::HashMap, path::PathBuf};

use super::host;
use crate::sysops::{Service, ServiceAction};

const PROPERTIES: &str = "--property=Id,LoadState,MainPID,ExecMainStatus,ExecStart";

/// `systemctl show` prints one `Key=value` block per unit, separated by blank lines.
fn parse_units(text: &str) -> Vec<HashMap<String, String>> {
    text.split("\n\n")
        .map(|block| {
            block
                .lines()
                .filter_map(|l| {
                    let (k, v) = l.split_once('=')?;
                    Some((k.to_string(), v.to_string()))
                })
                .collect::<HashMap<_, _>>()
        })
        .filter(|unit| unit.contains_key("Id"))
        .collect()
}

/// The program of `ExecStart={ path=/usr/bin/sleep ; argv[]=… }`.
fn exec_path(exec_start: &str) -> Option<PathBuf> {
    let path = exec_start
        .split("path=")
        .nth(1)?
        .split_whitespace()
        .next()?;
    path.starts_with('/').then(|| PathBuf::from(path))
}

fn service_of(unit: &HashMap<String, String>, scope: &'static str) -> Option<Service> {
    if unit.get("LoadState").map(String::as_str) != Some("loaded") {
        return None;
    }
    Some(Service {
        label: unit.get("Id")?.clone(),
        pid: unit
            .get("MainPID")
            .and_then(|p| p.parse::<u32>().ok())
            .filter(|&p| p > 0),
        status: unit
            .get("ExecMainStatus")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        program: unit.get("ExecStart").and_then(|e| exec_path(e)),
        scope,
    })
}

pub fn services() -> Vec<Service> {
    let mut list = Vec::new();
    for (scope, flags) in [("System", &[][..]), ("User", &["--user"][..])] {
        let mut args = flags.to_vec();
        args.extend(["show", "--all", "--type=service", PROPERTIES]);
        let Ok(out) = host("systemctl", &args) else {
            continue;
        };
        list.extend(
            parse_units(&out)
                .iter()
                .filter_map(|u| service_of(u, scope)),
        );
    }
    list
}

fn valid_label(label: &str) -> bool {
    !label.is_empty()
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-@:".contains(c))
}

/// Restart or stop a service. A unit of your user manager is controlled directly; anything else is a
/// system unit and goes through polkit.
pub fn control_service(label: &str, action: ServiceAction) -> Result<(), String> {
    if !valid_label(label) {
        return Err("Invalid service name".into());
    }
    let verb = match action {
        ServiceAction::Restart => "restart",
        ServiceAction::Stop => "stop",
    };
    let is_user_unit = host(
        "systemctl",
        &["--user", "show", "--property=LoadState", "--value", label],
    )
    .is_ok_and(|s| s.trim() == "loaded");
    if is_user_unit {
        host("systemctl", &["--user", verb, label]).map(|_| ())
    } else {
        host("pkexec", &["systemctl", verb, label]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHOW: &str = "Id=cron.service\nLoadState=loaded\nMainPID=812\nExecMainStatus=0\nExecStart={ path=/usr/sbin/cron ; argv[]=/usr/sbin/cron -f ; }\n\nId=bad.service\nLoadState=not-found\nMainPID=0\n\nId=oneshot.service\nLoadState=loaded\nMainPID=0\nExecMainStatus=3\nExecStart={ path=/bin/false ; argv[]=/bin/false ; }\n";

    #[test]
    fn show_blocks_become_services() {
        let units = parse_units(SHOW);
        assert_eq!(units.len(), 3);
        let cron = service_of(&units[0], "System").unwrap();
        assert_eq!(cron.label, "cron.service");
        assert_eq!(cron.pid, Some(812));
        assert_eq!(cron.program, Some(PathBuf::from("/usr/sbin/cron")));
        assert_eq!(cron.scope, "System");
    }

    #[test]
    fn stopped_units_have_no_pid_and_keep_their_last_exit() {
        let units = parse_units(SHOW);
        let oneshot = service_of(&units[2], "User").unwrap();
        assert_eq!(oneshot.pid, None);
        assert_eq!(oneshot.status, 3);
        assert_eq!(oneshot.scope, "User");
    }

    #[test]
    fn units_that_are_not_loaded_are_left_out() {
        let units = parse_units(SHOW);
        assert!(service_of(&units[1], "System").is_none());
    }

    #[test]
    fn labels_must_be_plain_unit_names() {
        assert!(valid_label("pipewire.service"));
        assert!(valid_label("getty@tty1.service"));
        assert!(!valid_label("a;rm -rf /"));
        assert!(!valid_label(""));
    }
}
