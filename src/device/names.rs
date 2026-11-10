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

//! The names of the joystick's controls, as the app shows them (docs/protocol.md, "Buttons" and
//! "Hats and names").

use gettextrs::gettext;
use x52::input::{Button, Input, Pov1Direction};

use crate::profile;

/// The name of `input`, translated: "Trigger, first stage", "POV 1 up".
pub(crate) fn input_name(input: Input) -> String {
    match input {
        Input::Button(button) => button_name(button),
        Input::Pov1(Pov1Direction::Up) => gettext("POV 1 up"),
        Input::Pov1(Pov1Direction::Right) => gettext("POV 1 right"),
        Input::Pov1(Pov1Direction::Down) => gettext("POV 1 down"),
        Input::Pov1(Pov1Direction::Left) => gettext("POV 1 left"),
    }
}

/// The name of `button`, translated.
fn button_name(button: Button) -> String {
    match button {
        Button::TriggerStage1 => gettext("Trigger, first stage"),
        Button::TriggerStage2 => gettext("Trigger, second stage"),
        Button::Fire => gettext("Fire"),
        // Translators: the A button of the joystick.
        Button::ButtonA => gettext("A"),
        // Translators: the B button of the joystick.
        Button::ButtonB => gettext("B"),
        // Translators: the C button of the joystick.
        Button::ButtonC => gettext("C"),
        Button::Pinkie => gettext("Pinkie switch"),
        // Translators: the D button of the joystick.
        Button::ButtonD => gettext("D"),
        // Translators: the E button of the joystick.
        Button::ButtonE => gettext("E"),
        // Translators: toggle switch 1 up, labelled T1 on the throttle.
        Button::T1 => gettext("T1"),
        // Translators: toggle switch 1 down, labelled T2 on the throttle.
        Button::T2 => gettext("T2"),
        // Translators: toggle switch 2 up, labelled T3 on the throttle.
        Button::T3 => gettext("T3"),
        // Translators: toggle switch 2 down, labelled T4 on the throttle.
        Button::T4 => gettext("T4"),
        // Translators: toggle switch 3 up, labelled T5 on the throttle.
        Button::T5 => gettext("T5"),
        // Translators: toggle switch 3 down, labelled T6 on the throttle.
        Button::T6 => gettext("T6"),
        Button::MouseButton => gettext("Mouse button"),
        Button::ScrollDown => gettext("Scroll wheel down"),
        Button::ScrollUp => gettext("Scroll wheel up"),
        Button::ScrollPress => gettext("Scroll wheel press"),
        Button::Pov2Up => gettext("POV 2 up"),
        Button::Pov2Right => gettext("POV 2 right"),
        Button::Pov2Down => gettext("POV 2 down"),
        Button::Pov2Left => gettext("POV 2 left"),
        Button::ThrottleHatUp => gettext("Throttle hat up"),
        Button::ThrottleHatRight => gettext("Throttle hat right"),
        Button::ThrottleHatDown => gettext("Throttle hat down"),
        Button::ThrottleHatLeft => gettext("Throttle hat left"),
        // The mode selector positions are never a "last button"; named as the MFD's mode line.
        Button::Mode1 => profile::mode_text(x52::input::Mode::One),
        Button::Mode2 => profile::mode_text(x52::input::Mode::Two),
        Button::Mode3 => profile::mode_text(x52::input::Mode::Three),
        // Translators: the i button on the throttle, also called the clutch.
        Button::Clutch => gettext("i (clutch)"),
        Button::Function => gettext("Function"),
        Button::StartStop => gettext("Start/Stop"),
        Button::Reset => gettext("Reset"),
        Button::PageUp => gettext("Page Up"),
        Button::PageDown => gettext("Page Down"),
        // Translators: the right MFD wheel turned up.
        Button::Up => gettext("Up"),
        // Translators: the right MFD wheel turned down.
        Button::Down => gettext("Down"),
        Button::Select => gettext("Select"),
    }
}

#[cfg(test)]
mod tests;
