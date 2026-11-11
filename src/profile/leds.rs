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

//! The LEDs as the LED editor shows them: names, states, where each sits, and what each button
//! shows for a mode's settings (docs/ui.md, "LEDs view").
//!
//! Plain Rust: the widgets only apply a [`LedView`], so the rules are tested here.

use gettextrs::gettext;
use x52::device::{BiColourLed, LedColour, SingleColourLed};
use x52::state::Leds;

use super::editor::EditorState;

/// An LED the editor shows: the ten that can be set, and POV 1, fixed green.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum EditorLed {
    /// A red and green LED.
    Bi(BiColourLed),
    /// A single-colour LED.
    Single(SingleColourLed),
    /// The POV 1 LED: always green, following the brightness and blink.
    Pov1,
}

impl EditorLed {
    /// Every LED the editor shows.
    #[cfg(test)]
    pub(crate) const ALL: [Self; 12] = [
        Self::Single(SingleColourLed::Fire),
        Self::Bi(BiColourLed::ButtonA),
        Self::Bi(BiColourLed::ButtonB),
        Self::Bi(BiColourLed::ButtonD),
        Self::Bi(BiColourLed::ButtonE),
        Self::Bi(BiColourLed::T1T2),
        Self::Bi(BiColourLed::T3T4),
        Self::Bi(BiColourLed::T5T6),
        Self::Bi(BiColourLed::Pov2),
        Self::Bi(BiColourLed::ButtonI),
        Self::Single(SingleColourLed::Throttle),
        Self::Pov1,
    ];

    /// The name printed on the device, translated where it is a word.
    pub(crate) fn name(self) -> String {
        match self {
            Self::Single(SingleColourLed::Fire) => gettext("Fire"),
            Self::Single(SingleColourLed::Throttle) => gettext("Throttle"),
            Self::Bi(led) => match led {
                BiColourLed::ButtonA => "A",
                BiColourLed::ButtonB => "B",
                BiColourLed::ButtonD => "D",
                BiColourLed::ButtonE => "E",
                BiColourLed::T1T2 => "T1/T2",
                BiColourLed::T3T4 => "T3/T4",
                BiColourLed::T5T6 => "T5/T6",
                BiColourLed::Pov2 => "POV 2",
                BiColourLed::ButtonI => "i",
            }
            .to_owned(),
            Self::Pov1 => "POV 1".to_owned(),
        }
    }

    /// The states the LED can be set to, in the order the popover lists them; none for POV 1.
    pub(crate) const fn choices(self) -> &'static [LedChoice] {
        match self {
            Self::Bi(_) => &[
                LedChoice::Off,
                LedChoice::Green,
                LedChoice::Amber,
                LedChoice::Red,
            ],
            Self::Single(_) => &[LedChoice::Off, LedChoice::On],
            Self::Pov1 => &[],
        }
    }
}

/// A state an LED is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LedChoice {
    /// Off.
    Off,
    /// Green; a bi-colour LED or POV 1.
    Green,
    /// Red and green together; a bi-colour LED.
    Amber,
    /// Red; a bi-colour LED.
    Red,
    /// On; a single-colour LED.
    On,
}

impl LedChoice {
    /// Every state.
    pub(crate) const ALL: [Self; 5] = [Self::Off, Self::Green, Self::Amber, Self::Red, Self::On];

    /// A stable name, used for the toggles of the popover.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Green => "green",
            Self::Amber => "amber",
            Self::Red => "red",
            Self::On => "on",
        }
    }

    /// The state named `id`.
    pub(crate) fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|choice| choice.id() == id)
    }

    /// The name shown, translated.
    pub(crate) fn label(self) -> String {
        match self {
            Self::Off => gettext("Off"),
            Self::Green => gettext("Green"),
            Self::Amber => gettext("Amber"),
            Self::Red => gettext("Red"),
            Self::On => gettext("On"),
        }
    }

    /// The style class that colours a button in this state.
    pub(crate) const fn css_class(self) -> &'static str {
        match self {
            Self::Off => "led-off",
            Self::Green => "led-green",
            Self::Amber => "led-amber",
            Self::Red => "led-red",
            Self::On => "led-on",
        }
    }
}

/// The state of `led` in `leds`.
pub(crate) const fn choice(leds: &Leds, led: EditorLed) -> LedChoice {
    match led {
        EditorLed::Bi(led) => match leds.colour(led) {
            LedColour::Off => LedChoice::Off,
            LedColour::Green => LedChoice::Green,
            LedColour::Amber => LedChoice::Amber,
            LedColour::Red => LedChoice::Red,
        },
        EditorLed::Single(led) => {
            if leds.is_on(led) {
                LedChoice::On
            } else {
                LedChoice::Off
            }
        }
        EditorLed::Pov1 => LedChoice::Green,
    }
}

