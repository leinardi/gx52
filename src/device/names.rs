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

//! The names of the joystick's controls (docs/protocol.md, "Buttons" and "Hats and names"): as the
//! MFD's "last button" line shows them, and as the input tester labels its indicators.
//!
//! Each MFD name fits an MFD line: at most 16 characters in English, shortened where the full
//! name is longer ("Trigger stage 1", "Thr. hat left"). A longer translation is cut to 16 bytes
//! when sent, as every dynamic line is. The input tester shows a shorter label on each indicator
//! and the full name in its tooltip (docs/ui.md, "Input tester").

use gettextrs::gettext;
use x52::input::{Button, Input, Pov1Direction};

use crate::profile;

/// The short name of `input`, translated: "Trigger stage 1", "POV 1 up".
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
        // Translators: shown on the joystick's 16-character display, like every name here; a
        // longer name is cut. The trigger's first stage.
        Button::TriggerStage1 => gettext("Trigger stage 1"),
        // Translators: the trigger pulled all the way, its second stage.
        Button::TriggerStage2 => gettext("Trigger stage 2"),
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
        // Translators: the throttle's scroll wheel, one notch down.
        Button::ScrollDown => gettext("Scroll down"),
        // Translators: the throttle's scroll wheel, one notch up.
        Button::ScrollUp => gettext("Scroll up"),
        // Translators: the throttle's scroll wheel pressed.
        Button::ScrollPress => gettext("Scroll press"),
        Button::Pov2Up => gettext("POV 2 up"),
        Button::Pov2Right => gettext("POV 2 right"),
        Button::Pov2Down => gettext("POV 2 down"),
        Button::Pov2Left => gettext("POV 2 left"),
        // Translators: the throttle hat pushed up; "Thr." is short for throttle.
        Button::ThrottleHatUp => gettext("Thr. hat up"),
        // Translators: the throttle hat pushed right.
        Button::ThrottleHatRight => gettext("Thr. hat right"),
        // Translators: the throttle hat pushed down.
        Button::ThrottleHatDown => gettext("Thr. hat down"),
        // Translators: the throttle hat pushed left.
        Button::ThrottleHatLeft => gettext("Thr. hat left"),
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

/// The label of `button` on its input tester indicator, translated: "Trigger", "Wheel up".
pub(crate) fn tester_label(button: Button) -> String {
    match button {
        // Translators: a label on the input tester's indicator for the trigger's first stage;
        // the labels there are short, the full name is in the tooltip.
        Button::TriggerStage1 => gettext("Trigger"),
        // Translators: the trigger's second stage, on its input tester indicator.
        Button::TriggerStage2 => gettext("Trigger 2"),
        // Translators: the pinkie switch, on its input tester indicator.
        Button::Pinkie => gettext("Pinkie"),
        // Translators: the mouse button next to the ministick, on its input tester indicator.
        Button::MouseButton => gettext("Mouse"),
        // Translators: the throttle's scroll wheel turned one notch down, on its input tester
        // indicator.
        Button::ScrollDown => gettext("Wheel down"),
        // Translators: the throttle's scroll wheel turned one notch up.
        Button::ScrollUp => gettext("Wheel up"),
        // Translators: the throttle's scroll wheel pressed.
        Button::ScrollPress => gettext("Wheel press"),
        // Translators: the throttle hat pushed up, on its input tester indicator.
        Button::ThrottleHatUp => gettext("Hat up"),
        // Translators: the throttle hat pushed right.
        Button::ThrottleHatRight => gettext("Hat right"),
        // Translators: the throttle hat pushed down.
        Button::ThrottleHatDown => gettext("Hat down"),
        // Translators: the throttle hat pushed left.
        Button::ThrottleHatLeft => gettext("Hat left"),
        // Translators: the i button on the throttle, on its input tester indicator.
        Button::Clutch => gettext("i"),
        Button::Fire
        | Button::ButtonA
        | Button::ButtonB
        | Button::ButtonC
        | Button::ButtonD
        | Button::ButtonE
        | Button::T1
        | Button::T2
        | Button::T3
        | Button::T4
        | Button::T5
        | Button::T6
        | Button::Pov2Up
        | Button::Pov2Right
        | Button::Pov2Down
        | Button::Pov2Left
        | Button::Mode1
        | Button::Mode2
        | Button::Mode3
        | Button::Function
        | Button::StartStop
        | Button::Reset
        | Button::PageUp
        | Button::PageDown
        | Button::Up
        | Button::Down
        | Button::Select => button_name(button),
    }
}

/// The full name of `button`, translated, for the input tester's tooltips: "Trigger, first
/// stage", "Scroll wheel down".
pub(crate) fn full_name(button: Button) -> String {
    match button {
        Button::TriggerStage1 => gettext("Trigger, first stage"),
        Button::TriggerStage2 => gettext("Trigger, second stage"),
        Button::ScrollDown => gettext("Scroll wheel down"),
        Button::ScrollUp => gettext("Scroll wheel up"),
        Button::ScrollPress => gettext("Scroll wheel press"),
        Button::ThrottleHatUp => gettext("Throttle hat up"),
        Button::ThrottleHatRight => gettext("Throttle hat right"),
        Button::ThrottleHatDown => gettext("Throttle hat down"),
        Button::ThrottleHatLeft => gettext("Throttle hat left"),
        // Translators: the left MFD wheel, which pressed is Function, turned up.
        Button::PageUp => gettext("Function wheel up"),
        // Translators: the left MFD wheel turned down.
        Button::PageDown => gettext("Function wheel down"),
        // Translators: the right MFD wheel, which pressed is Select, turned up.
        Button::Up => gettext("Select wheel up"),
        // Translators: the right MFD wheel turned down.
        Button::Down => gettext("Select wheel down"),
        Button::Fire
        | Button::ButtonA
        | Button::ButtonB
        | Button::ButtonC
        | Button::Pinkie
        | Button::ButtonD
        | Button::ButtonE
        | Button::T1
        | Button::T2
        | Button::T3
        | Button::T4
        | Button::T5
        | Button::T6
        | Button::MouseButton
        | Button::Pov2Up
        | Button::Pov2Right
        | Button::Pov2Down
        | Button::Pov2Left
        | Button::Mode1
        | Button::Mode2
        | Button::Mode3
        | Button::Clutch
        | Button::Function
        | Button::StartStop
        | Button::Reset
        | Button::Select => button_name(button),
    }
}

#[cfg(test)]
mod tests;
