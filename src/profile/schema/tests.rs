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

use x52::state::Leds;

use super::*;
use crate::profile::tests::fixture;

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
    assert_eq!(profile.led_brightness.level(), 64);
    assert_eq!(profile.mfd_brightness.level(), 32);
    assert_eq!(profile.date_format, DateFormat::DdMmYy);
    assert_eq!(profile.leds.colour(BiColourLed::ButtonD), LedColour::Red);
    assert_eq!(profile.leds.colour(BiColourLed::T1T2), LedColour::Amber);
    assert_eq!(profile.leds.colour(BiColourLed::ButtonI), LedColour::Off);
    assert!(profile.leds.is_on(SingleColourLed::Fire));
    assert_eq!(
        profile.mfd_lines,
        ["GX52", "", "Night flying"].map(str::to_owned)
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
    assert_eq!(profile.mfd_brightness.level(), 0);
}

#[test]
fn writing_gives_the_layout_of_profiles_md() {
    for name in ["valid-x52-pro.toml", "valid-x52.toml"] {
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
    odd.mfd_lines = ["a\"b".to_owned(), "\\".to_owned(), "ÄÖÜ".to_owned()];
    odd.leds = Leds::uniform(LedColour::Amber, false);
    odd.clock2.source = ZoneSpec::Iana(IanaZone::new("America/Santiago").unwrap());
    for profile in [
        Profile::builtin_default(DeviceType::X52Pro),
        Profile::builtin_default(DeviceType::X52),
        odd,
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
            "led_brightness",
            "mfd_brightness",
            "mfd.lines[1]",
            "clock_3.source"
        ]
    );
    assert_eq!(
        problems[0],
        problem("name", ProblemKind::Name(NameProblem::Empty))
    );
    assert_eq!(problems[2], problem("mfd_brightness", ProblemKind::Missing));
    assert_eq!(profile.name, builtin.name);
    assert_eq!(profile.led_brightness, builtin.led_brightness);
    assert_eq!(profile.mfd_brightness, builtin.mfd_brightness);
    assert_eq!(
        profile.mfd_lines,
        ["GX52", "", "Night flying"].map(str::to_owned)
    );
    assert_eq!(profile.clock3.source, builtin.clock3.source);
    // The rest of the profile loads.
    assert_eq!(
        profile.clock2.source,
        ZoneSpec::Iana(IanaZone::new("Europe/Rome").unwrap())
    );
    assert_eq!(profile.leds.colour(BiColourLed::ButtonD), LedColour::Red);
}

#[test]
fn an_mfd_without_three_lines_gets_the_builtin_lines() {
    let (profile, problems) = readable("two-mfd-lines.toml");
    assert_eq!(keys(&problems), ["mfd.lines"]);
    assert_eq!(
        profile.mfd_lines,
        Profile::builtin_default(DeviceType::X52Pro).mfd_lines
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
            "leds.a",
            "clock_1.source",
            "clock_2.source",
            "clock_3.source"
        ]
    );
    assert!(
        problems
            .iter()
            .all(|problem| matches!(problem.kind, ProblemKind::UnknownValue(_)))
    );
    assert_eq!(profile.date_format, builtin.date_format);
    assert_eq!(profile.leds.colour(BiColourLed::ButtonA), LedColour::Green);
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
            problem("shift", ProblemKind::NotForDeviceType),
            problem("blink", ProblemKind::NotForDeviceType),
            problem("leds", ProblemKind::NotForDeviceType),
            problem("mfd.scroll", ProblemKind::UnknownKey),
        ]
    );
    assert!(!profile.shift);
    assert_eq!(profile.name, "Plain");
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
        "led_brightness is out of range; the default is used"
    );
    assert_eq!(
        messages[2],
        "mfd_brightness is missing; the default is used"
    );
}