/// Sets `led` to `choice` in `leds`; `false`, changing nothing, when `led` cannot take `choice`.
pub(crate) const fn set_choice(leds: &mut Leds, led: EditorLed, choice: LedChoice) -> bool {
    match (led, choice) {
        (EditorLed::Bi(led), LedChoice::Off) => leds.set_colour(led, LedColour::Off),
        (EditorLed::Bi(led), LedChoice::Green) => leds.set_colour(led, LedColour::Green),
        (EditorLed::Bi(led), LedChoice::Amber) => leds.set_colour(led, LedColour::Amber),
        (EditorLed::Bi(led), LedChoice::Red) => leds.set_colour(led, LedColour::Red),
        (EditorLed::Single(led), LedChoice::Off) => leds.set_on(led, false),
        (EditorLed::Single(led), LedChoice::On) => leds.set_on(led, true),
        (EditorLed::Bi(_), LedChoice::On)
        | (EditorLed::Single(_), LedChoice::Green | LedChoice::Amber | LedChoice::Red)
        | (EditorLed::Pov1, _) => return false,
    }
    true
}

/// A part of the device, shown as one card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Card {
    /// The stick.
    Stick,
    /// The base of the stick, with the toggle switches.
    StickBase,
    /// The throttle.
    Throttle,
}

impl Card {
    /// Every card, in the order shown.
    pub(crate) const ALL: [Self; 3] = [Self::Stick, Self::StickBase, Self::Throttle];

    /// The heading of the card, translated.
    pub(crate) fn title(self) -> String {
        match self {
            Self::Stick => gettext("Stick"),
            Self::StickBase => gettext("Stick base"),
            Self::Throttle => gettext("Throttle"),
        }
    }

    /// The number of grid columns of the card.
    #[cfg(test)]
    pub(crate) const fn columns(self) -> i32 {
        match self {
            Self::Stick | Self::Throttle => 2,
            Self::StickBase => 3,
        }
    }
}

/// Where an LED sits: its card, and its cell in the card's grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Place {
    /// The LED.
    pub(crate) led: EditorLed,
    /// The card it is on.
    pub(crate) card: Card,
    /// The grid row, from `0`.
    pub(crate) row: i32,
    /// The grid column, from `0`.
    pub(crate) column: i32,
    /// The number of columns it spans.
    pub(crate) width: i32,
}

const fn place(led: EditorLed, card: Card, row: i32, column: i32, width: i32) -> Place {
    Place {
        led,
        card,
        row,
        column,
        width,
    }
}

/// Where every LED sits, roughly as on the device (docs/spikes/07-led-editor.md, "Design for the
/// app").
pub(crate) const PLACES: [Place; 12] = [
    place(EditorLed::Bi(BiColourLed::Pov2), Card::Stick, 0, 0, 1),
    place(
        EditorLed::Single(SingleColourLed::Fire),
        Card::Stick,
        0,
        1,
        1,
    ),
    place(EditorLed::Bi(BiColourLed::ButtonA), Card::Stick, 1, 0, 1),
    place(EditorLed::Bi(BiColourLed::ButtonB), Card::Stick, 1, 1, 1),
    place(EditorLed::Pov1, Card::Stick, 2, 0, 2),
    place(EditorLed::Bi(BiColourLed::T1T2), Card::StickBase, 0, 0, 1),
    place(EditorLed::Bi(BiColourLed::T3T4), Card::StickBase, 0, 1, 1),
    place(EditorLed::Bi(BiColourLed::T5T6), Card::StickBase, 0, 2, 1),
    place(EditorLed::Bi(BiColourLed::ButtonD), Card::Throttle, 0, 0, 1),
    place(EditorLed::Bi(BiColourLed::ButtonE), Card::Throttle, 0, 1, 1),
    place(EditorLed::Bi(BiColourLed::ButtonI), Card::Throttle, 1, 0, 1),
    place(
        EditorLed::Single(SingleColourLed::Throttle),
        Card::Throttle,
        1,
        1,
        1,
    ),
];

/// Whether the LEDs show as off, whatever their states: brightness `0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lighting {
    /// The LEDs show their states.
    Lit,
    /// Brightness is `0`: every LED is off on the device.
    Dark,
}

/// The accessible label of `led` in `choice`: "A LED: Red", with ", brightness 0" while
/// the LEDs are dark.
pub(crate) fn accessible_label(led: EditorLed, choice: LedChoice, lighting: Lighting) -> String {
    let label = match led {
        EditorLed::Pov1 => gettext("POV 1 LED: Green, fixed"),
        EditorLed::Bi(_) | EditorLed::Single(_) => tooltip(led, choice),
    };
    match lighting {
        Lighting::Lit => label,
        // Translators: {label} is an LED and its state, for example "A LED: Red".
        Lighting::Dark => gettext("{label}, brightness 0").replace("{label}", &label),
    }
}

