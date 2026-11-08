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

/// The bytes of `name` under `tests/testdata/profiles`.
pub(crate) fn fixture(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/testdata/profiles")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
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
    assert_eq!(profile.led_brightness.level(), 128);
    assert_eq!(profile.mfd_brightness.level(), 128);
    assert_eq!(profile.leds.colour(BiColourLed::Pov2), LedColour::Green);
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
        profile.mfd_lines,
        [String::new(), String::new(), String::new()]
    );
}

#[test]
fn the_builtin_default_is_the_default_device_state() {
    for device_type in DeviceType::ALL {
        assert_eq!(
            Profile::builtin_default(device_type).to_device_state(),
            DeviceState::default_for(device_type),
            "{device_type}"
        );
    }
}

#[test]
fn the_device_state_has_the_profile_values() {
    let mut profile = Profile::builtin_default(DeviceType::X52Pro);
    profile.mfd_lines[0] = "GX52".to_owned();
    profile.clock2.source = ZoneSpec::Iana(IanaZone::new("Europe/Rome").unwrap());
    profile.shift = true;
    let state = profile.to_device_state();
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
    profile.shift = true;
    profile.blink = true;
    profile
        .leds
        .set_colour(BiColourLed::ButtonA, LedColour::Red);
    let state = profile.to_device_state();
    assert!(!state.shift);
    assert!(!state.blink);
    assert_eq!(state.leds, DeviceState::default_for(DeviceType::X52).leds);
}

#[test]
fn unknown_zones_cannot_be_named() {
    assert!(IanaZone::new("Mars/Olympus_Mons").is_err());
    assert_eq!(IanaZone::new("Europe/Rome").unwrap().name(), "Europe/Rome");
}
