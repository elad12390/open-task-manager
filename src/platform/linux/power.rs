//! Power from the kernel's `/sys/class/power_supply` (battery and AC adapter), `/sys/class/thermal`
//! (temperatures) and `powerprofilesctl` (the low-power profile). A desktop without a battery reads
//! as AC power with no battery figures.

use std::{collections::HashMap, path::PathBuf};

use super::host;
use crate::sysops::Power;

/// A supply's `uevent` file: `POWER_SUPPLY_<KEY>=<value>` lines, keyed by the part after the prefix.
pub fn parse_uevent(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|l| {
            let (k, v) = l.split_once('=')?;
            Some((
                k.strip_prefix("POWER_SUPPLY_")?.to_string(),
                v.trim().to_string(),
            ))
        })
        .collect()
}

fn num(supply: &HashMap<String, String>, key: &str) -> Option<f64> {
    supply.get(key)?.parse().ok()
}

/// Watts drawn from the battery: `power_now` (µW), or current × voltage (µA × µV).
fn watts(supply: &HashMap<String, String>) -> Option<f64> {
    num(supply, "POWER_NOW")
        .map(|uw| uw / 1e6)
        .or_else(|| Some(num(supply, "CURRENT_NOW")? * num(supply, "VOLTAGE_NOW")? / 1e12))
}

/// Battery health: the full charge now as a share of the design capacity.
fn health(supply: &HashMap<String, String>) -> Option<f64> {
    [
        ("ENERGY_FULL", "ENERGY_FULL_DESIGN"),
        ("CHARGE_FULL", "CHARGE_FULL_DESIGN"),
    ]
    .iter()
    .find_map(|(full, design)| {
        let d = num(supply, design).filter(|d| *d > 0.)?;
        Some(num(supply, full)? / d * 100.)
    })
}

/// The hottest thermal zone, as "62 °C", or "No sensors" when the kernel exposes none.
pub fn thermal_summary(zones: &[(String, f64)]) -> String {
    zones
        .iter()
        .map(|(_, c)| *c)
        .fold(None, |max: Option<f64>, c| {
            Some(max.map_or(c, |m| m.max(c)))
        })
        .map_or_else(|| "No sensors".into(), |c| format!("{c:.0} °C"))
}

fn supplies() -> Vec<HashMap<String, String>> {
    let Ok(entries) = host::read_dir("/sys/class/power_supply") else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|dir| host::read(dir.join("uevent")).ok())
        .map(|text| parse_uevent(&text))
        .collect()
}

fn thermal_zones() -> Vec<(String, f64)> {
    let Ok(entries) = host::read_dir("/sys/class/thermal") else {
        return Vec::new();
    };
    let dirs: Vec<PathBuf> = entries
        .into_iter()
        .filter(|p| host::exists(p.join("temp")))
        .collect();
    dirs.iter()
        .filter_map(|dir| {
            let celsius = host::read(dir.join("temp"))
                .ok()?
                .trim()
                .parse::<f64>()
                .ok()?
                / 1000.;
            let kind = host::read(dir.join("type")).unwrap_or_default();
            Some((kind.trim().to_string(), celsius))
        })
        .collect()
}

pub fn power() -> Power {
    let supplies = supplies();
    let battery = supplies.iter().find(|s| {
        s.get("TYPE").map(String::as_str) == Some("Battery")
            && s.get("PRESENT").is_none_or(|p| p == "1")
    });
    let ac_online = supplies.iter().any(|s| {
        matches!(s.get("TYPE").map(String::as_str), Some("Mains" | "USB"))
            && s.get("ONLINE").map(String::as_str) == Some("1")
    });
    let mut power = Power {
        has_battery: battery.is_some(),
        source: if battery.is_some() && !ac_online {
            "Battery Power".into()
        } else {
            "AC Power".into()
        },
        thermal: thermal_summary(&thermal_zones()),
        low_power_mode: host("powerprofilesctl", &["get"]).is_ok_and(|p| p.trim() == "power-saver"),
        ..Default::default()
    };
    if let Some(b) = battery {
        power.percent = num(b, "CAPACITY").map(|c| c as u32);
        power.status = b
            .get("STATUS")
            .map_or_else(|| "unknown".into(), |s| s.to_lowercase());
        power.watts = watts(b);
        power.voltage = num(b, "VOLTAGE_NOW").map(|v| v / 1e6);
        power.cycles = num(b, "CYCLE_COUNT").map(|c| c as i64);
        power.health = health(b);
        power.temperature = num(b, "TEMP").map(|t| t / 10.);
    }
    power
}

#[cfg(test)]
mod tests {
    use super::*;

    const BATTERY: &str = "POWER_SUPPLY_NAME=BAT0\nPOWER_SUPPLY_TYPE=Battery\nPOWER_SUPPLY_STATUS=Discharging\nPOWER_SUPPLY_PRESENT=1\nPOWER_SUPPLY_VOLTAGE_NOW=12500000\nPOWER_SUPPLY_POWER_NOW=8400000\nPOWER_SUPPLY_ENERGY_FULL=48000000\nPOWER_SUPPLY_ENERGY_FULL_DESIGN=52000000\nPOWER_SUPPLY_CYCLE_COUNT=214\nPOWER_SUPPLY_CAPACITY=73\nPOWER_SUPPLY_TEMP=312\n";

    #[test]
    fn uevent_keys_drop_the_prefix() {
        let b = parse_uevent(BATTERY);
        assert_eq!(b.get("TYPE").map(String::as_str), Some("Battery"));
        assert_eq!(b.get("NAME").map(String::as_str), Some("BAT0"));
        assert!(!b.contains_key("POWER_SUPPLY_TYPE"));
    }

    #[test]
    fn battery_figures_are_read_from_the_uevent() {
        let b = parse_uevent(BATTERY);
        assert_eq!(watts(&b), Some(8.4));
        assert!((health(&b).unwrap() - 92.307).abs() < 0.01);
        assert_eq!(num(&b, "CAPACITY"), Some(73.));
        assert_eq!(num(&b, "TEMP").map(|t| t / 10.), Some(31.2));
    }

    #[test]
    fn watts_fall_back_to_current_times_voltage() {
        let b =
            parse_uevent("POWER_SUPPLY_CURRENT_NOW=700000\nPOWER_SUPPLY_VOLTAGE_NOW=12000000\n");
        assert!((watts(&b).unwrap() - 8.4).abs() < 1e-9);
        assert_eq!(health(&b), None);
    }

    #[test]
    fn the_hottest_zone_is_reported() {
        let zones = [
            ("acpitz".to_string(), 48.),
            ("x86_pkg_temp".to_string(), 62.4),
        ];
        assert_eq!(thermal_summary(&zones), "62 °C");
        assert_eq!(thermal_summary(&[]), "No sensors");
    }
}
