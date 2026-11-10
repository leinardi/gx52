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

use x52::device::DeviceType;
use x52::input::InputMap;

use super::*;

#[test]
fn every_button_but_the_mode_keys_has_its_own_name() {
    let mut names = BTreeSet::new();
    let mut buttons = BTreeSet::new();
    for device_type in DeviceType::ALL {
        let map = InputMap::for_device(device_type);
        for (code, button) in map.buttons() {
            if map.mode_for(code).is_none() && buttons.insert(button) {
                let name = input_name(Input::Button(button));
                assert!(!name.is_empty(), "{button:?}");
                assert!(names.insert(name), "{button:?}: a name used twice");
            }
        }
    }
    for direction in Pov1Direction::ALL {
        assert!(
            names.insert(input_name(Input::Pov1(direction))),
            "{direction:?}"
        );
    }
}

#[test]
fn names_follow_protocol_md() {
    assert_eq!(
        input_name(Input::Button(Button::TriggerStage1)),
        "Trigger, first stage"
    );
    assert_eq!(input_name(Input::Button(Button::Fire)), "Fire");
    assert_eq!(input_name(Input::Pov1(Pov1Direction::Up)), "POV 1 up");
    assert_eq!(input_name(Input::Button(Button::Pov2Left)), "POV 2 left");
}
