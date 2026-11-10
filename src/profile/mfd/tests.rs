// Copyright 2026 Roberto Leinardi.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The text budget, the preview, the line sources and the drafts of the MFD editor.

use jiff::tz;
use x52::charset::MfdText;
use x52::clock::{Clock1Source, DateFormat, FixedOffset, ZoneSource};

use super::*;

const FULL: &str = "ABCDEFGHIJKLMNOP";

/// The line `inserted` typed at `at` (a character offset) of `text` makes.
fn typed(text: &str, at: usize, inserted: &str) -> String {
    let split = text
        .char_indices()
        .nth(at)
        .map_or(text.len(), |(index, _)| index);
    let (before, after) = text.split_at(split);
    format!(
        "{before}{}{after}",
        accepted_insert(before, inserted, after)
    )
}

#[test]
fn a_full_line_accepts_nothing_and_keeps_its_text() {
    for at in [0, 8, 16] {
        assert_eq!(typed(FULL, at, "X"), FULL, "typed at {at}");
    }
}

#[test]
fn typing_fills_the_line_and_stops_at_16() {
    assert_eq!(typed("ABC", 3, "D"), "ABCD");
    assert_eq!(typed("ABCDEFGHIJKLMNO", 15, "P"), FULL);
    assert_eq!(typed("ABCDEFGHIJKLMNO", 0, "x"), "xABCDEFGHIJKLMNO");
}

#[test]
fn a_paste_is_cut_to_the_room_left() {
    assert_eq!(typed("ABCDEFGHIJKL", 12, "1234567890"), "ABCDEFGHIJKL1234");
    assert_eq!(typed("ABCDEFGHIJKL", 4, "1234567890"), "ABCD1234EFGHIJKL");
    // Characters without a glyph still take one byte each.
    assert_eq!(typed("", 0, "€€€€€€€€€€€€€€€€€€"), "€".repeat(16));
}

#[test]
fn a_selection_replaced_by_longer_text_keeps_the_rest() {
    // "EFGH" selected in a full line and deleted: GTK then inserts the replacement.
    let before = "ABCD";
    let after = "IJKLMNOP";
    assert_eq!(accepted_insert(before, "123456", after), "1234");
}

#[test]
fn a_combining_accent_is_accepted_on_a_full_line() {
    // e followed by a combining acute composes to é, one byte: the line stays at 16.
    let line = "ABCDEFGHIJKLMNOe";
    assert_eq!(accepted_insert(line, "\u{301}", ""), "\u{301}");
    assert_eq!(charset::encode(&typed(line, 16, "\u{301}")).len(), 16);
}

#[test]
fn a_character_that_composes_with_the_text_after_it_is_accepted() {
    // A Hangul vowel jamo has no glyph and takes a byte; a leading consonant typed before it
    // composes with it into one syllable, so the line stays at 16.
    let line = format!("\u{1161}{}", &FULL[1..]);
    assert_eq!(charset::encode(&line).len(), 16);
    assert_eq!(accepted_insert("", "\u{1100}", &line), "\u{1100}");
    assert_eq!(accepted_insert("", "x", &line), "");
}

#[test]
fn text_info_counts_converted_bytes_and_lists_substitutions() {
    let info = TextInfo::of("Éjszakai repülő €");
    assert_eq!(info.count, 17);
    assert_eq!(info.substitutions, "ő → o, € → ?");
    let info = TextInfo::of("e\u{301}");
    assert_eq!(info.count, 1);
    assert_eq!(info.counter(), "1/16");
    assert_eq!(info.counter_label(), "1 of 16 characters");
    assert_eq!(info.substitutions, "");
}

#[test]
fn sources_round_trip_in_combo_order() {
    assert_eq!(
        LineSource::ALL.map(LineSource::label),
        [
            "Custom text",
            "Profile name",
            "Mode",
            "Last button",
            "Empty"
        ]
    );
    for source in LineSource::ALL {
        let line = source.to_line(|| "GX52".to_owned());
        assert_eq!(LineSource::of(&line), source);
    }
    assert_eq!(
        LineSource::CustomText.to_line(|| "Fire".to_owned()),
        MfdLineSource::Text("Fire".to_owned())
    );
}

