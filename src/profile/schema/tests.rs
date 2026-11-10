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

use x52::input::Mode;
use x52::state::Leds;

use super::*;
use crate::profile::tests::fixture;

fn text(text: &str) -> MfdLineSource {
    MfdLineSource::Text(text.to_owned())
}

fn readable(name: &str) -> (Profile, Vec<FieldProblem>) {
    match parse(&fixture(name), Schema::CURRENT) {
        Ok(Parsed::Readable {
            profile,
            problems,
            migrated_from: None,
        }) => (*profile, problems),
        other => panic!("{name}: {other:?}"),
    }
}

fn problem(key: &str, kind: ProblemKind) -> FieldProblem {
    FieldProblem {
        key: key.to_owned(),
        kind,
    }
}

fn keys(problems: &[FieldProblem]) -> Vec<&str> {
    problems
        .iter()
        .map(|problem| problem.key.as_str())
        .collect()
}

#[test]
fn the_profiles_md_example_reads_completely() {
    let (profile, problems) = readable("valid-x52-pro.toml");
    assert_eq!(problems, []);
    assert_eq!(profile.device_type, DeviceType::X52Pro);
    assert_eq!(profile.name, "Night flying");
    assert_eq!(profile.kind(), ProfileKind::Single);
    let settings = profile.settings_for(Mode::One);
    assert_eq!(settings.led_brightness.level(), 64);
    assert_eq!(settings.mfd_brightness.level(), 32);
    assert_eq!(profile.date_format, DateFormat::DdMmYy);
    assert_eq!(settings.leds.colour(BiColourLed::ButtonD), LedColour::Red);
    assert_eq!(settings.leds.colour(BiColourLed::T1T2), LedColour::Amber);
    assert_eq!(settings.leds.colour(BiColourLed::ButtonI), LedColour::Off);
    assert!(settings.leds.is_on(SingleColourLed::Fire));
    assert_eq!(
        settings.mfd_lines,
        [
            text("GX52"),
            MfdLineSource::LastButton,
            MfdLineSource::ProfileName
        ]
    );
    assert_eq!(profile.clock1.source, Clock1Source::Local);
    assert_eq!(
        profile.clock2.source,
        ZoneSpec::Iana(IanaZone::new("Europe/Rome").unwrap())
    );
    assert_eq!(
        profile.clock3.source,
        ZoneSpec::Fixed(FixedOffset::new(345).unwrap())
    );
    assert!(!profile.clock3.h24);
}

#[test]
fn an_x52_profile_reads_without_leds_shift_or_blink() {
    let (profile, problems) = readable("valid-x52.toml");
    assert_eq!(problems, []);
    assert_eq!(profile.device_type, DeviceType::X52);
    assert_eq!(profile.clock1.source, Clock1Source::Utc);
    assert_eq!(profile.settings_for(Mode::One).mfd_brightness.level(), 0);
}

#[test]
fn the_per_mode_example_reads_one_set_per_mode() {
    let (profile, problems) = readable("valid-x52-pro-per-mode.toml");
    assert_eq!(problems, []);
    assert_eq!(profile.name, "DCS World");
    assert_eq!(profile.kind(), ProfileKind::PerMode);
    let levels = Mode::ALL.map(|mode| profile.settings_for(mode).led_brightness.level());
    assert_eq!(levels, [64, 96, 128]);
    let button_a =
        Mode::ALL.map(|mode| profile.settings_for(mode).leds.colour(BiColourLed::ButtonA));
    assert_eq!(
        button_a,
        [LedColour::Green, LedColour::Red, LedColour::Amber]
    );
    assert_eq!(
        profile.settings_for(Mode::Two).mfd_lines,
        [MfdLineSource::Mode, MfdLineSource::Empty, text("Ready")]
    );
    assert_eq!(
        profile.settings_for(Mode::Three).mfd_lines,
        [const { MfdLineSource::Empty }; MFD_LINES]
    );
}

