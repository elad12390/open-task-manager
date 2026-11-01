//! The host's process table, read from inside a Flatpak sandbox. The sandbox's own `/proc` lists only
//! its processes, so one host command walks the host's `/proc` and prints a line per process; the
//! parsing and the CPU and disk rates (successive samples of the same counters) happen here.

use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
    time::Instant,
};

use super::host;
use crate::metrics::ProcessInfo;

/// One process per line. Fields are separated by U+001F and a cgroup's line breaks by U+001E. The
/// `U` line carries the host's uptime.
const SCRIPT: &str = r#"
{ find /proc -mindepth 2 -maxdepth 2 -name exe -printf 'X\t%h\t%l\n' 2>/dev/null
  for d in /proc/[0-9]*; do printf 'P\t%s\n' "$d"; done
} | awk '
function slurp(f, sep,   l, s) {
  s = ""
  while ((getline l < f) > 0) s = s (s == "" ? "" : sep) l
  close(f)
  return s
}
function keyed(f, key,   l) {
  l = ""
  while ((getline l < f) > 0) if (index(l, key) == 1) break
  close(f)
  return index(l, key) == 1 ? substr(l, length(key) + 1) : ""
}
function last_paren(s,   k) {
  for (k = length(s); k > 0; k--) if (substr(s, k, 1) == ")") return k
  return 0
}
function row(d,   st, p, comm, rest, f, n2, uid, a, n, ns, rss, rd, wr, c, cmdok, arg, args, cg, pid, kern, user) {
  st = slurp(d "/stat", "")
  if (st == "") return
  p = last_paren(st)
  if (p == 0) return
  comm = substr(st, index(st, "(") + 1, p - index(st, "(") - 1)
  rest = substr(st, p + 2)
  n2 = split(rest, f, " ")
  uid = split(keyed(d "/status", "Uid:"), a, " ") ? a[1] : ""
  n = split(keyed(d "/status", "NSpid:"), a, " ")
  ns = n >= 2 ? a[n] : 0
  rss = split(keyed(d "/status", "VmRSS:"), a, " ") ? a[1] : 0
  rd = keyed(d "/io", "read_bytes:") + 0
  wr = keyed(d "/io", "write_bytes:") + 0
  cmdok = (getline c < (d "/cmdline")) > 0
  close(d "/cmdline")
  gsub(/[^ -~]/, " ", c)
  args = split(c, arg, " ")
  pid = substr(d, 7)
  user = (uid in name) ? name[uid] : uid
  kern = (pid == 2 || (pid > 1 && !cmdok && exe[d] == "")) ? 1 : 0
  cg = slurp(d "/cgroup", "\036")
  print pid, f[2], user, f[1], comm, f[20], f[12], f[13], rss, rd, wr, exe[d], (args ? arg[1] : ""), cg, kern, ns
}
BEGIN {
  FS = "\t"
  OFS = "\037"
  while ((getline l < "/etc/passwd") > 0) {
    m = split(l, pw, ":")
    if (m >= 3) name[pw[3]] = pw[1]
  }
  close("/etc/passwd")
  if ((getline up < "/proc/uptime") > 0) {
    split(up, u, " ")
    print "U", u[1]
  }
  close("/proc/uptime")
}
$1 == "X" { exe[$2] = $3; next }
$1 == "P" { dirs[++count] = $2 }
END { for (i = 1; i <= count; i++) row(dirs[i]) }
'
"#;

const CLOCK_TICKS: f64 = 100.;

/// A host process as the table reads it, before the rates are worked out.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub pid: u32,
    pub ppid: u32,
    pub user: String,
    pub state: char,
    pub name: String,
    pub start: u64,
    pub ticks: u64,
    pub rss_kb: u64,
    pub read: u64,
    pub write: u64,
    pub exe: String,
    pub arg0: String,
    pub cgroup: String,
    pub kernel: bool,
    /// The process's PID inside its own sandbox, for the app's own process.
    pub ns: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Sample {
    pub uptime: f64,
    pub rows: Vec<Row>,
}

/// The cumulative counters of one process at one sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Counters {
    pub ticks: u64,
    pub read: u64,
    pub write: u64,
}

/// The per-process facts the classifier and the signals need, kept from the latest sample.
#[derive(Clone, Debug, Default)]
struct Meta {
    cgroup: String,
    kernel: bool,
}

struct Previous {
    at: Instant,
    counters: HashMap<u32, Counters>,
}

