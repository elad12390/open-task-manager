//! Holiday themes: which holiday a date falls in, and the palette and greeting it wears.
//!
//! Lunar and lunisolar dates come from the tables below. Sources for 2025–2035: timeanddate.com
//! public-holiday pages (israel, india, china), qppstudio.net (Umm al-Qura Ramadan and Eid dates,
//! used where timeanddate has none). Islamic days are the observed dates from timeanddate, which
//! can differ by a day from Umm al-Qura; every window is wide enough to absorb that.

use gpui::{Rgba, rgba};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum HolidayMode {
    #[default]
    Automatic,
    Off,
    Pinned(Holiday),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Holiday {
    NewYearsEve,
    NewYearsDay,
    LunarNewYear,
    ValentinesDay,
    StPatricksDay,
    Holi,
    Ramadan,
    EidAlFitr,
    Passover,
    Easter,
    EarthDay,
    PrideMonth,
    IndependenceDay,
    EidAlAdha,
    RoshHashanah,
    Halloween,
    DiaDeLosMuertos,
    Diwali,
    Thanksgiving,
    Hanukkah,
    Christmas,
}

/// A date as (year, month, day).
type Ymd = (i32, u32, u32);

/// Chinese New Year day (timeanddate.com/holidays/china, 2025–2035).
const LUNAR_NEW_YEAR: [Ymd; 11] = [
    (2025, 1, 29),
    (2026, 2, 17),
    (2027, 2, 6),
    (2028, 1, 26),
    (2029, 2, 13),
    (2030, 2, 3),
    (2031, 1, 23),
    (2032, 2, 11),
    (2033, 1, 31),
    (2034, 2, 19),
    (2035, 2, 8),
];

/// Ramadan start (first day) and the Eid al-Fitr day that ends it (timeanddate.com/holidays/israel;
/// 2035 Eid from qppstudio.net, Umm al-Qura).
const RAMADAN: [(Ymd, Ymd); 12] = [
    ((2025, 3, 1), (2025, 3, 30)),
    ((2026, 2, 18), (2026, 3, 20)),
    ((2027, 2, 8), (2027, 3, 10)),
    ((2028, 1, 28), (2028, 2, 27)),
    ((2029, 1, 16), (2029, 2, 15)),
    ((2030, 1, 6), (2030, 2, 5)),
    ((2030, 12, 26), (2031, 1, 25)),
    ((2031, 12, 15), (2032, 1, 14)),
    ((2032, 12, 4), (2033, 1, 3)),
    ((2033, 11, 23), (2033, 12, 23)),
    ((2034, 11, 12), (2034, 12, 12)),
    ((2035, 11, 1), (2035, 12, 1)),
];

/// Eid al-Adha day (timeanddate.com/holidays/israel; 2035 from qppstudio.net).
const EID_AL_ADHA: [Ymd; 11] = [
    (2025, 6, 6),
    (2026, 5, 27),
    (2027, 5, 17),
    (2028, 5, 5),
    (2029, 4, 24),
    (2030, 4, 14),
    (2031, 4, 3),
    (2032, 3, 22),
    (2033, 3, 12),
    (2034, 3, 1),
    (2035, 2, 18),
];

/// Passover, first day (timeanddate.com/holidays/israel).
const PASSOVER: [Ymd; 11] = [
    (2025, 4, 13),
    (2026, 4, 2),
    (2027, 4, 22),
    (2028, 4, 11),
    (2029, 3, 31),
    (2030, 4, 18),
    (2031, 4, 8),
    (2032, 3, 27),
    (2033, 4, 14),
    (2034, 4, 4),
    (2035, 4, 24),
];

/// Rosh Hashanah, first day (timeanddate.com/holidays/israel).
const ROSH_HASHANAH: [Ymd; 11] = [
    (2025, 9, 23),
    (2026, 9, 12),
    (2027, 10, 2),
    (2028, 9, 21),
    (2029, 9, 10),
    (2030, 9, 28),
    (2031, 9, 18),
    (2032, 9, 6),
    (2033, 9, 24),
    (2034, 9, 14),
    (2035, 10, 4),
];

/// Hanukkah, first day (timeanddate.com/holidays/israel).
const HANUKKAH: [Ymd; 11] = [
    (2025, 12, 15),
    (2026, 12, 5),
    (2027, 12, 25),
    (2028, 12, 13),
    (2029, 12, 2),
    (2030, 12, 21),
    (2031, 12, 10),
    (2032, 11, 28),
    (2033, 12, 17),
    (2034, 12, 7),
    (2035, 12, 26),
];

/// Diwali (timeanddate.com/holidays/india).
const DIWALI: [Ymd; 11] = [
    (2025, 10, 20),
    (2026, 11, 8),
    (2027, 10, 29),
    (2028, 10, 17),
    (2029, 11, 5),
    (2030, 10, 26),
    (2031, 11, 14),
    (2032, 11, 2),
    (2033, 10, 22),
    (2034, 11, 10),
    (2035, 10, 30),
];

/// Holi, the colour day (timeanddate.com/holidays/india).
const HOLI: [Ymd; 11] = [
    (2025, 3, 14),
    (2026, 3, 4),
    (2027, 3, 22),
    (2028, 3, 11),
    (2029, 3, 1),
    (2030, 3, 20),
    (2031, 3, 9),
    (2032, 3, 27),
    (2033, 3, 16),
    (2034, 3, 5),
    (2035, 3, 24),
];

impl Holiday {
    /// Declaration order breaks ties between windows of equal length.
    pub const ALL: [Holiday; 21] = [
        Holiday::NewYearsEve,
        Holiday::NewYearsDay,
        Holiday::LunarNewYear,
        Holiday::ValentinesDay,
        Holiday::StPatricksDay,
        Holiday::Holi,
        Holiday::Ramadan,
        Holiday::EidAlFitr,
        Holiday::Passover,
        Holiday::Easter,
        Holiday::EarthDay,
        Holiday::PrideMonth,
        Holiday::IndependenceDay,
        Holiday::EidAlAdha,
        Holiday::RoshHashanah,
        Holiday::Halloween,
        Holiday::DiaDeLosMuertos,
        Holiday::Diwali,
        Holiday::Thanksgiving,
        Holiday::Hanukkah,
        Holiday::Christmas,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Holiday::NewYearsEve => "New Year's Eve",
            Holiday::NewYearsDay => "New Year's Day",
            Holiday::LunarNewYear => "Lunar New Year",
            Holiday::ValentinesDay => "Valentine's Day",
            Holiday::StPatricksDay => "St Patrick's Day",
            Holiday::Holi => "Holi",
            Holiday::Ramadan => "Ramadan",
            Holiday::EidAlFitr => "Eid al-Fitr",
            Holiday::Passover => "Passover",
            Holiday::Easter => "Easter",
            Holiday::EarthDay => "Earth Day",
            Holiday::PrideMonth => "Pride Month",
            Holiday::IndependenceDay => "Independence Day",
            Holiday::EidAlAdha => "Eid al-Adha",
            Holiday::RoshHashanah => "Rosh Hashanah",
            Holiday::Halloween => "Halloween",
            Holiday::DiaDeLosMuertos => "Día de los Muertos",
            Holiday::Diwali => "Diwali",
            Holiday::Thanksgiving => "Thanksgiving",
            Holiday::Hanukkah => "Hanukkah",
            Holiday::Christmas => "Christmas",
        }
    }

    /// The plain-text line under the app name in the sidebar.
    pub fn greeting(self) -> &'static str {
        match self {
            Holiday::NewYearsEve => "Happy New Year's Eve",
            Holiday::NewYearsDay => "Happy New Year",
            Holiday::LunarNewYear => "Happy Lunar New Year",
            Holiday::ValentinesDay => "Happy Valentine's Day",
            Holiday::StPatricksDay => "Happy St Patrick's Day",
            Holiday::Holi => "Happy Holi",
            Holiday::Ramadan => "Ramadan Mubarak",
            Holiday::EidAlFitr => "Eid Mubarak",
            Holiday::Passover => "Happy Passover",
            Holiday::Easter => "Happy Easter",
            Holiday::EarthDay => "Happy Earth Day",
            Holiday::PrideMonth => "Happy Pride",
            Holiday::IndependenceDay => "Happy Independence Day",
            Holiday::EidAlAdha => "Eid Mubarak",
            Holiday::RoshHashanah => "Shanah Tovah",
            Holiday::Halloween => "Happy Halloween",
            Holiday::DiaDeLosMuertos => "Feliz Día de Muertos",
            Holiday::Diwali => "Happy Diwali",
            Holiday::Thanksgiving => "Happy Thanksgiving",
            Holiday::Hanukkah => "Happy Hanukkah",
            Holiday::Christmas => "Merry Christmas",
        }
    }

    pub fn palette(self) -> Palette {
        let (light, dark, fill, tint, band) = match self {
            Holiday::NewYearsEve => (0x8a5c00, 0xffd060, 0x6b4a00, 0xffc83d, [0xffd060, 0xff8a3d]),
            Holiday::NewYearsDay => (0x1f4fa8, 0x8ab4ff, 0x1b3f8c, 0x3b6bd6, [0x8ab4ff, 0xc9a7ff]),
            Holiday::LunarNewYear => (0xb3111a, 0xff8a80, 0x8a0c14, 0xe0202a, [0xffd34d, 0xe0202a]),
            Holiday::ValentinesDay => {
                (0xa3125a, 0xff8fc1, 0x8c0f48, 0xff5c8a, [0xff8fc1, 0xff5c7a])
            }
            Holiday::StPatricksDay => {
                (0x137a3a, 0x6ee08a, 0x0b5a2a, 0x2ea84f, [0x6ee08a, 0x1f9d4c])
            }
            Holiday::Holi => (0xb3420b, 0xffb36b, 0x7a2d05, 0xff7a1a, [0xff4fa0, 0xffb020]),
            Holiday::Ramadan => (0x0b6b5e, 0x7fe0cf, 0x0a4f46, 0x14a38f, [0xd4af37, 0x1f8a7a]),
            Holiday::EidAlFitr => (0x6b3fa0, 0xc9a8ff, 0x4b2578, 0x8a5cd6, [0xd4af37, 0x8a5cd6]),
            Holiday::Passover => (0x1e3a8a, 0x9db8ff, 0x15296a, 0x3b5fd0, [0xe8c872, 0x3b5fd0]),
            Holiday::Easter => (0x6a2c91, 0xe0b3ff, 0x4a1d66, 0xd68cff, [0xffe066, 0x7fd8ff]),
            Holiday::EarthDay => (0x0f5f3a, 0x7dd3a0, 0x0a4a2b, 0x2ca65a, [0x3fa9f5, 0x5cc46b]),
            Holiday::PrideMonth => (0x8a1a8a, 0xffb3ff, 0x5c0f5c, 0xff3b8c, [0xff5a4d, 0x4d8dff]),
            Holiday::IndependenceDay => {
                (0x1b3f94, 0x9fc0ff, 0x132c6b, 0xb22234, [0xb22234, 0x3c3b6e])
            }
            Holiday::EidAlAdha => (0x1c5f8f, 0x8fcfff, 0x164a73, 0x2f8fd6, [0xd4af37, 0x2f8fd6]),
            Holiday::RoshHashanah => (0x7a4a00, 0xffcf7a, 0x5a3500, 0xffb84d, [0xffd27a, 0xe07b39]),
            Holiday::Halloween => (0xa84300, 0xff9a4d, 0x6e2c00, 0xff7a18, [0xff7a18, 0x6b2fa3]),
            Holiday::DiaDeLosMuertos => {
                (0xa31b5f, 0xffd166, 0x6e0f44, 0xff9f1c, [0xffb703, 0xe63976])
            }
            Holiday::Diwali => (0x9a5b00, 0xffd166, 0x6b3f00, 0xffb300, [0xffb000, 0xff5a1f]),
            Holiday::Thanksgiving => (0x8a4b10, 0xffb866, 0x5c2f08, 0xc46a1b, [0xd98b2b, 0x7a3e12]),
            Holiday::Hanukkah => (0x1d4ed8, 0x93c5fd, 0x163b9a, 0x3a6fd8, [0xe5e7eb, 0x1d4ed8]),
            Holiday::Christmas => (0xb0141f, 0xff8585, 0x7a0c14, 0x1f7a3a, [0xd4262e, 0x1e8f4a]),
        };
        Palette {
            accent_light: solid(light),
            accent_dark: solid(dark),
            fill: solid(fill),
            tint: wash(tint),
            band: band.map(solid),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub accent_light: Rgba,
    pub accent_dark: Rgba,
    /// Behind white text, like `Theme::selection`.
    pub fill: Rgba,
    /// A low-alpha wash mixed into the window tint and glass.
    pub tint: Rgba,
    pub band: [Rgba; 2],
}

impl Palette {
    pub fn accent(&self, dark: bool) -> Rgba {
        if dark {
            self.accent_dark
        } else {
            self.accent_light
        }
    }
}

fn solid(hex: u32) -> Rgba {
    rgba((hex << 8) | 0xff)
}

fn wash(hex: u32) -> Rgba {
    rgba((hex << 8) | 0x14)
}

/// The Gregorian day count since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days(year: i32, month: u32, day: u32) -> i64 {
    let y = i64::from(if month <= 2 { year - 1 } else { year });
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// 0 is Sunday. 1970-01-01 was a Thursday.
fn weekday(day: i64) -> i64 {
    (day + 4).rem_euclid(7)
}

/// Fourth Thursday of November.
fn thanksgiving(year: i32) -> i64 {
    let first = days(year, 11, 1);
    let first_thursday = first + (4 - weekday(first)).rem_euclid(7);
    first_thursday + 21
}

/// Anonymous Gregorian computus.
fn easter(year: i32) -> i64 {
    let (a, b, c) = (year % 19, year / 100, year % 100);
    let (d, e) = (b / 4, b % 4);
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15).rem_euclid(30);
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k).rem_euclid(7);
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    days(year, month as u32, day as u32)
}

