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

use super::*;

/// Every control the page shows for `map`: the Buttons groups, the toggles and the hat cells.
fn shown(map: &InputMap) -> Vec<Input> {
    let mut inputs: Vec<Input> = ButtonGroup::ALL
        .into_iter()
        .flat_map(|group| group_buttons(group, map))
        .chain(toggles(map))
        .map(|(button, _)| Input::Button(button))
        .collect();
    for hat in Hat::ALL.into_iter().filter(|hat| hat.is_on(map)) {
        inputs.extend(Pov1Direction::ALL.map(|direction| hat.input(direction)));
    }
    inputs
}

#[test]
fn every_button_of_each_map_is_shown_once() {
    for device in DeviceType::ALL {
        let map = InputMap::for_device(device);
        let shown = shown(&map);
        let unique: BTreeSet<Input> = shown.iter().copied().collect();
        assert_eq!(shown.len(), unique.len(), "{device}: shown twice");
        let expected: BTreeSet<Input> = map
            .buttons()
            .filter(|(code, _)| map.mode_for(*code).is_none())
            .map(|(_, button)| Input::Button(button))
            .chain(Pov1Direction::ALL.map(Input::Pov1))
            .collect();
        assert_eq!(unique, expected, "{device}");
    }
}

#[test]
fn groups_hold_the_same_controls_on_both_maps() {
    let pro = InputMap::for_device(DeviceType::X52Pro);
    let x52 = InputMap::for_device(DeviceType::X52);
    let buttons = |group, map| -> Vec<Button> {
        group_buttons(group, map)
            .into_iter()
            .map(|(button, _)| button)
            .collect()
    };
    for map in [&pro, &x52] {
        assert!(buttons(ButtonGroup::Throttle, map).contains(&Button::Clutch));
        assert!(buttons(ButtonGroup::Throttle, map).contains(&Button::MouseButton));
    }
    // The codes come from each map: the i button is 718 on the Pro and 717 on the X52.
    assert!(group_buttons(ButtonGroup::Throttle, &pro).contains(&(Button::Clutch, 718)));
    assert!(group_buttons(ButtonGroup::Throttle, &x52).contains(&(Button::Clutch, 717)));
    assert!(group_buttons(ButtonGroup::Throttle, &x52).contains(&(Button::MouseButton, 718)));
    assert_eq!(
        buttons(ButtonGroup::Mfd, &x52),
        [Button::Function, Button::StartStop, Button::Reset],
        "the X52 has no MFD wheels"
    );
    assert_eq!(buttons(ButtonGroup::Mfd, &pro).len(), 8);
}

#[test]
fn a_hat_points_at_one_cell_and_opposites_cancel() {
    let map = InputMap::for_device(DeviceType::X52Pro);
    let mut held = HeldInputs::new();
    assert_eq!(Hat::Pov2.cell(&held), (1, 1));
    held.press(Input::Button(Button::Pov2Up));
    held.press(Input::Button(Button::Pov2Right));
    assert_eq!(Hat::Pov2.cell(&held), (2, 0));
    assert_eq!(Hat::Throttle.cell(&held), (1, 1));
    held.set_pov1(-1, 1);
    assert_eq!(Hat::Pov1.cell(&held), (0, 2));
    held.press(Input::Button(Button::Pov2Down));
    assert_eq!(Hat::Pov2.cell(&held), (2, 1));
    assert!(Hat::ALL.into_iter().all(|hat| hat.is_on(&map)));
}

#[test]
fn the_tooltip_names_the_number_games_show() {
    let map = InputMap::for_device(DeviceType::X52Pro);
    assert_eq!(
        tooltip(&map, Button::Fire, 289),
        "Button 2 · Fire · evdev code 289"
    );
    assert_eq!(
        tooltip(&map, Button::Select, 726),
        "Button 39 · Select · evdev code 726"
    );
}

#[test]
fn the_summary_lists_the_held_inputs_and_the_mode() {
    let map = InputMap::for_device(DeviceType::X52Pro);
    let mut held = HeldInputs::new();
    assert_eq!(summary(&map, &held, None), "Buttons pressed: none.");
    held.press(Input::Button(Button::ButtonA));
    held.press(Input::Button(Button::Fire));
    held.set_pov1(0, -1);
    assert_eq!(
        summary(&map, &held, Some(Mode::Two)),
        "Buttons pressed: Fire, A, POV 1 up. Mode 2."
    );
}

#[test]
fn a_mode_change_alone_changes_the_summary() {
    let map = InputMap::for_device(DeviceType::X52Pro);
    let held = HeldInputs::new();
    assert_ne!(
        summary(&map, &held, Some(Mode::One)),
        summary(&map, &held, Some(Mode::Three))
    );
}

fn ms(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}

#[test]
fn a_short_press_stays_lit_for_the_minimum() {
    let mut pulse = Pulse::default();
    assert_eq!(pulse.show(true, ms(1000)), PulseStep::Light);
    assert_eq!(pulse.show(true, ms(1001)), PulseStep::Keep);
    assert_eq!(pulse.show(false, ms(1004)), PulseStep::PutOutIn(ms(146)));
    assert!(pulse.expire());
    assert_eq!(pulse, Pulse::default());
}

#[test]
fn a_long_press_goes_out_at_the_release() {
    let mut pulse = Pulse::default();
    pulse.show(true, ms(0));
    assert_eq!(pulse.show(false, ms(150)), PulseStep::PutOut);
    assert_eq!(pulse.show(false, ms(160)), PulseStep::Keep);
}

#[test]
fn a_press_again_before_the_timer_keeps_it_lit() {
    let mut pulse = Pulse::default();
    pulse.show(true, ms(0));
    assert_eq!(pulse.show(false, ms(10)), PulseStep::PutOutIn(ms(140)));
    assert_eq!(pulse.show(true, ms(20)), PulseStep::Keep, "still lit");
    assert!(!pulse.expire(), "held when the timer fires");
    assert_eq!(pulse.show(false, ms(400)), PulseStep::PutOut);
}