static PREVIOUS: Mutex<Option<Previous>> = Mutex::new(None);
static META: Mutex<Option<HashMap<u32, Meta>>> = Mutex::new(None);

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Parses the script's output. Lines that do not have every field are skipped.
pub fn parse(out: &str) -> Sample {
    let mut sample = Sample::default();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\u{1f}').collect();
        if f[0] == "U" {
            sample.uptime = f.get(1).and_then(|s| s.parse().ok()).unwrap_or(0.);
            continue;
        }
        if f.len() < 16 {
            continue;
        }
        let num = |s: &str| s.parse::<u64>().unwrap_or(0);
        let Ok(pid) = f[0].parse::<u32>() else {
            continue;
        };
        sample.rows.push(Row {
            pid,
            ppid: f[1].parse().unwrap_or(0),
            user: f[2].to_string(),
            state: f[3].chars().next().unwrap_or('?'),
            name: f[4].to_string(),
            start: num(f[5]),
            ticks: num(f[6]) + num(f[7]),
            rss_kb: num(f[8]),
            read: num(f[9]),
            write: num(f[10]),
            exe: f[11].to_string(),
            arg0: f[12].to_string(),
            cgroup: f[13].replace('\u{1e}', "\n"),
            kernel: f[14] == "1",
            ns: f[15].parse().ok().filter(|&n: &u32| n > 0),
        });
    }
    sample
}

/// The name a process state letter has in the process table (as sysinfo names them).
pub fn state_name(letter: char) -> &'static str {
    match letter {
        'R' => "Runnable",
        'S' => "Sleeping",
        'I' => "Idle",
        'D' => "UninterruptibleDiskSleep",
        'Z' => "Zombie",
        'T' => "Stopped",
        't' => "Tracing",
        'X' | 'x' => "Dead",
        'K' => "Wakekill",
        'W' => "Waking",
        'P' => "Parked",
        _ => "Unknown",
    }
}

pub fn counters(sample: &Sample) -> HashMap<u32, Counters> {
    sample
        .rows
        .iter()
        .map(|r| {
            (
                r.pid,
                Counters {
                    ticks: r.ticks,
                    read: r.read,
                    write: r.write,
                },
            )
        })
        .collect()
}

/// The process table: CPU as a share of one core and disk as bytes per second, from the change since
/// the previous sample `secs` ago. A process seen for the first time has no rate yet.
pub fn processes(
    sample: &Sample,
    previous: &HashMap<u32, Counters>,
    secs: f64,
    me: Option<u32>,
) -> Vec<ProcessInfo> {
    let secs = secs.max(0.05);
    sample
        .rows
        .iter()
        .map(|r| {
            let before = previous.get(&r.pid).copied().unwrap_or(Counters {
                ticks: r.ticks,
                read: r.read,
                write: r.write,
            });
            let cpu =
                (r.ticks.saturating_sub(before.ticks) as f64 / CLOCK_TICKS / secs * 100.) as f32;
            let per_sec = |now: u64, then: u64| (now.saturating_sub(then) as f64 / secs) as u64;
            let started = r.start as f64 / CLOCK_TICKS;
            ProcessInfo {
                pid: r.pid,
                ppid: Some(r.ppid).filter(|&p| p != r.pid),
                name: r.name.clone(),
                user: r.user.clone(),
                cpu: if previous.contains_key(&r.pid) {
                    cpu
                } else {
                    0.
                },
                memory: r.rss_kb * 1024,
                disk_read: per_sec(r.read, before.read),
                disk_write: per_sec(r.write, before.write),
                state: state_name(r.state).to_string(),
                run_time: (sample.uptime - started).max(0.) as u64,
                path: if r.exe.is_empty() && r.arg0.starts_with('/') {
                    r.arg0.clone()
                } else {
                    r.exe.clone()
                },
                is_self: me.is_some() && r.ns == me,
                owner_app: String::new(),
            }
        })
        .collect()
}

/// Samples the host's processes now. None when the host cannot be asked (the table then falls back to
/// what the sandbox sees).
pub fn sample() -> Option<Vec<ProcessInfo>> {
    let out = host::run(&["sh", "-c", SCRIPT]).ok()?;
    let sample = parse(&out);
    let now = Instant::now();
    let mut previous = locked(&PREVIOUS);
    let secs = previous
        .as_ref()
        .map_or(1., |p| now.duration_since(p.at).as_secs_f64());
    let before = previous.take().map(|p| p.counters).unwrap_or_default();
    let me = Some(std::process::id());
    let processes = processes(&sample, &before, secs, me);
    *locked(&META) = Some(
        sample
            .rows
            .iter()
            .map(|r| {
                (
                    r.pid,
                    Meta {
                        cgroup: r.cgroup.clone(),
                        kernel: r.kernel,
                    },
                )
            })
            .collect(),
    );
    *previous = Some(Previous {
        at: now,
        counters: counters(&sample),
    });
    Some(processes)
}

/// The cgroup text of a process from the latest sample.
pub fn cgroup(pid: u32) -> Option<String> {
    locked(&META).as_ref()?.get(&pid).map(|m| m.cgroup.clone())
}

/// Whether the latest sample names a process the kernel runs itself.
pub fn is_kernel(pid: u32) -> bool {
    locked(&META)
        .as_ref()
        .and_then(|m| m.get(&pid))
        .is_some_and(|m| m.kernel)
}

