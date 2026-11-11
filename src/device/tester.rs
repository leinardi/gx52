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

//! What the input tester shows (docs/ui.md, "Input tester"): its groups of indicators, the
//! direction grids of the hats, the tooltips and the text a screen reader reads.
//!
//! Everything here is in terms of controls ([`Button`], [`Pov1Direction`]), never evdev codes:
//! the same code is another control on another device type (718 is the i button on the X52 Pro
//! and the mouse button on the X52), so the codes and numbers come from the device's
//! [`InputMap`] only.

use std::time::Duration;

use gettextrs::gettext;
use x52::input::{Button, HeldInputs, Input, InputMap, Mode, Pov1Direction};

use super::names;
use crate::profile;

/// A group of buttons on the tester's Buttons card, by part of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ButtonGroup {
    /// The buttons of the stick.
    Stick,
    /// The buttons of the throttle.
    Throttle,
    /// The MFD's buttons and wheels.
    Mfd,
}

impl ButtonGroup {
    /// Every group, in the order the card shows them.
    pub(crate) const ALL: [Self; 3] = [Self::Stick, Self::Throttle, Self::Mfd];

    /// The subheading of the group, translated.
    pub(crate) fn title(self) -> String {
        match self {
            Self::Stick => gettext("Stick"),
            Self::Throttle => gettext("Throttle"),
            // Translators: the multi-function display and its buttons.
            Self::Mfd => gettext("MFD"),
        }
    }

    /// The buttons of the group, in the order they are shown; a device may lack some.
    pub(crate) const fn buttons(self) -> &'static [Button] {
        match self {
            Self::Stick => &[
                Button::TriggerStage1,
                Button::TriggerStage2,
                Button::Fire,
                Button::ButtonA,
                Button::ButtonB,
                Button::ButtonC,
                Button::Pinkie,
            ],
            Self::Throttle => &[
                Button::ButtonD,
                Button::ButtonE,
                Button::Clutch,
                Button::MouseButton,
                Button::ScrollUp,
                Button::ScrollDown,
                Button::ScrollPress,
            ],
            Self::Mfd => &[
                Button::Function,
                Button::StartStop,
                Button::Reset,
                Button::PageUp,
                Button::PageDown,
                Button::Up,
                Button::Down,
                Button::Select,
            ],
        }
    }
}

/// The toggle switches in the order of their 3-column grid: the up positions, then the down ones.
pub(crate) const TOGGLES: [Button; 6] = [
    Button::T1,
    Button::T3,
    Button::T5,
    Button::T2,
    Button::T4,
    Button::T6,
];

/// A hat, shown as a 3×3 grid of directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hat {
    /// POV 1, the hat that is a pair of axes.
    Pov1,
    /// POV 2, four buttons.
    Pov2,
    /// The throttle hat, four buttons.
    Throttle,
}

impl Hat {
    /// Every hat, in the order the POVs card shows them.
    pub(crate) const ALL: [Self; 3] = [Self::Pov1, Self::Pov2, Self::Throttle];

    /// The caption under the grid, translated.
    pub(crate) fn title(self) -> String {
        match self {
            Self::Pov1 => gettext("POV 1"),
            Self::Pov2 => gettext("POV 2"),
            Self::Throttle => gettext("Throttle hat"),
        }
    }

    /// The input of `direction` on this hat.
    pub(crate) const fn input(self, direction: Pov1Direction) -> Input {
        match (self, direction) {
            (Self::Pov1, direction) => Input::Pov1(direction),
            (Self::Pov2, Pov1Direction::Up) => Input::Button(Button::Pov2Up),
            (Self::Pov2, Pov1Direction::Right) => Input::Button(Button::Pov2Right),
            (Self::Pov2, Pov1Direction::Down) => Input::Button(Button::Pov2Down),
            (Self::Pov2, Pov1Direction::Left) => Input::Button(Button::Pov2Left),
            (Self::Throttle, Pov1Direction::Up) => Input::Button(Button::ThrottleHatUp),
            (Self::Throttle, Pov1Direction::Right) => Input::Button(Button::ThrottleHatRight),
            (Self::Throttle, Pov1Direction::Down) => Input::Button(Button::ThrottleHatDown),
            (Self::Throttle, Pov1Direction::Left) => Input::Button(Button::ThrottleHatLeft),
        }
    }

    /// Whether the device has the hat: POV 1 when the map has its axes, a button hat when it has
    /// its four buttons.
    pub(crate) fn is_on(self, map: &InputMap) -> bool {
        Pov1Direction::ALL
            .into_iter()
            .all(|direction| has_input(map, self.input(direction)))
    }

    /// The cell of the 3×3 grid the hat points at, as (column, row) from the top left; (1, 1)
    /// when centred. Opposite directions held together cancel out.
    pub(crate) fn cell(self, held: &HeldInputs) -> (i32, i32) {
        let on = |direction| i32::from(held.is_held(self.input(direction)));
        (
            1 + on(Pov1Direction::Right) - on(Pov1Direction::Left),
            1 + on(Pov1Direction::Down) - on(Pov1Direction::Up),
        )
    }
}