/// The tooltip of `led` in `choice`.
pub(crate) fn tooltip(led: EditorLed, choice: LedChoice) -> String {
    match led {
        EditorLed::Pov1 => gettext(
            "POV 1 is always green. It can’t be changed and follows the brightness and blink settings.",
        ),
        // Translators: {led} is the name of an LED, for example "Fire", {state} its state.
        EditorLed::Bi(_) | EditorLed::Single(_) => gettext("{led} LED: {state}")
            .replace("{led}", &led.name())
            .replace("{state}", &choice.label()),
    }
}

impl EditorState {
    /// Whether the LEDs show as off.
    pub(crate) fn lighting(&self) -> Lighting {
        if self.settings.led_brightness.level() == 0 {
            Lighting::Dark
        } else {
            Lighting::Lit
        }
    }
}

/// What the button of one LED shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LedView {
    /// The LED's state, which the popover shows even while the LEDs are dark.
    pub(crate) choice: LedChoice,
    /// The style class of the colour shown.
    pub(crate) class: &'static str,
    /// The accessible label.
    pub(crate) label: String,
    /// The tooltip.
    pub(crate) tooltip: String,
    /// Whether the button blinks.
    pub(crate) blinking: bool,
}

/// What the button of `led` shows in `state`: off while the brightness is `0`, blinking where
/// the device blinks.
pub(crate) fn led_view(state: &EditorState, led: EditorLed) -> LedView {
    let settings = &state.settings;
    let lighting = state.lighting();
    let choice = choice(&settings.leds, led);
    let blinks = match led {
        EditorLed::Pov1 => true,
        EditorLed::Bi(BiColourLed::ButtonI) => choice != LedChoice::Off,
        EditorLed::Bi(_) | EditorLed::Single(_) => false,
    };
    LedView {
        choice,
        class: match lighting {
            Lighting::Lit => choice.css_class(),
            Lighting::Dark => LedChoice::Off.css_class(),
        },
        label: accessible_label(led, choice, lighting),
        tooltip: tooltip(led, choice),
        blinking: blinks
            && lighting == Lighting::Lit
            && settings.blink
            && state.device_type.has_blink(),
    }
}