/// The process's PID inside the sandbox, which is the app's own PID when it is the app itself.
pub fn own_pid() -> Option<u32> {
    let me = std::process::id();
    let sample = parse(&host::run(&["sh", "-c", SCRIPT]).ok()?);
    sample.rows.iter().find(|r| r.ns == Some(me)).map(|r| r.pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIELDS: [&str; 16] = [
        "1",
        "0",
        "root",
        "S",
        "systemd",
        "10",
        "400",
        "100",
        "9000",
        "4096",
        "0",
        "/usr/lib/systemd/systemd",
        "/sbin/init",
        "0::/init.scope",
        "0",
        "",
    ];

    fn line(fields: &[&str]) -> String {
        fields.join("\u{1f}")
    }

    fn with(pid: &str, ppid: &str, ticks: (&str, &str), state: &str) -> String {
        let mut f = FIELDS.map(str::to_string);
        f[0] = pid.into();
        f[1] = ppid.into();
        f[3] = state.into();
        f[6] = ticks.0.into();
        f[7] = ticks.1.into();
        f.join("\u{1f}")
    }

    #[test]
    fn a_host_line_becomes_a_row() {
        let text = format!("U\u{1f}1000.5\n{}\n", line(&FIELDS));
        let sample = parse(&text);
        assert_eq!(sample.uptime, 1000.5);
        assert_eq!(sample.rows.len(), 1);
        let r = &sample.rows[0];
        assert_eq!((r.pid, r.ppid, r.state), (1, 0, 'S'));
        assert_eq!((r.ticks, r.rss_kb, r.start), (500, 9000, 10));
        assert_eq!(r.exe, "/usr/lib/systemd/systemd");
        assert_eq!(r.cgroup, "0::/init.scope");
        assert!(!r.kernel);
        assert_eq!(r.ns, None);
    }

    #[test]
    fn short_or_broken_lines_are_skipped() {
        let sample = parse("garbage\nP\u{1f}bad\n");
        assert!(sample.rows.is_empty());
    }

    #[test]
    fn cgroup_lines_are_restored_from_the_record_separator() {
        let mut f = FIELDS.map(str::to_string);
        f[13] = "1:cpu:/a\u{1e}0::/b".into();
        let sample = parse(&f.join("\u{1f}"));
        assert_eq!(sample.rows[0].cgroup, "1:cpu:/a\n0::/b");
    }

    #[test]
    fn cpu_is_the_tick_change_per_second_as_a_share_of_one_core() {
        let before = parse(&format!(
            "U\u{1f}100\n{}\n",
            with("42", "1", ("100", "0"), "R")
        ));
        let after = parse(&format!(
            "U\u{1f}102\n{}\n",
            with("42", "1", ("200", "100"), "R")
        ));
        let prev = counters(&before);
        let table = processes(&after, &prev, 2.0, None);
        assert_eq!(table.len(), 1);
        assert!((table[0].cpu - 100.0).abs() < 1e-3, "{}", table[0].cpu);
        assert_eq!(table[0].state, "Runnable");
        assert_eq!(table[0].memory, 9000 * 1024);
    }

    #[test]
    fn a_new_process_has_no_cpu_rate_yet() {
        let now = parse(&format!(
            "U\u{1f}100\n{}\n",
            with("7", "1", ("300", "0"), "S")
        ));
        let table = processes(&now, &HashMap::new(), 1.0, None);
        assert_eq!(table[0].cpu, 0.);
    }

    #[test]
    fn disk_rates_are_bytes_per_second_from_the_counter_change() {
        let mut f = FIELDS.map(str::to_string);
        f[9] = "10000".into();
        f[10] = "2000".into();
        let now = Sample {
            uptime: 100.,
            rows: parse(&f.join("\u{1f}")).rows,
        };
        let before = HashMap::from([(
            1,
            Counters {
                ticks: 500,
                read: 6000,
                write: 2000,
            },
        )]);
        let table = processes(&now, &before, 2.0, None);
        assert_eq!(table[0].disk_read, 2000);
        assert_eq!(table[0].disk_write, 0);
    }

    #[test]
    fn the_app_is_found_by_its_sandbox_pid_and_run_time_comes_from_the_boot_time() {
        let mut f = FIELDS.map(str::to_string);
        f[0] = "5555".into();
        f[15] = std::process::id().to_string();
        f[5] = "3000".into();
        let sample = Sample {
            uptime: 130.,
            rows: parse(&f.join("\u{1f}")).rows,
        };
        let table = processes(&sample, &HashMap::new(), 1.0, Some(std::process::id()));
        assert!(table[0].is_self);
        assert_eq!(table[0].run_time, 100);
        assert_eq!(table[0].ppid, Some(0));
    }

    #[test]
    fn state_letters_have_sysinfo_names() {
        assert_eq!(state_name('T'), "Stopped");
        assert_eq!(state_name('S'), "Sleeping");
        assert_eq!(state_name('?'), "Unknown");
    }
}