/// Whether `map` has `input`.
fn has_input(map: &InputMap, input: Input) -> bool {
    match input {
        Input::Button(button) => map.code_of(button).is_some(),
        Input::Pov1(_) => {
            map.has_axis(x52::input::Axis::Pov1X) && map.has_axis(x52::input::Axis::Pov1Y)
        }
    }
}

/// The buttons of `group` that the device has, with their evdev code.
pub(crate) fn group_buttons(group: ButtonGroup, map: &InputMap) -> Vec<(Button, u16)> {
    present(group.buttons(), map)
}

/// The toggle switches the device has, in grid order, with their evdev code.
pub(crate) fn toggles(map: &InputMap) -> Vec<(Button, u16)> {
    present(&TOGGLES, map)
}

fn present(buttons: &[Button], map: &InputMap) -> Vec<(Button, u16)> {
    buttons
        .iter()
        .filter_map(|button| Some((*button, map.code_of(*button)?)))
        .collect()
}

/// The tooltip of the indicator of the button with evdev `code`: "Button 2 · Fire · evdev code
/// 289".
pub(crate) fn tooltip(map: &InputMap, button: Button, code: u16) -> String {
    let number = map.button_number(code).unwrap_or_default();
    // Translators: the tooltip of a button on the input tester: the number games show for it,
    // its name and its Linux input event code.
    gettext("Button {number} · {name} · evdev code {code}")
        .replace("{number}", &number.to_string())
        .replace("{name}", &names::full_name(button))
        .replace("{code}", &code.to_string())
}

/// What a screen reader reads for the held inputs and the mode: "Buttons pressed: Fire, A.
/// Mode 2." Inputs are listed in the order the page shows them.
pub(crate) fn summary(map: &InputMap, held: &HeldInputs, mode: Option<Mode>) -> String {
    let buttons = ButtonGroup::ALL
        .into_iter()
        .flat_map(|group| group.buttons().iter().copied())
        .chain(TOGGLES)
        .map(Input::Button)
        .chain(
            Hat::ALL
                .into_iter()
                .flat_map(|hat| Pov1Direction::ALL.map(|direction| hat.input(direction))),
        )
        .filter(|input| has_input(map, *input) && held.is_held(*input))
        .map(input_full_name)
        .collect::<Vec<_>>();
    let pressed = if buttons.is_empty() {
        // Translators: read by a screen reader on the input tester when no button is held.
        gettext("Buttons pressed: none.")
    } else {
        // Translators: read by a screen reader on the input tester; {buttons} is a list of
        // button names separated by commas.
        gettext("Buttons pressed: {buttons}.").replace("{buttons}", &buttons.join(", "))
    };
    match mode {
        Some(mode) => format!("{pressed} {}.", profile::mode_text(mode)),
        None => pressed,
    }
}

/// The full name of `input`.
fn input_full_name(input: Input) -> String {
    match input {
        Input::Button(button) => names::full_name(button),
        Input::Pov1(_) => names::input_name(input),
    }
}

/// How long an indicator stays lit at least: a wheel notch is a press and a release a few
/// milliseconds apart, which would otherwise never show.
pub(crate) const MIN_LIT: Duration = Duration::from_millis(150);

/// Whether an indicator is lit: while its input is held, and for [`MIN_LIT`] at least.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Pulse {
    /// Whether the input is held, as last shown.
    held: bool,
    /// Since when the indicator is lit (monotonic time), while it is.
    lit_since: Option<Duration>,
}

/// What an indicator does after [`Pulse::show`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PulseStep {
    /// Nothing changes.
    Keep,
    /// It lights up.
    Light,
    /// It goes out now.
    PutOut,
    /// It stays lit and goes out after this long, unless pressed again: [`Pulse::expire`].
    PutOutIn(Duration),
}

impl Pulse {
    /// The input is `held` or not at monotonic time `now`.
    pub(crate) fn show(&mut self, held: bool, now: Duration) -> PulseStep {
        if self.held == held {
            return PulseStep::Keep;
        }
        self.held = held;
        if held {
            if self.lit_since.is_some() {
                return PulseStep::Keep;
            }
            self.lit_since = Some(now);
            return PulseStep::Light;
        }
        let Some(since) = self.lit_since else {
            return PulseStep::Keep;
        };
        match MIN_LIT.checked_sub(now.saturating_sub(since)) {
            Some(left) if !left.is_zero() => PulseStep::PutOutIn(left),
            Some(_) | None => {
                self.lit_since = None;
                PulseStep::PutOut
            }
        }
    }

    /// The time of a [`PulseStep::PutOutIn`] has come: true when the indicator goes out, false
    /// when the input was pressed again meanwhile.
    pub(crate) fn expire(&mut self) -> bool {
        if self.held || self.lit_since.is_none() {
            return false;
        }
        self.lit_since = None;
        true
    }

    /// Unlit and not held: the page is no longer shown.
    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests;
