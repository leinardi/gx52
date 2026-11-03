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

use std::collections::BTreeSet;

use super::*;

#[test]
fn from_ids_knows_exactly_the_three_documented_devices() {
    assert_eq!(
        DeviceType::from_ids(0x06a3, 0x0762),
        Some(DeviceType::X52Pro)
    );
    assert_eq!(DeviceType::from_ids(0x06a3, 0x0255), Some(DeviceType::X52));
    assert_eq!(DeviceType::from_ids(0x06a3, 0x075c), Some(DeviceType::X52));
    let known = [0x0762, 0x0255, 0x075c];
    for product in 0..=u16::MAX {
        if !known.contains(&product) {
            assert_eq!(
                DeviceType::from_ids(VENDOR_ID, product),
                None,
                "{product:#06x}"
            );
        }
    }
    // A known product ID under another vendor is not ours.
    assert_eq!(DeviceType::from_ids(0x046d, 0x0762), None);
    assert_eq!(DeviceType::from_ids(0x0000, 0x0255), None);
}

#[test]
fn only_the_pro_has_leds_shift_and_blink() {
    assert!(DeviceType::X52Pro.has_leds());
    assert!(DeviceType::X52Pro.has_shift());
    assert!(DeviceType::X52Pro.has_blink());
    assert!(!DeviceType::X52.has_leds());
    assert!(!DeviceType::X52.has_shift());
    assert!(!DeviceType::X52.has_blink());
}

#[test]
fn ids_are_the_profile_names() {
    assert_eq!(DeviceType::X52Pro.id(), "x52-pro");
    assert_eq!(DeviceType::X52.id(), "x52");
    assert_eq!(DeviceType::X52Pro.to_string(), "x52-pro");
}

#[test]
fn bi_colour_bits_match_the_led_bit_table() {
    let table = [
        (BiColourLed::ButtonA, 2, 3),
        (BiColourLed::ButtonB, 4, 5),
        (BiColourLed::ButtonD, 6, 7),
        (BiColourLed::ButtonE, 8, 9),
        (BiColourLed::T1T2, 10, 11),
        (BiColourLed::T3T4, 12, 13),
        (BiColourLed::T5T6, 14, 15),
        (BiColourLed::Pov2, 16, 17),
        (BiColourLed::ButtonI, 18, 19),
    ];
    assert_eq!(table.map(|(led, _, _)| led), BiColourLed::ALL);
    for (led, red, green) in table {
        assert_eq!((led.red_bit(), led.green_bit()), (red, green), "{led:?}");
    }
    assert_eq!(SingleColourLed::Fire.bit(), 1);
    assert_eq!(SingleColourLed::Throttle.bit(), 20);
}

#[test]
fn led_bits_cover_exactly_1_to_20() {
    let bits: Vec<u8> = BiColourLed::ALL
        .iter()
        .flat_map(|led| [led.red_bit(), led.green_bit()])
        .chain(SingleColourLed::ALL.iter().map(|led| led.bit()))
        .collect();
    let unique: BTreeSet<u8> = bits.iter().copied().collect();
    assert_eq!(bits.len(), unique.len(), "a bit is used twice: {bits:?}");
    assert_eq!(unique, (1..=20).collect::<BTreeSet<u8>>());
}

#[test]
fn led_keys_are_the_profile_keys() {
    let keys: Vec<&str> = BiColourLed::ALL
        .iter()
        .map(|led| led.key())
        .chain(SingleColourLed::ALL.iter().map(|led| led.key()))
        .collect();
    assert_eq!(
        keys,
        [
            "a", "b", "d", "e", "t1_t2", "t3_t4", "t5_t6", "pov_2", "i", "fire", "throttle"
        ]
    );
}

#[test]
fn amber_is_red_and_green() {
    let halves = [
        LedColour::Off,
        LedColour::Green,
        LedColour::Red,
        LedColour::Amber,
    ]
    .map(|colour| (colour.green(), colour.red()));
    assert_eq!(
        halves,
        [(false, false), (true, false), (false, true), (true, true)]
    );
}