/// 2026-11-09 at 12:45 UTC.
fn noon() -> Timestamp {
    "2026-11-09T12:45:00Z".parse().unwrap()
}

/// The built-in X52 Pro state with `lines`.
fn state(lines: [&str; MFD_LINES]) -> DeviceState {
    let mut state = DeviceState::default_for(DeviceType::X52Pro);
    state.mfd_lines = lines.map(|line| MfdText::new(line).unwrap());
    state.clock1.source = Clock1Source::Utc;
    state.date_format = DateFormat::DdMmYy;
    state
}

fn shown(state: &DeviceState, clock: ShownClock) -> MfdPreview {
    preview(state, DeviceType::X52Pro, clock, noon(), &TimeZone::UTC).unwrap()
}

#[test]
fn a_character_without_a_glyph_previews_as_its_fallback() {
    let preview = shown(&state(["repülő €", "", "Fire"]), ShownClock::One);
    assert_eq!(preview.lines[0], "repülo ?        ");
    assert_eq!(preview.lines[1], " ".repeat(16));
    assert_eq!(preview.lines[2], "Fire            ");
    assert!(preview.lines.iter().all(|line| line.chars().count() == 16));
}

#[test]
fn clock_1_shows_24_h_and_12_h() {
    let mut state = state(["", "", ""]);
    assert_eq!(shown(&state, ShownClock::One).clock, "12:45");
    state.clock1.h24 = false;
    assert_eq!(shown(&state, ShownClock::One).clock, "12:45 PM");
    state.clock1.source = Clock1Source::Local;
    let midnight = preview(
        &state,
        DeviceType::X52Pro,
        ShownClock::One,
        noon(),
        &TimeZone::fixed(tz::offset(11)),
    )
    .unwrap();
    assert_eq!(midnight.clock, "11:45 PM");
    let morning = preview(
        &state,
        DeviceType::X52Pro,
        ShownClock::One,
        noon(),
        &TimeZone::fixed(tz::offset(-12)),
    )
    .unwrap();
    assert_eq!(morning.clock, "12:45 AM");
    let noon_hour = preview(
        &state,
        DeviceType::X52Pro,
        ShownClock::One,
        "2026-11-09T12:00:00Z".parse().unwrap(),
        &TimeZone::UTC,
    )
    .unwrap();
    assert_eq!(noon_hour.clock, "12:00 PM");
    assert_eq!(morning.date, "09.11.26");
}

#[test]
fn clocks_2_and_3_show_clock_1_plus_their_offset() {
    let mut state = state(["", "", ""]);
    // A zone with daylight saving time: New York is at UTC−05:00 in November.
    state.clock2.source = ZoneSource::Zone(TimeZone::posix("EST5EDT,M3.2.0,M11.1.0").unwrap());
    // A fixed offset that crosses midnight: 12:45 UTC is 00:15 the next day at UTC+11:30.
    state.clock3.source = ZoneSource::Fixed(FixedOffset::new(690).unwrap());
    let two = shown(&state, ShownClock::Two);
    assert_eq!((two.clock.as_str(), two.clock_tag), ("07:45", Some(2)));
    let three = shown(&state, ShownClock::Three);
    assert_eq!((three.clock.as_str(), three.clock_tag), ("00:15", Some(3)));
    // The date is always clock 1's.
    assert_eq!(three.date, "09.11.26");
    state.clock3.h24 = false;
    assert_eq!(shown(&state, ShownClock::Three).clock, "12:15 AM");
    assert_eq!(shown(&state, ShownClock::One).clock_tag, None);
}

#[test]
fn the_date_follows_the_format_with_dots() {
    let mut state = state(["", "", ""]);
    for (format, date) in [
        (DateFormat::DdMmYy, "09.11.26"),
        (DateFormat::MmDdYy, "11.09.26"),
        (DateFormat::YyMmDd, "26.11.09"),
    ] {
        state.date_format = format;
        assert_eq!(shown(&state, ShownClock::One).date, date, "{format:?}");
    }
}