/// The description of the Brightness and Blink group, when it has one.
pub(crate) fn brightness_note(state: &EditorState) -> Option<String> {
    if !state.device_type.has_leds() {
        return Some(gettext(
            "This model has no LEDs that can be set one by one.",
        ));
    }
    match state.lighting() {
        Lighting::Lit => None,
        Lighting::Dark => Some(gettext(
            "Brightness is 0: every LED is off. The states are kept.",
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use x52::command::Brightness;
    use x52::device::DeviceType;
    use x52::input::Mode;

    use super::*;
    use crate::profile::clocks::ProfileClocks;
    use crate::profile::{ModeSettings, Profile, ProfileId, ProfileKind};

    fn pro_state(level: u8, blink: bool) -> EditorState {
        let mut settings = ModeSettings::builtin(DeviceType::X52Pro);
        settings.led_brightness = Brightness::new(level).expect("a level within 0–128");
        settings.blink = blink;
        EditorState {
            profile: ProfileId::default_for(DeviceType::X52Pro),
            device_type: DeviceType::X52Pro,
            kind: ProfileKind::Single,
            mode: Mode::One,
            settings,
            clocks: ProfileClocks::of(&Profile::builtin_default(DeviceType::X52Pro)),
        }
    }

    #[test]
    fn every_led_is_placed_once() {
        let mut expected: Vec<EditorLed> = BiColourLed::ALL.map(EditorLed::Bi).to_vec();
        expected.extend(SingleColourLed::ALL.map(EditorLed::Single));
        expected.push(EditorLed::Pov1);
        for led in &expected {
            let count = PLACES.iter().filter(|place| place.led == *led).count();
            assert_eq!(count, 1, "{led:?} placed {count} times");
        }
        assert_eq!(PLACES.len(), expected.len());
        assert_eq!(
            EditorLed::ALL.into_iter().collect::<HashSet<_>>(),
            expected.into_iter().collect::<HashSet<_>>()
        );
    }

    #[test]
    fn places_fit_their_card_and_never_overlap() {
        let mut cells = HashSet::new();
        for place in PLACES {
            assert!(place.width >= 1, "{place:?}");
            assert!(place.row >= 0 && place.column >= 0, "{place:?}");
            assert!(
                place.column + place.width <= place.card.columns(),
                "{place:?} is wider than its card"
            );
            for column in place.column..place.column + place.width {
                assert!(
                    cells.insert((place.card, place.row, column)),
                    "{place:?} overlaps another LED"
                );
            }
        }
    }

    #[test]
    fn every_choice_round_trips() {
        for led in EditorLed::ALL {
            for choice in led.choices() {
                let mut leds = Leds::uniform(LedColour::Off, false);
                assert!(set_choice(&mut leds, led, *choice), "{led:?} {choice:?}");
                assert_eq!(super::choice(&leds, led), *choice, "{led:?}");
            }
        }
    }

    #[test]
    fn a_choice_an_led_cannot_take_changes_nothing() {
        let before = Leds::uniform(LedColour::Green, true);
        for (led, choice) in [
            (EditorLed::Bi(BiColourLed::ButtonA), LedChoice::On),
            (EditorLed::Single(SingleColourLed::Fire), LedChoice::Red),
            (EditorLed::Pov1, LedChoice::Off),
        ] {
            let mut leds = before;
            assert!(!set_choice(&mut leds, led, choice), "{led:?} {choice:?}");
            assert_eq!(leds, before);
        }
    }

    #[test]
    fn choices_have_stable_ids() {
        for choice in LedChoice::ALL {
            assert_eq!(LedChoice::from_id(choice.id()), Some(choice));
        }
        assert_eq!(LedChoice::from_id("blue"), None);
    }

    #[test]
    fn labels_name_the_led_and_its_state() {
        let led = EditorLed::Bi(BiColourLed::ButtonA);
        assert_eq!(
            accessible_label(led, LedChoice::Red, Lighting::Lit),
            "A LED: Red"
        );
        assert_eq!(
            accessible_label(led, LedChoice::Red, Lighting::Dark),
            "A LED: Red, brightness 0"
        );
        assert_eq!(
            accessible_label(EditorLed::Pov1, LedChoice::Green, Lighting::Dark),
            "POV 1 LED: Green, fixed, brightness 0"
        );
        assert_eq!(tooltip(led, LedChoice::Red), "A LED: Red");
    }

    #[test]
    fn brightness_zero_shows_every_led_off_and_keeps_the_states() {
        let lit = pro_state(128, false);
        let dark = pro_state(0, false);
        for led in EditorLed::ALL {
            let before = led_view(&lit, led);
            let after = led_view(&dark, led);
            assert_eq!(after.class, "led-off", "{led:?}");
            assert_eq!(after.choice, before.choice, "{led:?}");
            assert!(after.label.ends_with(", brightness 0"), "{led:?}");
        }
        assert_eq!(
            brightness_note(&dark).as_deref(),
            Some("Brightness is 0: every LED is off. The states are kept.")
        );
    }

    #[test]
    fn brightness_above_zero_brings_the_states_back() {
        let lit = pro_state(64, false);
        for led in EditorLed::ALL {
            let view = led_view(&lit, led);
            assert_eq!(view.class, view.choice.css_class(), "{led:?}");
            assert!(!view.label.contains("brightness 0"), "{led:?}");
        }
        assert_eq!(brightness_note(&lit), None);
    }

    #[test]
    fn blink_blinks_i_and_pov_1_only_while_lit() {
        let button_i = EditorLed::Bi(BiColourLed::ButtonI);
        let blinking = pro_state(128, true);
        assert!(led_view(&blinking, button_i).blinking);
        assert!(led_view(&blinking, EditorLed::Pov1).blinking);
        for led in EditorLed::ALL {
            if led != button_i && led != EditorLed::Pov1 {
                assert!(!led_view(&blinking, led).blinking, "{led:?}");
            }
        }

        let mut i_off = blinking.clone();
        set_choice(&mut i_off.settings.leds, button_i, LedChoice::Off);
        assert!(!led_view(&i_off, button_i).blinking);
        assert!(led_view(&i_off, EditorLed::Pov1).blinking);

        let dark = pro_state(0, true);
        assert!(!led_view(&dark, button_i).blinking);
        assert!(!led_view(&dark, EditorLed::Pov1).blinking);

        let still = pro_state(128, false);
        assert!(!led_view(&still, button_i).blinking);
        assert!(!led_view(&still, EditorLed::Pov1).blinking);
    }

    #[test]
    fn the_x52_has_only_the_brightness_with_a_note() {
        let mut state = pro_state(128, true);
        state.device_type = DeviceType::X52;
        assert_eq!(
            brightness_note(&state).as_deref(),
            Some("This model has no LEDs that can be set one by one.")
        );
        assert!(!led_view(&state, EditorLed::Pov1).blinking);
    }
}
