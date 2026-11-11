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

use x52::charset::MfdText;
use x52::device::DeviceType;
use x52::input::InputMap;

use super::*;

/// Checks that `input` has a name of its own that fits an MFD line, and records it in `names`.
fn check_name(input: Input, names: &mut BTreeSet<String>) {
    let name = input_name(input);
    assert!(!name.is_empty(), "{input:?}");
    assert!(
        MfdText::new(&name).is_ok(),
        "{name:?} does not fit an MFD line"
    );
    assert!(names.insert(name), "{input:?}: a name used twice");
}

#[test]
fn every_input_but_the_mode_keys_has_its_own_name_that_fits_a_line() {
    let mut names = BTreeSet::new();
    let mut buttons = BTreeSet::new();
    for device_type in DeviceType::ALL {
        let map = InputMap::for_device(device_type);
        for (code, button) in map.buttons() {
            if map.mode_for(code).is_none() && buttons.insert(button) {
                check_name(Input::Button(button), &mut names);
            }
        }
    }
    for direction in Pov1Direction::ALL {
        check_name(Input::Pov1(direction), &mut names);
    }
}

#[test]
fn names_follow_protocol_md_shortened_to_a_line() {
    let name = |button| input_name(Input::Button(button));
    assert_eq!(name(Button::TriggerStage1), "Trigger stage 1");
    assert_eq!(name(Button::Fire), "Fire");
    assert_eq!(input_name(Input::Pov1(Pov1Direction::Up)), "POV 1 up");
    assert_eq!(name(Button::Pov2Left), "POV 2 left");
    assert_eq!(name(Button::ThrottleHatRight), "Thr. hat right");
}

#[test]
fn every_button_of_each_map_has_a_distinct_tester_label_and_full_name() {
    for device_type in DeviceType::ALL {
        let map = InputMap::for_device(device_type);
        let mut labels = BTreeSet::new();
        let mut full = BTreeSet::new();
        for (_, button) in map.buttons() {
            let label = tester_label(button);
            assert!(!label.is_empty(), "{button:?}");
            assert!(
                labels.insert(label),
                "{device_type}: {button:?} label used twice"
            );
            let name = full_name(button);
            assert!(!name.is_empty(), "{button:?}");
            assert!(
                full.insert(name),
                "{device_type}: {button:?} name used twice"
            );
        }
    }
}

#[test]
fn tester_names_follow_the_prototype() {
    assert_eq!(tester_label(Button::TriggerStage1), "Trigger");
    assert_eq!(tester_label(Button::ScrollUp), "Wheel up");
    assert_eq!(tester_label(Button::Clutch), "i");
    assert_eq!(tester_label(Button::PageUp), "Page Up");
    assert_eq!(full_name(Button::TriggerStage2), "Trigger, second stage");
    assert_eq!(full_name(Button::Up), "Select wheel up");
    assert_eq!(full_name(Button::Fire), "Fire");
}