#[test]
fn writing_gives_the_layout_of_profiles_md() {
    for name in [
        "valid-x52-pro.toml",
        "valid-x52-pro-per-mode.toml",
        "valid-x52.toml",
    ] {
        let (profile, _) = readable(name);
        assert_eq!(
            write(&profile),
            String::from_utf8(fixture(name)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn written_profiles_read_back_unchanged() {
    let mut odd = Profile::builtin(DeviceType::X52Pro, "\"Quoted\" \\ 'name' — 飛行".to_owned());
    let mut settings = ModeSettings::builtin(DeviceType::X52Pro);
    settings.mfd_lines = [text("a\"b"), text("\\"), text("ÄÖÜ")];
    settings.leds = Leds::uniform(LedColour::Amber, false);
    odd.settings = ModeSets::Single(settings.clone());
    odd.clock2.source = ZoneSpec::Iana(IanaZone::new("America/Santiago").unwrap());
    let mut per_mode = odd.clone();
    let mut mode_3 = settings.clone();
    mode_3.mfd_lines = [
        MfdLineSource::Mode,
        MfdLineSource::LastButton,
        MfdLineSource::ProfileName,
    ];
    mode_3.shift = true;
    per_mode.settings = ModeSets::PerMode(Box::new([
        settings,
        ModeSettings::builtin(DeviceType::X52Pro),
        mode_3,
    ]));
    let mut x52_per_mode = Profile::builtin_default(DeviceType::X52);
    x52_per_mode.settings = x52_per_mode.settings.into_per_mode();
    for profile in [
        Profile::builtin_default(DeviceType::X52Pro),
        Profile::builtin_default(DeviceType::X52),
        odd,
        per_mode,
        x52_per_mode,
    ] {
        let written = write(&profile);
        match parse(written.as_bytes(), Schema::CURRENT) {
            Ok(Parsed::Readable {
                profile: read,
                problems,
                migrated_from: None,
            }) => {
                assert_eq!(problems, [], "{written}");
                assert_eq!(*read, profile, "{written}");
            }
            other => panic!("{written}: {other:?}"),
        }
    }
}

#[test]
fn documents_that_are_not_profiles_are_refused() {
    let cases = [
        ("not-utf8.bin", ParseError::NotUtf8),
        (
            "missing-schema-version.toml",
            ParseError::MissingSchemaVersion,
        ),
        (
            "wrong-schema-version.toml",
            ParseError::WrongSchemaVersion("\"one\"".to_owned()),
        ),
        (
            "zero-schema-version.toml",
            ParseError::WrongSchemaVersion("0".to_owned()),
        ),
        ("missing-device-type.toml", ParseError::MissingDeviceType),
        (
            "unknown-device-type.toml",
            ParseError::UnknownDeviceType("x56".to_owned()),
        ),
    ];
    for (name, expected) in cases {
        assert_eq!(
            parse(&fixture(name), Schema::CURRENT),
            Err(expected),
            "{name}"
        );
    }
    assert!(matches!(
        parse(&fixture("malformed.txt"), Schema::CURRENT),
        Err(ParseError::Malformed(_))
    ));
}

#[test]
fn out_of_range_fields_get_the_builtin_value() {
    let (profile, problems) = readable("out-of-range.toml");
    let builtin = Profile::builtin(DeviceType::X52Pro, fallback_name());
    assert_eq!(
        keys(&problems),
        [
            "name",
            "clock_3.source",
            "settings.led_brightness",
            "settings.mfd_brightness",
            "settings.mfd.lines[1].text",
        ]
    );
    assert_eq!(
        problems[0],
        problem("name", ProblemKind::Name(NameProblem::Empty))
    );
    assert_eq!(
        problems[3],
        problem("settings.mfd_brightness", ProblemKind::Missing)
    );
    assert_eq!(profile.name, builtin.name);
    let settings = profile.settings_for(Mode::One);
    let builtin_settings = builtin.settings_for(Mode::One);
    assert_eq!(settings.led_brightness, builtin_settings.led_brightness);
    assert_eq!(settings.mfd_brightness, builtin_settings.mfd_brightness);
    assert_eq!(
        settings.mfd_lines,
        [text("GX52"), MfdLineSource::Empty, text("Night flying")]
    );
    assert_eq!(profile.clock3.source, builtin.clock3.source);
    // The rest of the profile loads.
    assert_eq!(
        profile.clock2.source,
        ZoneSpec::Iana(IanaZone::new("Europe/Rome").unwrap())
    );
    assert_eq!(settings.leds.colour(BiColourLed::ButtonD), LedColour::Red);
}

#[test]
fn an_mfd_without_three_lines_gets_the_builtin_lines() {
    let (profile, problems) = readable("two-mfd-lines.toml");
    assert_eq!(keys(&problems), ["settings.mfd.lines"]);
    assert_eq!(
        profile.settings_for(Mode::One).mfd_lines,
        ModeSettings::builtin(DeviceType::X52Pro).mfd_lines
    );
}

#[test]
fn unknown_values_get_the_builtin_value() {
    let (profile, problems) = readable("unknown-values.toml");
    let builtin = Profile::builtin(DeviceType::X52Pro, fallback_name());
    assert_eq!(
        keys(&problems),
        [
            "date_format",
            "clock_1.source",
            "clock_2.source",
            "clock_3.source",
            "settings.leds.a",
        ]
    );
    assert!(
        problems
            .iter()
            .all(|problem| matches!(problem.kind, ProblemKind::UnknownValue(_)))
    );
    assert_eq!(profile.date_format, builtin.date_format);
    assert_eq!(
        profile
            .settings_for(Mode::One)
            .leds
            .colour(BiColourLed::ButtonA),
        LedColour::Green
    );
    assert_eq!(profile.clock1.source, builtin.clock1.source);
    assert_eq!(profile.clock2.source, builtin.clock2.source);
    assert_eq!(profile.clock3.source, builtin.clock3.source);
    assert_eq!(profile.name, "Night flying");
}

#[test]
fn unknown_keys_and_keys_of_the_other_type_are_ignored_and_reported() {
    let (profile, problems) = readable("unknown-keys-x52.toml");
    assert_eq!(
        problems,
        [
            problem("colour_scheme", ProblemKind::UnknownKey),
            problem("settings.shift", ProblemKind::NotForDeviceType),
            problem("settings.blink", ProblemKind::NotForDeviceType),
            problem("settings.leds", ProblemKind::NotForDeviceType),
            problem("settings.mfd.scroll", ProblemKind::UnknownKey),
        ]
    );
    assert!(!profile.settings_for(Mode::One).shift);
    assert_eq!(profile.name, "Plain");
}

#[test]
fn an_unknown_kind_is_read_as_single() {
    let (profile, problems) = readable("unknown-kind.toml");
    assert_eq!(
        problems,
        [problem(
            "kind",
            ProblemKind::UnknownValue("\"triple\"".to_owned())
        )]
    );
    assert_eq!(profile.kind(), ProfileKind::Single);
    assert_eq!(profile.settings_for(Mode::One).led_brightness.level(), 64);
}

#[test]
fn a_missing_kind_is_read_as_single() {
    let contents = String::from_utf8(fixture("valid-x52-pro.toml"))
        .unwrap()
        .replace("kind = \"single\"\n", "");
    let Ok(Parsed::Readable {
        profile, problems, ..
    }) = parse(contents.as_bytes(), Schema::CURRENT)
    else {
        panic!("{contents}");
    };
    assert_eq!(problems, [problem("kind", ProblemKind::Missing)]);
    assert_eq!(profile.kind(), ProfileKind::Single);
    assert_eq!(profile.settings_for(Mode::One).led_brightness.level(), 64);
}

#[test]
fn a_missing_mode_table_gets_the_builtin_settings() {
    let (profile, problems) = readable("missing-mode-table.toml");
    assert_eq!(problems, [problem("mode_2", ProblemKind::Missing)]);
    assert_eq!(profile.kind(), ProfileKind::PerMode);
    assert_eq!(
        profile.settings_for(Mode::Two),
        &ModeSettings::builtin(DeviceType::X52Pro)
    );
    assert_eq!(
        profile.settings_for(Mode::Three).led_brightness.level(),
        128
    );
    assert_eq!(profile.settings_for(Mode::One).led_brightness.level(), 64);
}

#[test]
fn tables_of_the_other_kind_are_ignored_and_reported() {
    let (profile, problems) = readable("other-kind-tables.toml");
    assert_eq!(problems, [problem("mode_2", ProblemKind::UnknownKey)]);
    assert_eq!(profile.kind(), ProfileKind::Single);
    assert_eq!(profile.settings_for(Mode::Two).led_brightness.level(), 64);

    let per_mode = String::from_utf8(fixture("valid-x52-pro-per-mode.toml")).unwrap()
        + "\n[settings]\nled_brightness = 1\n";
    let Ok(Parsed::Readable { problems, .. }) = parse(per_mode.as_bytes(), Schema::CURRENT) else {
        panic!("{per_mode}");
    };
    assert_eq!(problems, [problem("settings", ProblemKind::UnknownKey)]);
}

#[test]
fn an_unknown_line_source_gives_an_empty_line() {
    let (profile, problems) = readable("unknown-line-source.toml");
    assert_eq!(
        problems,
        [
            problem(
                "settings.mfd.lines[1].source",
                ProblemKind::UnknownValue("\"clock\"".to_owned())
            ),
            problem("settings.mfd.lines[2].text", ProblemKind::UnknownKey),
        ]
    );
    assert_eq!(
        profile.settings_for(Mode::One).mfd_lines,
        [text("GX52"), MfdLineSource::Empty, MfdLineSource::Mode]
    );
}

#[test]
fn text_lines_are_checked_on_the_converted_bytes() {
    let (profile, problems) = readable("line-text-too-long.toml");
    assert_eq!(keys(&problems), ["settings.mfd.lines[1].text"]);
    assert!(matches!(problems[0].kind, ProblemKind::OutOfRange(_)));
    assert_eq!(
        profile.settings_for(Mode::One).mfd_lines,
        [
            text("ABCDEFGHIJKLMNOP"),
            MfdLineSource::Empty,
            text("ÄÖÜäöüéèêëàâçñ¿¡")
        ]
    );
}

#[test]
fn a_text_line_without_text_gives_an_empty_line() {
    let (profile, problems) = readable("line-text-missing.toml");
    assert_eq!(
        problems,
        [
            problem("settings.mfd.lines[0].text", ProblemKind::Missing),
            problem(
                "settings.mfd.lines[1]",
                ProblemKind::WrongType {
                    expected: "a table"
                }
            ),
        ]
    );
    assert_eq!(
        profile.settings_for(Mode::One).mfd_lines,
        [const { MfdLineSource::Empty }; MFD_LINES]
    );
}

#[test]
fn a_newer_schema_is_only_listed() {
    assert_eq!(
        parse(&fixture("newer-schema.toml"), Schema::CURRENT),
        Ok(Parsed::Newer {
            version: 99,
            device_type: DeviceType::X52Pro,
            name: Some("Night flying".to_owned()),
        })
    );
}

#[test]
fn an_older_schema_is_migrated_in_memory() {
    fn noop(_: &mut Table) {}
    let schema = Schema::new(2, &[noop]);
    match parse(&fixture("valid-x52-pro.toml"), schema) {
        Ok(Parsed::Readable {
            profile,
            problems,
            migrated_from,
        }) => {
            assert_eq!(migrated_from, Some(1));
            assert_eq!(problems, []);
            assert_eq!(profile.name, "Night flying");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn problems_have_a_message_for_the_user() {
    let (_, problems) = readable("out-of-range.toml");
    let messages: Vec<String> = problems.iter().map(FieldProblem::message).collect();
    assert_eq!(
        messages[0],
        "The name is empty or too long; a default name is used"
    );
    assert_eq!(
        messages[1],
        "clock_3.source is out of range; the default is used"
    );
    assert_eq!(
        messages[3],
        "settings.mfd_brightness is missing; the default is used"
    );
}

#[test]
fn a_damaged_file_names_its_device_type_when_it_is_toml() {
    for (name, expected) in [
        ("missing-schema-version.toml", Some(DeviceType::X52Pro)),
        ("wrong-schema-version.toml", Some(DeviceType::X52Pro)),
        ("valid-x52.toml", Some(DeviceType::X52)),
        ("malformed.txt", None),
        ("not-utf8.bin", None),
        ("missing-device-type.toml", None),
        ("unknown-device-type.toml", None),
    ] {
        assert_eq!(sniff_device_type(&fixture(name)), expected, "{name}");
    }
}
