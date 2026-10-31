//! Process filters: free text plus chips (app, user, category, state, CPU, memory), and the
//! autocomplete suggestions offered while typing.
//!
//! Chips of the same kind widen the filter (Chrome *or* Firefox); chips of different kinds narrow
//! it (Chrome *and* using more than 5% CPU). Free text matches name, owning app, user and PID.

use std::collections::{HashMap, HashSet};

use crate::metrics::{ProcessInfo, fmt_bytes};

const MB: u64 = 1024 * 1024;
const GB: u64 = 1024 * MB;

#[derive(Clone, Debug, PartialEq)]
pub enum Chip {
    App(String),
    User(String),
    Category(String),
    State(String),
    CpuAbove(f32),
    MemoryAbove(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    App,
    User,
    Category,
    State,
    Cpu,
    Memory,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::App => "App",
            Kind::User => "User",
            Kind::Category => "Category",
            Kind::State => "State",
            Kind::Cpu => "CPU",
            Kind::Memory => "Memory",
        }
    }
}

impl Chip {
    pub fn kind(&self) -> Kind {
        match self {
            Chip::App(_) => Kind::App,
            Chip::User(_) => Kind::User,
            Chip::Category(_) => Kind::Category,
            Chip::State(_) => Kind::State,
            Chip::CpuAbove(_) => Kind::Cpu,
            Chip::MemoryAbove(_) => Kind::Memory,
        }
    }

    pub fn value(&self) -> String {
        match self {
            Chip::App(s) | Chip::User(s) | Chip::Category(s) | Chip::State(s) => s.clone(),
            Chip::CpuAbove(x) => format!("> {x}%"),
            Chip::MemoryAbove(b) => format!("> {}", fmt_bytes(*b)),
        }
    }

    fn matches(&self, p: &ProcessInfo, categories: &HashMap<u32, (&'static str, String)>) -> bool {
        match self {
            Chip::App(a) => &p.owner_app == a,
            Chip::User(u) => &p.user == u,
            Chip::Category(c) => categories.get(&p.pid).is_some_and(|(_, s)| s == c),
            Chip::State(s) => &p.state == s,
            Chip::CpuAbove(x) => p.cpu > *x,
            Chip::MemoryAbove(b) => p.memory > *b,
        }
    }
}

/// A ready-to-use filter over one snapshot.
pub struct Filter<'a> {
    chips: &'a [Chip],
    text: String,
    categories: HashMap<u32, (&'static str, String)>,
}

impl<'a> Filter<'a> {
    pub fn new(chips: &'a [Chip], query: &str, processes: &[ProcessInfo]) -> Self {
        let categories = if chips.iter().any(|c| c.kind() == Kind::Category) {
            crate::categories::categorize(processes)
        } else {
            HashMap::new()
        };
        Self {
            chips,
            text: query.trim().to_lowercase(),
            categories,
        }
    }

    pub fn is_active(&self) -> bool {
        !self.chips.is_empty() || !self.text.is_empty()
    }

