//! Holiday themes: calendar windows, overlap rules, palette contrast, settings and the picker.

use gpui::{Rgba, TestAppContext, WindowAppearance, rgba};

use crate::{
    app::{OtmApp, Page},
    holidays::{Holiday, HolidayMode, active_on, current},
    settings::{Appearance, Settings},
    theme::Theme,
    ui_tests::{ENV_LOCK, click},
};

fn holiday(year: i32, month: u32, day: u32) -> Option<Holiday> {
    active_on(year, month, day)
}

fn luminance(c: Rgba) -> f32 {
    let lin = |v: f32| {
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG 2.x contrast ratio.
fn contrast(a: Rgba, b: Rgba) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn opaque(c: Rgba) -> Rgba {
    Rgba { a: 1., ..c }
}

fn white() -> Rgba {
    rgba(0xffffffff)
}

#[test]
fn fixed_holidays_show_on_their_days() {
    assert_eq!(holiday(2026, 1, 1), Some(Holiday::NewYearsDay));
    assert_eq!(holiday(2026, 2, 14), Some(Holiday::ValentinesDay));
    assert_eq!(holiday(2026, 3, 17), Some(Holiday::StPatricksDay));
    assert_eq!(holiday(2026, 4, 22), Some(Holiday::EarthDay));
    assert_eq!(holiday(2026, 6, 15), Some(Holiday::PrideMonth));
    assert_eq!(holiday(2026, 7, 4), Some(Holiday::IndependenceDay));
    assert_eq!(holiday(2026, 12, 25), Some(Holiday::Christmas));
    assert_eq!(holiday(2026, 12, 31), Some(Holiday::NewYearsEve));
}

#[test]
fn new_years_eve_is_only_the_last_day() {
    assert_eq!(holiday(2026, 12, 31), Some(Holiday::NewYearsEve));
    assert_eq!(holiday(2026, 12, 30), None);
    assert_eq!(holiday(2026, 1, 2), Some(Holiday::NewYearsDay));
    assert_eq!(holiday(2026, 1, 3), None);
}

#[test]
fn christmas_window_is_dec_18_to_26() {
    assert_eq!(holiday(2026, 12, 17), None);
    assert_eq!(holiday(2026, 12, 18), Some(Holiday::Christmas));
    assert_eq!(holiday(2026, 12, 26), Some(Holiday::Christmas));
    assert_eq!(holiday(2026, 12, 27), None);
}

#[test]
fn halloween_window_is_oct_24_to_31_and_dia_de_muertos_takes_the_last_day() {
    assert_eq!(holiday(2026, 10, 23), None);
    assert_eq!(holiday(2026, 10, 24), Some(Holiday::Halloween));
    assert_eq!(holiday(2026, 10, 30), Some(Holiday::Halloween));
    assert_eq!(holiday(2026, 10, 31), Some(Holiday::DiaDeLosMuertos));
    assert_eq!(holiday(2026, 11, 1), Some(Holiday::DiaDeLosMuertos));
    assert_eq!(holiday(2026, 11, 2), Some(Holiday::DiaDeLosMuertos));
    assert_eq!(holiday(2026, 11, 3), None);
}

#[test]
fn thanksgiving_is_the_fourth_thursday_of_november() {
    assert_eq!(holiday(2026, 11, 26), Some(Holiday::Thanksgiving));
    assert_eq!(holiday(2026, 11, 23), None);
    assert_eq!(holiday(2026, 11, 24), Some(Holiday::Thanksgiving));
    assert_eq!(holiday(2026, 11, 29), Some(Holiday::Thanksgiving));
    assert_eq!(holiday(2026, 11, 30), None);
    assert_eq!(holiday(2027, 11, 25), Some(Holiday::Thanksgiving));
    assert_eq!(holiday(2035, 11, 22), Some(Holiday::Thanksgiving));
}

#[test]
fn easter_follows_the_computus() {
    assert_eq!(holiday(2026, 4, 5), Some(Holiday::Easter));
    assert_eq!(holiday(2026, 4, 3), Some(Holiday::Easter));
    assert_eq!(holiday(2026, 4, 6), Some(Holiday::Easter));
    assert_eq!(holiday(2027, 3, 28), Some(Holiday::Easter));
    assert_eq!(holiday(2027, 3, 25), None);
    assert_eq!(holiday(2027, 3, 29), Some(Holiday::Easter));
    assert_eq!(holiday(2027, 3, 30), None);
}

#[test]
fn lunar_new_year_2026_starts_feb_17() {
    assert_eq!(holiday(2026, 2, 16), None);
    assert_eq!(holiday(2026, 2, 17), Some(Holiday::LunarNewYear));
    assert_eq!(holiday(2026, 2, 26), Some(Holiday::LunarNewYear));
    assert_eq!(holiday(2026, 3, 3), Some(Holiday::Holi));
}

#[test]
fn table_dates_for_2026() {
    assert_eq!(holiday(2026, 3, 4), Some(Holiday::Holi));
    assert_eq!(holiday(2026, 3, 20), Some(Holiday::EidAlFitr));
    assert_eq!(holiday(2026, 4, 2), Some(Holiday::Passover));
    assert_eq!(holiday(2026, 5, 27), Some(Holiday::EidAlAdha));
    assert_eq!(holiday(2026, 9, 12), Some(Holiday::RoshHashanah));
    assert_eq!(holiday(2026, 11, 8), Some(Holiday::Diwali));
    assert_eq!(holiday(2026, 12, 5), Some(Holiday::Hanukkah));
}

#[test]
fn the_table_covers_every_year_from_2025_to_2035() {
    let days = [
        (2025, 10, 20, Holiday::Diwali),
        (2027, 10, 29, Holiday::Diwali),
        (2030, 10, 26, Holiday::Diwali),
        (2035, 10, 30, Holiday::Diwali),
        (2025, 3, 14, Holiday::Holi),
        (2031, 3, 9, Holiday::Holi),
        (2029, 4, 24, Holiday::EidAlAdha),
        (2033, 3, 12, Holiday::EidAlAdha),
        (2028, 4, 11, Holiday::Passover),
        (2035, 4, 24, Holiday::Passover),
        (2029, 9, 10, Holiday::RoshHashanah),
        (2035, 10, 4, Holiday::RoshHashanah),
        (2032, 12, 1, Holiday::Hanukkah),
        (2034, 12, 7, Holiday::Hanukkah),
        (2034, 2, 19, Holiday::LunarNewYear),
        (2031, 1, 23, Holiday::LunarNewYear),
        (2028, 2, 26, Holiday::EidAlFitr),
        (2030, 2, 5, Holiday::EidAlFitr),
    ];
    for (y, m, d, expected) in days {
        assert_eq!(holiday(y, m, d), Some(expected), "{y}-{m:02}-{d:02}");
    }
}

#[test]
fn diwali_window_is_three_days_either_side() {
    assert_eq!(holiday(2026, 11, 4), None);
    assert_eq!(holiday(2026, 11, 5), Some(Holiday::Diwali));
    assert_eq!(holiday(2026, 11, 11), Some(Holiday::Diwali));
    assert_eq!(holiday(2026, 11, 12), None);
}

#[test]
fn hanukkah_lasts_eight_nights() {
    assert_eq!(holiday(2026, 12, 4), Some(Holiday::Hanukkah));
    assert_eq!(holiday(2026, 12, 12), Some(Holiday::Hanukkah));
    assert_eq!(holiday(2026, 12, 13), None);
}

#[test]
fn ramadan_covers_the_whole_month() {
    assert_eq!(holiday(2029, 1, 20), Some(Holiday::Ramadan));
    assert_eq!(holiday(2029, 2, 10), Some(Holiday::Ramadan));
    assert_eq!(holiday(2029, 1, 15), None);
    assert_eq!(holiday(2026, 3, 10), Some(Holiday::Ramadan));
}

#[test]
fn pride_is_all_of_june() {
    assert_eq!(holiday(2026, 5, 31), None);
    assert_eq!(holiday(2026, 6, 1), Some(Holiday::PrideMonth));
    assert_eq!(holiday(2026, 6, 30), Some(Holiday::PrideMonth));
    assert_eq!(holiday(2026, 7, 1), None);
}

#[test]
fn shorter_window_wins_when_windows_overlap() {
    // Holi (3 days) inside Ramadan and Lunar New Year.
    assert_eq!(holiday(2026, 3, 4), Some(Holiday::Holi));
    assert_ne!(holiday(2026, 3, 6), Some(Holiday::Holi));
    // Eid al-Fitr (4 days) inside Ramadan (29 days).
    assert_eq!(holiday(2026, 3, 19), Some(Holiday::EidAlFitr));
    // St Patrick's (3 days) inside Ramadan.
    assert_eq!(holiday(2026, 3, 17), Some(Holiday::StPatricksDay));
    // Ramadan starts Feb 18, but Lunar New Year (15 days) is shorter than Ramadan (29 days).
    assert_eq!(holiday(2026, 2, 18), Some(Holiday::LunarNewYear));
    // Día de los Muertos (3 days) beats Halloween (8 days) on Oct 31.
    assert_eq!(holiday(2026, 10, 31), Some(Holiday::DiaDeLosMuertos));
}

#[test]
fn equal_windows_go_to_the_earlier_holiday_in_the_list() {
    // Dec 26 2030: Hanukkah (Dec 20–28) and Christmas (Dec 18–26) are both 8 days long, and
    // Hanukkah comes first. Ramadan starts the same day but is longer than both.
    assert_eq!(holiday(2030, 12, 26), Some(Holiday::Hanukkah));
    assert_eq!(holiday(2030, 12, 29), Some(Holiday::Ramadan));
}

#[test]
fn pinned_and_off_modes_ignore_the_calendar() {
    assert_eq!(current(HolidayMode::Off), None);
    assert_eq!(
        current(HolidayMode::Pinned(Holiday::Diwali)),
        Some(Holiday::Diwali)
    );
}

#[test]
fn every_holiday_has_a_greeting_and_a_name() {
    for h in Holiday::ALL {
        assert!(!h.name().is_empty());
        assert!(h.greeting().starts_with(|c: char| c.is_ascii_uppercase()));
    }
    assert_eq!(Holiday::ALL.len(), 21);
}

#[test]
fn every_holiday_palette_meets_contrast_in_light_and_dark() {
    for h in Holiday::ALL {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let settings = Settings {
                appearance,
                holiday_theme: HolidayMode::Pinned(h),
                ..Default::default()
            };
            let t = Theme::resolve(WindowAppearance::Light, &settings);
            assert_eq!(t.holiday, Some(h));
            for (name, layer) in [
                ("glass", t.glass),
                ("glass_strong", t.glass_strong),
                ("window_tint", t.window_tint),
            ] {
                let ratio = contrast(t.accent, opaque(layer));
                assert!(
                    ratio >= 4.5,
                    "{} {appearance:?}: accent text on {name} is {ratio:.2}:1",
                    h.name()
                );
            }
            let ratio = contrast(white(), t.selection);
            assert!(
                ratio >= 4.5,
                "{} {appearance:?}: white text on the selection fill is {ratio:.2}:1",
                h.name()
            );
        }
    }
}

#[test]
fn base_selection_fill_meets_contrast_in_light_and_dark() {
    for appearance in [Appearance::Light, Appearance::Dark] {
        let settings = Settings {
            appearance,
            holiday_theme: HolidayMode::Off,
            ..Default::default()
        };
        let t = Theme::resolve(WindowAppearance::Light, &settings);
        assert_eq!(t.holiday, None);
        let ratio = contrast(white(), t.selection);
        assert!(ratio >= 4.5, "{appearance:?} selection is {ratio:.2}:1");
    }
}

#[test]
fn holiday_accent_is_set_per_appearance() {
    let h = Holiday::Diwali;
    let light = Theme::resolve(
        WindowAppearance::Light,
        &Settings {
            appearance: Appearance::Light,
            holiday_theme: HolidayMode::Pinned(h),
            ..Default::default()
        },
    );
    let dark = Theme::resolve(
        WindowAppearance::Dark,
        &Settings {
            appearance: Appearance::Dark,
            holiday_theme: HolidayMode::Pinned(h),
            ..Default::default()
        },
    );
    assert_eq!(light.accent, h.palette().accent_light);
    assert_eq!(dark.accent, h.palette().accent_dark);
    assert_ne!(light.accent, dark.accent);
}

#[test]
fn holiday_mode_round_trips_through_settings_json() {
    for mode in [
        HolidayMode::Automatic,
        HolidayMode::Off,
        HolidayMode::Pinned(Holiday::Christmas),
    ] {
        let settings = Settings {
            holiday_theme: mode,
            ..Default::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.holiday_theme, mode);
    }
}

#[test]
fn settings_file_without_the_holiday_field_loads_as_automatic() {
    let old = r#"{"appearance":"Dark","glass":"Tinted","refresh_secs":2,"mark_self":false,"text_scale":115,"high_contrast":true,"popout_skin":"Glass"}"#;
    let settings: Settings = serde_json::from_str(old).unwrap();
    assert_eq!(settings.holiday_theme, HolidayMode::Automatic);
    assert_eq!(settings.appearance, Appearance::Dark);
    assert_eq!(
        (
            settings.refresh_secs,
            settings.text_scale,
            settings.high_contrast
        ),
        (2, 115, true)
    );
}

/// The picker pins a holiday, shows its greeting in the sidebar, and Off removes it.
#[gpui::test]
fn holiday_picker_previews_and_turns_off(cx: &mut TestAppContext) {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("otm-holiday-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: single-threaded test setup before any app code reads the variable.
    unsafe { std::env::set_var("OTM_DATA_DIR", &dir) };

    let (view, cx) = cx.add_window_view(OtmApp::new);
    cx.run_until_parked();
    view.update(cx, |app, cx| {
        app.open_page(Page::Settings, cx);
        cx.notify();
    });

    click(cx, "holiday-Diwali");
    assert_eq!(
        view.read_with(cx, |app, _| app.settings.holiday_theme),
        HolidayMode::Pinned(Holiday::Diwali)
    );
    assert_eq!(
        Settings::load().holiday_theme,
        HolidayMode::Pinned(Holiday::Diwali)
    );
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |app, _| {
            Theme::resolve(WindowAppearance::Light, &app.settings).holiday
        }),
        Some(Holiday::Diwali)
    );
    assert!(
        cx.debug_bounds("sidebar-greeting-Diwali").is_some(),
        "the sidebar shows the Diwali greeting while Diwali is pinned"
    );

    click(cx, "holiday-Off");
    assert_eq!(
        view.read_with(cx, |app, _| app.settings.holiday_theme),
        HolidayMode::Off
    );
    assert_eq!(
        view.read_with(cx, |app, _| {
            Theme::resolve(WindowAppearance::Light, &app.settings).holiday
        }),
        None,
        "Off removes the holiday from the theme, so the sidebar draws no greeting"
    );

    click(cx, "holiday-Automatic");
    assert_eq!(
        view.read_with(cx, |app, _| app.settings.holiday_theme),
        HolidayMode::Automatic
    );
    std::fs::remove_dir_all(&dir).ok();
}