/// The day-count windows a holiday shows on, for the holidays that start in `year`.
fn spans(holiday: Holiday, year: i32) -> Vec<(i64, i64)> {
    let fixed = |month, day| days(year, month, day);
    let table = |t: &[Ymd], span: fn(i64) -> (i64, i64)| -> Vec<(i64, i64)> {
        t.iter()
            .filter(|(y, _, _)| *y == year)
            .map(|&(y, m, d)| span(days(y, m, d)))
            .collect()
    };
    match holiday {
        Holiday::NewYearsEve => vec![(fixed(12, 31), fixed(12, 31))],
        Holiday::NewYearsDay => vec![(fixed(1, 1) - 1, fixed(1, 1) + 1)],
        Holiday::LunarNewYear => table(&LUNAR_NEW_YEAR, |c| (c, c + 14)),
        Holiday::ValentinesDay => vec![(fixed(2, 12), fixed(2, 14))],
        Holiday::StPatricksDay => vec![(fixed(3, 15), fixed(3, 17))],
        Holiday::Holi => table(&HOLI, |c| (c - 1, c + 1)),
        Holiday::Ramadan => RAMADAN
            .iter()
            .filter(|(start, _)| start.0 == year)
            .map(|&(start, end)| {
                (
                    days(start.0, start.1, start.2),
                    days(end.0, end.1, end.2) - 1,
                )
            })
            .collect(),
        Holiday::EidAlFitr => RAMADAN
            .iter()
            .filter(|(_, end)| end.0 == year)
            .map(|&(_, end)| {
                let c = days(end.0, end.1, end.2);
                (c - 1, c + 2)
            })
            .collect(),
        Holiday::Passover => table(&PASSOVER, |c| (c, c + 7)),
        Holiday::Easter => {
            let c = easter(year);
            vec![(c - 2, c + 1)]
        }
        Holiday::EarthDay => vec![(fixed(4, 21), fixed(4, 23))],
        Holiday::PrideMonth => vec![(fixed(6, 1), fixed(6, 30))],
        Holiday::IndependenceDay => vec![(fixed(7, 3), fixed(7, 5))],
        Holiday::EidAlAdha => table(&EID_AL_ADHA, |c| (c, c + 3)),
        Holiday::RoshHashanah => table(&ROSH_HASHANAH, |c| (c, c + 1)),
        Holiday::Halloween => vec![(fixed(10, 24), fixed(10, 31))],
        Holiday::DiaDeLosMuertos => vec![(fixed(10, 31), fixed(11, 2))],
        Holiday::Diwali => table(&DIWALI, |c| (c - 3, c + 3)),
        Holiday::Thanksgiving => {
            let c = thanksgiving(year);
            vec![(c - 2, c + 3)]
        }
        Holiday::Hanukkah => table(&HANUKKAH, |c| (c - 1, c + 7)),
        Holiday::Christmas => vec![(fixed(12, 18), fixed(12, 26))],
    }
}

/// The holiday shown on a calendar date. When windows overlap, the shorter one wins.
pub fn active_on(year: i32, month: u32, day: u32) -> Option<Holiday> {
    let today = days(year, month, day);
    let mut best: Option<(i64, Holiday)> = None;
    for holiday in Holiday::ALL {
        for y in year - 1..=year + 1 {
            for (start, end) in spans(holiday, y) {
                if (start..=end).contains(&today) && best.is_none_or(|(len, _)| end - start < len) {
                    best = Some((end - start, holiday));
                }
            }
        }
    }
    best.map(|(_, holiday)| holiday)
}

/// The holiday to show for a setting, using the local calendar date for `Automatic`.
pub fn current(mode: HolidayMode) -> Option<Holiday> {
    match mode {
        HolidayMode::Automatic => {
            let (year, month, day) = today();
            active_on(year, month, day)
        }
        HolidayMode::Off => None,
        HolidayMode::Pinned(holiday) => Some(holiday),
    }
}

/// The local calendar date.
pub fn today() -> (i32, u32, u32) {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&now, &mut tm) };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}