#[test]
fn shift_shows_only_where_the_device_has_it() {
    let mut state = state(["", "", ""]);
    state.shift = true;
    assert!(shown(&state, ShownClock::One).shift);
    let x52 = preview(
        &state,
        DeviceType::X52,
        ShownClock::One,
        noon(),
        &TimeZone::UTC,
    )
    .unwrap();
    assert!(!x52.shift);
}

#[test]
fn the_backlight_has_four_steps() {
    for (level, step) in [
        (0, Backlight::Off),
        (1, Backlight::Low),
        (40, Backlight::Low),
        (41, Backlight::Medium),
        (90, Backlight::Medium),
        (91, Backlight::High),
        (128, Backlight::High),
    ] {
        assert_eq!(
            Backlight::of(Brightness::new(level).unwrap()),
            step,
            "{level}"
        );
    }
    let mut state = state(["", "", ""]);
    state.mfd_brightness = Brightness::new(0).unwrap();
    assert_eq!(shown(&state, ShownClock::One).backlight, Backlight::Off);
}

#[test]
fn the_label_reads_the_lines_the_clock_and_the_date() {
    let state = state(["GX52", "Fire", ""]);
    assert_eq!(
        accessible_label(&shown(&state, ShownClock::One)),
        "Simulated MFD: GX52, Fire, blank; clock 1 12:45, date 09.11.26"
    );
    assert!(accessible_label(&shown(&state, ShownClock::Three)).contains("; clock 3 "));
}

fn key(profile: &str, set: SettingsSet, line: usize) -> DraftKey {
    DraftKey {
        profile: ProfileId::parse(profile).unwrap(),
        set,
        line,
    }
}

#[test]
fn a_draft_comes_back_only_in_its_profile_set_and_line() {
    let mut drafts = TextDrafts::default();
    drafts.leave(key("aaa", SettingsSet::Single, 0), "Fire".to_owned());
    assert_eq!(drafts.resume(&key("bbb", SettingsSet::Single, 0)), "");
    assert_eq!(drafts.resume(&key("aaa", SettingsSet::Single, 1)), "");
    assert_eq!(
        drafts.resume(&key("aaa", SettingsSet::Mode(Mode::One), 0)),
        ""
    );
    assert_eq!(drafts.resume(&key("aaa", SettingsSet::Single, 0)), "Fire");
}

#[test]
fn a_mode_draft_stays_in_its_mode() {
    let mut drafts = TextDrafts::default();
    let one = key("aaa", SettingsSet::Mode(Mode::One), 2);
    let two = key("aaa", SettingsSet::Mode(Mode::Two), 2);
    drafts.leave(one.clone(), "Landing".to_owned());
    assert_eq!(drafts.resume(&two), "");
    assert_eq!(drafts.resume(&one), "Landing");
    // Taken back: the text is in the profile again, and leaving keeps the new text.
    assert_eq!(drafts.resume(&one), "");
}

#[test]
fn the_set_follows_the_profile_kind() {
    assert_eq!(
        SettingsSet::of(ProfileKind::Single, Mode::Two),
        SettingsSet::Single
    );
    assert_eq!(
        SettingsSet::of(ProfileKind::PerMode, Mode::Two),
        SettingsSet::Mode(Mode::Two)
    );
}

#[test]
fn each_backlight_step_has_its_own_class() {
    let classes: Vec<&str> = Backlight::ALL.map(Backlight::css_class).to_vec();
    assert_eq!(
        classes,
        ["backlight-0", "backlight-1", "backlight-2", "backlight-3"]
    );
}

#[test]
fn the_clocks_are_numbered_from_1() {
    assert_eq!(ShownClock::ALL.map(ShownClock::number), [1, 2, 3]);
    assert_eq!(ShownClock::default(), ShownClock::One);
}