    pub fn matches(&self, p: &ProcessInfo) -> bool {
        let text_ok = self.text.is_empty()
            || p.name.to_lowercase().contains(&self.text)
            || p.owner_app.to_lowercase().contains(&self.text)
            || p.user.to_lowercase().contains(&self.text)
            || p.pid.to_string().starts_with(&self.text);
        if !text_ok {
            return false;
        }
        let mut kinds: Vec<Kind> = self.chips.iter().map(Chip::kind).collect();
        kinds.dedup();
        let kinds: HashSet<Kind> = kinds.into_iter().collect();
        kinds.into_iter().all(|k| {
            self.chips
                .iter()
                .filter(|c| c.kind() == k)
                .any(|c| c.matches(p, &self.categories))
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Suggestion {
    pub chip: Chip,
    /// How many processes the chip alone would match.
    pub count: usize,
}

/// `cpu>5`, `cpu 5`, `mem>500`, `mem > 1.5g`, `memory>200mb`: a numeric chip, if `text` is one.
pub fn parse_numeric(text: &str) -> Option<Chip> {
    let t = text.trim().to_lowercase();
    let (kind, rest) = if let Some(r) = t.strip_prefix("cpu") {
        (Kind::Cpu, r)
    } else if let Some(r) = t.strip_prefix("memory").or_else(|| t.strip_prefix("mem")) {
        (Kind::Memory, r)
    } else {
        return None;
    };
    let rest = rest.trim_start_matches([' ', '>', ':', '=']).trim();
    let digits: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let n: f64 = digits.parse().ok()?;
    let unit = rest[digits.len()..].trim().trim_end_matches('b');
    Some(match kind {
        Kind::Cpu => Chip::CpuAbove(n as f32),
        _ => Chip::MemoryAbove(match unit {
            "g" => (n * GB as f64) as u64,
            "k" => (n * 1024.) as u64,
            _ => (n * MB as f64) as u64,
        }),
    })
}

fn presets(kind: Kind) -> Vec<Chip> {
    match kind {
        Kind::Cpu => vec![Chip::CpuAbove(1.), Chip::CpuAbove(5.), Chip::CpuAbove(25.)],
        _ => vec![
            Chip::MemoryAbove(100 * MB),
            Chip::MemoryAbove(500 * MB),
            Chip::MemoryAbove(GB),
        ],
    }
}

/// Suggestions for what has been typed so far, best first. `app:`, `user:`, `cat:` and `state:`
/// restrict the kind; `cpu`/`mem` offer thresholds. With nothing typed, offers the busiest apps.
pub fn suggest(text: &str, chosen: &[Chip], processes: &[ProcessInfo]) -> Vec<Suggestion> {
    const LIMIT: usize = 8;
    let t = text.trim().to_lowercase();
    let count = |chip: &Chip, categories: &HashMap<u32, (&'static str, String)>| {
        processes
            .iter()
            .filter(|p| chip.matches(p, categories))
            .count()
    };
    let none = HashMap::new();
    let fresh = |c: &Chip| !chosen.contains(c);

    // Numbers: an exact threshold, or presets while only the word is typed.
    if let Some(chip) = parse_numeric(&t) {
        return vec![Suggestion {
            count: count(&chip, &none),
            chip,
        }]
        .into_iter()
        .filter(|s| fresh(&s.chip))
        .collect();
    }
    let numeric_word =
        |w: &str| !t.is_empty() && w.starts_with(t.trim_end_matches(['>', ' ', ':']));
    let mut out: Vec<Suggestion> = Vec::new();
    for (word, kind) in [("cpu", Kind::Cpu), ("memory", Kind::Memory)] {
        if numeric_word(word) {
            out.extend(
                presets(kind)
                    .into_iter()
                    .filter(fresh)
                    .map(|chip| Suggestion {
                        count: count(&chip, &none),
                        chip,
                    }),
            );
        }
    }

    let (only, needle) = match t.split_once(':') {
        Some(("app", n)) => (Some(Kind::App), n.trim()),
        Some(("user", n)) => (Some(Kind::User), n.trim()),
        Some(("cat" | "category", n)) => (Some(Kind::Category), n.trim()),
        Some(("state", n)) => (Some(Kind::State), n.trim()),
        _ => (None, t.as_str()),
    };
    let wants = |k: Kind| only.is_none_or(|o| o == k);

    // Every value of every kind, with its process count and total memory for ranking.
    let mut values: HashMap<(Kind, String), (usize, u64)> = HashMap::new();
    let categories = if wants(Kind::Category) {
        crate::categories::categorize(processes)
    } else {
        HashMap::new()
    };
    for p in processes {
        let mut add = |k: Kind, v: &str| {
            if !v.is_empty() && wants(k) {
                let e = values.entry((k, v.to_string())).or_default();
                e.0 += 1;
                e.1 += p.memory;
            }
        };
        add(Kind::App, &p.owner_app);
        add(Kind::User, &p.user);
        add(Kind::State, &p.state);
        if let Some((_, section)) = categories.get(&p.pid) {
            add(Kind::Category, section);
        }
    }
    let apps: HashSet<String> = values
        .keys()
        .filter(|(k, _)| *k == Kind::App)
        .map(|(_, v)| v.clone())
        .collect();
    let mut ranked: Vec<(u8, u64, Suggestion)> = values
        .into_iter()
        // A category named after an app adds nothing over the app itself.
        .filter(|((k, v), _)| *k != Kind::Category || !apps.contains(v))
        .filter_map(|((k, v), (n, memory))| {
            let lower = v.to_lowercase();
            let rank = if needle.is_empty() {
                // Nothing typed: the heaviest apps first, then everything else.
                if k == Kind::App { 1 } else { 3 }
            } else if lower.starts_with(needle) {
                0
            } else if lower
                .split([' ', '.', '-', '_', '&'])
                .any(|w| w.starts_with(needle))
            {
                1
            } else if lower.contains(needle) {
                2
            } else {
                return None;
            };
            let chip = match k {
                Kind::App => Chip::App(v),
                Kind::User => Chip::User(v),
                Kind::Category => Chip::Category(v),
                _ => Chip::State(v),
            };
            fresh(&chip).then_some((rank, memory, Suggestion { chip, count: n }))
        })
        .collect();
    ranked.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    out.extend(ranked.into_iter().map(|(_, _, s)| s));
    out.truncate(LIMIT);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, name: &str, app: &str, user: &str, cpu: f32, mem_mb: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            name: name.into(),
            owner_app: app.into(),
            user: user.into(),
            cpu,
            memory: mem_mb * MB,
            state: "Sleeping".into(),
            ..Default::default()
        }
    }

    fn sample() -> Vec<ProcessInfo> {
        vec![
            proc(10, "Google Chrome", "Google Chrome", "alex", 6.0, 500),
            proc(
                11,
                "Google Chrome Helper",
                "Google Chrome",
                "alex",
                1.0,
                300,
            ),
            proc(20, "firefox", "Firefox", "alex", 3.0, 600),
            proc(
                30,
                "WindowServer",
                "Operating System",
                "_windowserver",
                9.0,
                600,
            ),
            proc(31, "logd", "Operating System", "root", 0.1, 20),
        ]
    }

    #[test]
    fn chips_of_one_kind_widen_and_of_different_kinds_narrow() {
        let ps = sample();
        let chips = [
            Chip::App("Google Chrome".into()),
            Chip::App("Firefox".into()),
        ];
        let f = Filter::new(&chips, "", &ps);
        assert_eq!(ps.iter().filter(|p| f.matches(p)).count(), 3);

        let chips = [Chip::App("Google Chrome".into()), Chip::CpuAbove(5.)];
        let f = Filter::new(&chips, "", &ps);
        let hits: Vec<u32> = ps.iter().filter(|p| f.matches(p)).map(|p| p.pid).collect();
        assert_eq!(hits, vec![10]);
    }

    #[test]
    fn free_text_matches_the_owning_app() {
        let ps = sample();
        let f = Filter::new(&[], "operating", &ps);
        assert_eq!(ps.iter().filter(|p| f.matches(p)).count(), 2);
    }

    #[test]
    fn numeric_filters_parse_units() {
        assert_eq!(parse_numeric("cpu>5"), Some(Chip::CpuAbove(5.)));
        assert_eq!(parse_numeric("CPU 12.5"), Some(Chip::CpuAbove(12.5)));
        assert_eq!(parse_numeric("mem>500"), Some(Chip::MemoryAbove(500 * MB)));
        assert_eq!(
            parse_numeric("memory > 1.5gb"),
            Some(Chip::MemoryAbove(GB + GB / 2))
        );
        assert_eq!(parse_numeric("cpu"), None);
        assert_eq!(parse_numeric("chrome"), None);
    }

    #[test]
    fn suggestions_rank_prefix_matches_first_and_skip_chosen_chips() {
        let ps = sample();
        let s = suggest("goo", &[], &ps);
        assert_eq!(s[0].chip, Chip::App("Google Chrome".into()));
        assert_eq!(s[0].count, 2);

        let s = suggest("user:ro", &[], &ps);
        assert_eq!(
            s.iter().map(|s| &s.chip).collect::<Vec<_>>(),
            vec![&Chip::User("root".into())]
        );

        let chosen = [Chip::App("Google Chrome".into())];
        assert!(
            suggest("goo", &chosen, &ps)
                .iter()
                .all(|s| s.chip != chosen[0])
        );
    }

    #[test]
    fn typing_cpu_offers_thresholds() {
        let ps = sample();
        let s = suggest("cpu", &[], &ps);
        assert_eq!(s[0].chip, Chip::CpuAbove(1.));
        assert_eq!(s[1].count, 2); // > 5%: Chrome and WindowServer
        let s = suggest("cpu>5", &[], &ps);
        assert_eq!(s.len(), 1);
    }
}
