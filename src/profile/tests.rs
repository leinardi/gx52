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

use std::path::Path;

use x52::clock::{Clock1Source, FixedOffset};
use x52::device::{BiColourLed, LedColour};

use super::*;

/// The bytes of `relative` under `tests/testdata`.
pub(crate) fn testdata(relative: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// The bytes of `name` under `tests/testdata/profiles`.
pub(crate) fn fixture(name: &str) -> Vec<u8> {
    testdata(&format!("profiles/{name}"))
}

#[test]
fn ids_take_only_lowercase_letters_digits_and_dashes() {
    for valid in [
        "x52-pro-default",
        "0f1e2d3c-aaaa-4bbb-8ccc-0123456789ab",
        "a",
    ] {
        assert!(ProfileId::parse(valid).is_some(), "{valid}");
    }
    for invalid in ["", "X52", "a b", "a_b", "../a", "a.toml", "é"] {
        assert!(ProfileId::parse(invalid).is_none(), "{invalid}");
    }
}

#[test]
fn reserved_ids_name_their_device_type() {
    for device_type in DeviceType::ALL {
        let id = ProfileId::default_for(device_type);
        assert_eq!(id.default_type(), Some(device_type));
        assert_eq!(id.file_name(), format!("{}-default.toml", device_type.id()));
    }
    assert_eq!(
        ProfileId::default_for(DeviceType::X52Pro).as_str(),
        "x52-pro-default"
    );
    assert_eq!(
        ProfileId::default_for(DeviceType::X52).as_str(),
        "x52-default"
    );
}

#[test]
fn random_ids_are_valid_and_never_reserved() {
    let id = ProfileId::random();
    assert_eq!(ProfileId::parse(id.as_str()), Some(id.clone()));
    assert_eq!(id.default_type(), None);
    assert_ne!(ProfileId::random(), id);
}

#[test]
fn the_builtin_default_has_the_values_of_profiles_md() {
    let profile = Profile::builtin_default(DeviceType::X52Pro);
    assert_eq!(profile.name, "Default");
    assert_eq!(profile.kind(), ProfileKind::Single);
    let settings = profile.settings_for(Mode::One);
    assert_eq!(settings.led_brightness.level(), 128);
    assert_eq!(settings.mfd_brightness.level(), 128);
    assert_eq!(settings.leds.colour(BiColourLed::Pov2), LedColour::Green);
    assert_eq!(profile.clock1.source, Clock1Source::Local);
    assert_eq!(
        profile.clock2.source,
        ZoneSpec::Fixed(FixedOffset::new(-240).unwrap())
    );
    assert_eq!(
        profile.clock3.source,
        ZoneSpec::Fixed(FixedOffset::new(480).unwrap())
    );
    assert_eq!(
        settings.mfd_lines,
        [const { MfdLineSource::Empty }; MFD_LINES]
    );
}

#[test]
fn the_builtin_default_is_the_default_device_state() {
    for device_type in DeviceType::ALL {
        for mode in Mode::ALL {
            assert_eq!(
                Profile::builtin_default(device_type).to_device_state(mode, &LiveValues::default()),
                DeviceState::default_for(device_type),
                "{device_type} {mode:?}"
            );
        }
    }
}

#[test]
fn the_device_state_has_the_profile_values() {
    let mut profile = Profile::builtin_default(DeviceType::X52Pro);
    let settings = profile.settings.for_mode_mut(Mode::One);
    settings.mfd_lines[0] = MfdLineSource::Text("GX52".to_owned());
    settings.shift = true;
    profile.clock2.source = ZoneSpec::Iana(IanaZone::new("Europe/Rome").unwrap());
    let state = profile.to_device_state(Mode::One, &LiveValues::default());
    assert_eq!(state.mfd_lines[0], MfdText::new("GX52").unwrap());
    assert_eq!(
        state.clock2.source,
        ZoneSource::iana("Europe/Rome").unwrap()
    );
    assert!(state.shift);
}

#[test]
fn the_x52_state_never_has_leds_shift_or_blink() {
    let mut profile = Profile::builtin_default(DeviceType::X52);
    let settings = profile.settings.for_mode_mut(Mode::One);
    settings.shift = true;
    settings.blink = true;
    settings
        .leds
        .set_colour(BiColourLed::ButtonA, LedColour::Red);
    let state = profile.to_device_state(Mode::One, &LiveValues::default());
    assert!(!state.shift);
    assert!(!state.blink);
    assert_eq!(state.leds, DeviceState::default_for(DeviceType::X52).leds);
}

/// A per-mode X52 Pro profile whose modes differ in LED brightness: 10, 20 and 30.
fn per_mode_profile() -> Profile {
    let mut profile = Profile::builtin(DeviceType::X52Pro, "Per mode".to_owned());
    profile.settings = ModeSets::PerMode(Box::new([10, 20, 30].map(|level| {
        let mut settings = ModeSettings::builtin(DeviceType::X52Pro);
        settings.led_brightness = Brightness::new(level).unwrap();
        settings
    })));
    profile
}

#[test]
fn a_single_profile_has_the_same_settings_in_every_mode() {
    let mut profile = Profile::builtin_default(DeviceType::X52Pro);
    profile.settings.for_mode_mut(Mode::Three).led_brightness = Brightness::new(7).unwrap();
    for mode in Mode::ALL {
        assert_eq!(
            profile.settings_for(mode).led_brightness.level(),
            7,
            "{mode:?}"
        );
        assert_eq!(
            profile
                .to_device_state(mode, &LiveValues::default())
                .led_brightness
                .level(),
            7,
            "{mode:?}"
        );
    }
}

#[test]
fn a_per_mode_profile_has_one_set_per_mode() {
    let profile = per_mode_profile();
    assert_eq!(profile.kind(), ProfileKind::PerMode);
    for (mode, level) in Mode::ALL.into_iter().zip([10, 20, 30]) {
        assert_eq!(profile.settings_for(mode).led_brightness.level(), level);
        let state = profile.to_device_state(mode, &LiveValues::default());
        assert_eq!(state.led_brightness.level(), level, "{mode:?}");
    }
}

#[test]
fn changing_the_kind_copies_the_single_set_or_keeps_mode_1() {
    let single = Profile::builtin_default(DeviceType::X52Pro).settings;
    let per_mode = single.clone().into_per_mode();
    assert_eq!(per_mode.kind(), ProfileKind::PerMode);
    for mode in Mode::ALL {
        assert_eq!(per_mode.for_mode(mode), single.for_mode(Mode::One));
    }
    assert_eq!(per_mode.clone().into_per_mode(), per_mode);
    assert_eq!(per_mode.into_single(), single);

    let kept = per_mode_profile().settings.into_single();
    assert_eq!(kept.kind(), ProfileKind::Single);
    for mode in Mode::ALL {
        assert_eq!(kept.for_mode(mode).led_brightness.level(), 10, "{mode:?}");
    }
    assert_eq!(kept.clone().into_single(), kept);
}

#[test]
fn dynamic_lines_show_the_live_values_cut_to_a_line() {
    let mut profile = Profile::builtin(DeviceType::X52Pro, "A name of twenty chars".to_owned());
    profile.settings.for_mode_mut(Mode::One).mfd_lines = [
        MfdLineSource::ProfileName,
        MfdLineSource::Mode,
        MfdLineSource::LastButton,
    ];
    let live = LiveValues {
        mode_shown: Mode::Two,
        last_button: Some("Trigger, first stage".to_owned()),
    };
    let lines = profile.to_device_state(Mode::One, &live).mfd_lines;
    assert_eq!(
        lines.map(|line| line.preview()),
        ["A name of twenty", "Mode 2", "Trigger, first s"].map(str::to_owned)
    );
    let idle = profile.to_device_state(Mode::One, &LiveValues::default());
    assert_eq!(idle.mfd_lines[1].preview(), "Mode 1");
    assert_eq!(idle.mfd_lines[2], MfdText::EMPTY, "nothing held");
}

#[test]
fn a_text_line_too_long_for_the_mfd_is_not_shown() {
    let mut profile = Profile::builtin_default(DeviceType::X52Pro);
    profile.settings.for_mode_mut(Mode::One).mfd_lines[0] =
        MfdLineSource::Text("this line is far too long".to_owned());
    let state = profile.to_device_state(Mode::One, &LiveValues::default());
    assert_eq!(state.mfd_lines[0], MfdText::EMPTY);
}

#[test]
fn unknown_zones_cannot_be_named() {
    assert!(IanaZone::new("Mars/Olympus_Mons").is_err());
    assert_eq!(IanaZone::new("Europe/Rome").unwrap().name(), "Europe/Rome");
}
