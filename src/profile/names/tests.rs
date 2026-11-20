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

use x52::device::DeviceType;

use super::*;

const BUTTON_E: Input = Input::Button(Button::ButtonE);
const GEAR: Input = Input::Button(Button::ThrottleHatLeft);

fn id() -> ProfileId {
    ProfileId::parse("0f1e2d3c-aaaa-4bbb-8ccc-0123456789ab").expect("a valid id")
}

fn named() -> BTreeMap<Input, String> {
    BTreeMap::from([
        (BUTTON_E, "Emerg. Lights".to_owned()),
        (GEAR, "Gear".to_owned()),
    ])
}

/// A set of names with its generations, edited as the application edits them.
struct Edited {
    names: BTreeMap<Input, String>,
    generations: NameGenerations,
}

impl Edited {
    const SET: SettingsSet = SettingsSet::Mode(Mode::Two);

    fn new() -> Self {
        Self {
            names: named(),
            generations: NameGenerations::default(),
        }
    }

    fn clear(&mut self) -> NamesUndo {
        let before = std::mem::take(&mut self.names);
        NamesUndo::record(
            &mut self.generations,
            id(),
            Self::SET,
            before,
            BTreeMap::new(),
        )
    }

    fn type_name(&mut self, input: Input, text: &str) {
        if set_name(&mut self.names, input, text) {
            self.generations.bump(&id(), Self::SET);
        }
    }

    fn undo(&mut self, undo: &NamesUndo) {
        if let Some(names) = undo.restore(&mut self.generations) {
            self.names = names;
        }
    }
}

#[test]
fn the_groups_hold_every_nameable_control_once() {
    for (device_type, count) in [(DeviceType::X52Pro, 40), (DeviceType::X52, 35)] {
        let map = InputMap::for_device(device_type);
        let grouped: Vec<Input> = NameGroup::ALL
            .into_iter()
            .flat_map(|group| group.nameable(&map))
            .collect();
        assert_eq!(grouped.len(), count, "{device_type:?}");
        let mut sorted = grouped.clone();
        sorted.sort_unstable();
        let mut nameable: Vec<Input> = map.nameable().collect();
        nameable.sort_unstable();
        assert_eq!(sorted, nameable, "{device_type:?}");
    }
}

#[test]
fn the_summaries_count_and_list_the_names() {
    assert_eq!(
        count_summary(5, Some(Mode::Two)),
        "5 controls have custom names in mode 2."
    );
    assert_eq!(count_summary(1, None), "1 control has a custom name.");
    assert_eq!(
        count_summary(0, Some(Mode::Three)),
        "No custom names in mode 3: every control uses its built-in name."
    );
    assert_eq!(
        group_summary(&["Gear", "Emerg. Lights"]),
        "2 custom: Gear, Emerg. Lights"
    );
    assert_eq!(group_summary(&[]), "Built-in names");
    assert_eq!(
        shown_on_line(2),
        "Shown on MFD line 3 while the control is held."
    );
}

#[test]
fn a_blank_name_is_no_name() {
    let mut names = named();
    assert!(set_name(&mut names, BUTTON_E, "  "));
    assert!(!names.contains_key(&BUTTON_E));
    assert!(!set_name(&mut names, BUTTON_E, ""));
    assert!(
        !set_name(&mut names, GEAR, "Gear"),
        "the same name changes nothing"
    );
}

#[test]
fn undo_after_clear_restores_the_names() {
    let mut edited = Edited::new();
    let undo = edited.clear();
    assert!(edited.names.is_empty());
    edited.undo(&undo);
    assert_eq!(edited.names, named());
    // The Undo is spent.
    edited.names.clear();
    edited.undo(&undo);
    assert!(edited.names.is_empty());
}

#[test]
fn undo_never_overwrites_a_later_name() {
    let mut edited = Edited::new();
    let undo = edited.clear();
    edited.type_name(BUTTON_E, "Eject");
    edited.undo(&undo);
    assert_eq!(
        edited.names,
        BTreeMap::from([(BUTTON_E, "Eject".to_owned())])
    );
}

#[test]
fn undo_does_nothing_after_a_name_typed_and_reset() {
    let mut edited = Edited::new();
    let undo = edited.clear();
    edited.type_name(BUTTON_E, "E");
    edited.type_name(BUTTON_E, "");
    edited.undo(&undo);
    assert!(edited.names.is_empty());
}

#[test]
fn undo_does_nothing_after_a_kind_change() {
    let mut edited = Edited::new();
    let undo = edited.clear();
    edited.generations.bump_profile(&id());
    edited.undo(&undo);
    assert!(edited.names.is_empty());
}

#[test]
fn badges_are_the_button_numbers_and_the_arrows() {
    let pro = InputMap::for_device(DeviceType::X52Pro);
    let plain = InputMap::for_device(DeviceType::X52);
    assert_eq!(
        badge(&pro, Input::Button(Button::TriggerStage1)).as_deref(),
        Some("1")
    );
    assert_eq!(
        badge(&pro, Input::Pov1(Pov1Direction::Left)).as_deref(),
        Some("←")
    );
    // Every nameable control has a badge, and no two buttons share one.
    let mut numbers: Vec<String> = NameGroup::ALL
        .into_iter()
        .flat_map(|group| group.nameable(&pro))
        .filter(|input| matches!(input, Input::Button(_)))
        .map(|input| badge(&pro, input).expect("a nameable button has a number"))
        .collect();
    let count = numbers.len();
    numbers.sort();
    numbers.dedup();
    assert_eq!(numbers.len(), count);
    assert_eq!(badge(&plain, Input::Button(Button::Select)), None);
}
