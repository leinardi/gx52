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

//! The virtual mouse: the ministick and the throttle's mouse buttons drive a uinput pointer
//! (docs/protocol.md, "Virtual mouse (uinput)"; docs/architecture.md, "Virtual-mouse loop").
//!
//! The speed formula, the sub-pixel carry and the button mapping come from spike S9
//! (`spikes/s9-virtual-mouse/src/main.rs` on the `spikes` branch, at 345116e), whose values are
//! the defaults of the five settings.

use evdev::{EventType, KeyCode, RelativeAxisCode, SynchronizationCode};
use x52::input::Button;

/// The ministick's centre: both axes report `0..=15` (4 bits in the HID report).
pub(crate) const MINISTICK_CENTRE: i32 = 8;

/// How many `REL_WHEEL_HI_RES` units one wheel notch is (the kernel's convention).
const HI_RES_PER_NOTCH: i32 = 120;

/// Which way a wheel notch scrolls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum WheelDirection {
    /// Wheel up scrolls up, as on a mouse.
    #[default]
    Natural,
    /// Wheel up scrolls down (Invert Wheel).
    Inverted,
}

/// The five settings of the virtual mouse other than the switch, as the loop applies them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Tuning {
    /// Pixels per tick at one step outside the deadzone (Speed).
    pub(crate) gain: f64,
    /// The exponent of the offset past the deadzone (Curve).
    pub(crate) curve: f64,
    /// Offsets up to this many steps from the centre do not move the pointer (Deadzone).
    pub(crate) deadzone: f64,
    /// Which way the wheel scrolls (Invert Wheel).
    pub(crate) wheel: WheelDirection,
}

impl Default for Tuning {
    /// The schema's defaults: the spike's values.
    fn default() -> Self {
        Self {
            gain: 0.6,
            curve: 1.6,
            deadzone: 1.0,
            wheel: WheelDirection::Natural,
        }
    }
}

impl Tuning {
    /// The pointer speed, in pixels per tick, for a ministick `offset` in steps from the centre:
    /// `sign × gain × (|offset| − deadzone) ^ curve`, and 0 inside the deadzone.
    #[must_use]
    pub(crate) fn speed(&self, offset: f64) -> f64 {
        let past_deadzone = offset.abs() - self.deadzone;
        if past_deadzone <= 0.0 {
            0.0
        } else {
            offset.signum() * self.gain * past_deadzone.powf(self.curve)
        }
    }

    /// Whether a ministick `offset` lies outside the deadzone, so the pointer moves.
    #[must_use]
    pub(crate) fn moves(&self, offset: i32) -> bool {
        f64::from(offset).abs() > self.deadzone
    }
}

/// The motion of one pointer axis: whole pixels out, the fraction carried to the next tick.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct AxisMotion {
    carry: f64,
}

impl AxisMotion {
    /// The whole pixels to move this tick for a ministick `offset` from the centre. Fractions
    /// carry over, so slow speeds still move; back at the centre the carry is dropped, so the
    /// pointer stops dead.
    pub(crate) fn step(&mut self, offset: i32, tuning: &Tuning) -> i32 {
        if offset == 0 {
            self.carry = 0.0;
            return 0;
        }
        let total = self.carry + tuning.speed(f64::from(offset));
        let whole = total.trunc();
        self.carry = total - whole;
        // The bounds keep the cast exact; no setting comes near them (3 × 8^3 px at most).
        #[expect(
            clippy::cast_possible_truncation,
            reason = "clamped to the i32 range first, and trunc() left no fraction"
        )]
        let pixels = whole.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32;
        pixels
    }

    /// Forgets the carry, as when the pointer is created again.
    pub(crate) fn reset(&mut self) {
        self.carry = 0.0;
    }
}

/// A button of the virtual mouse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PointerButton {
    /// `BTN_LEFT`, from the mouse button next to the ministick.
    Left,
    /// `BTN_RIGHT`, from the scroll wheel press.
    Right,
}

impl PointerButton {
    /// Both buttons, in the order they are released.
    pub(crate) const ALL: [Self; 2] = [Self::Left, Self::Right];

    const fn key(self) -> KeyCode {
        match self {
            Self::Left => KeyCode::BTN_LEFT,
            Self::Right => KeyCode::BTN_RIGHT,
        }
    }
}

/// One thing the virtual mouse reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointerEvent {
    /// Move by `dx`, `dy` pixels.
    Motion {
        /// Right is positive.
        dx: i32,
        /// Down is positive.
        dy: i32,
    },
    /// A button went down or up.
    Button {
        /// The button.
        button: PointerButton,
        /// Down, or up.
        pressed: bool,
    },
    /// The wheel turned by `notches`: positive scrolls up.
    Wheel {
        /// Notches, positive up.
        notches: i32,
    },
}

/// What a joystick button does on the virtual mouse, or `None` when it does nothing: the mouse
/// button and the wheel press are the left and right buttons, a wheel notch (its press only) one
/// wheel step.
#[must_use]
pub(crate) fn pointer_event(
    button: Button,
    pressed: bool,
    tuning: &Tuning,
) -> Option<PointerEvent> {
    let notch = |up: i32| {
        pressed.then_some(PointerEvent::Wheel {
            notches: match tuning.wheel {
                WheelDirection::Natural => up,
                WheelDirection::Inverted => -up,
            },
        })
    };
    match button {
        Button::MouseButton => Some(PointerEvent::Button {
            button: PointerButton::Left,
            pressed,
        }),
        Button::ScrollPress => Some(PointerEvent::Button {
            button: PointerButton::Right,
            pressed,
        }),
        Button::ScrollUp => notch(1),
        Button::ScrollDown => notch(-1),
        Button::TriggerStage1
        | Button::TriggerStage2
        | Button::Fire
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
        | Button::Pov2Up
        | Button::Pov2Right
        | Button::Pov2Down
        | Button::Pov2Left
        | Button::ThrottleHatUp
        | Button::ThrottleHatRight
        | Button::ThrottleHatDown
        | Button::ThrottleHatLeft
        | Button::Mode1
        | Button::Mode2
        | Button::Mode3
        | Button::Clutch
        | Button::Function
        | Button::StartStop
        | Button::Reset
        | Button::PageUp
        | Button::PageDown
        | Button::Up
        | Button::Down
        | Button::Select => None,
    }
}

/// Whether `button` is one of the four the virtual mouse uses.
#[must_use]
pub(crate) fn is_mouse_button(button: Button) -> bool {
    pointer_event(button, true, &Tuning::default()).is_some()
}

/// One uinput event: its type, code and value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RawEvent {
    /// The evdev event type.
    pub(crate) event_type: u16,
    /// The code within the type.
    pub(crate) code: u16,
    /// The value.
    pub(crate) value: i32,
}

impl RawEvent {
    const fn relative(axis: RelativeAxisCode, value: i32) -> Self {
        Self {
            event_type: EventType::RELATIVE.0,
            code: axis.0,
            value,
        }
    }

    /// The `SYN_REPORT` that ends a batch.
    pub(crate) const SYN_REPORT: Self = Self {
        event_type: EventType::SYNCHRONIZATION.0,
        code: SynchronizationCode::SYN_REPORT.0,
        value: 0,
    };
}

/// The uinput events of `events`, ending with `SYN_REPORT`; empty when there is nothing to report
/// (a motion of 0, 0 alone). `REL_X` and `REL_Y` appear only when non-zero.
#[must_use]
pub(crate) fn encode(events: &[PointerEvent]) -> Vec<RawEvent> {
    let mut raw = Vec::new();
    for event in events {
        match *event {
            PointerEvent::Motion { dx, dy } => {
                if dx != 0 {
                    raw.push(RawEvent::relative(RelativeAxisCode::REL_X, dx));
                }
                if dy != 0 {
                    raw.push(RawEvent::relative(RelativeAxisCode::REL_Y, dy));
                }
            }
            PointerEvent::Button { button, pressed } => raw.push(RawEvent {
                event_type: EventType::KEY.0,
                code: button.key().code(),
                value: i32::from(pressed),
            }),
            PointerEvent::Wheel { notches } => {
                raw.push(RawEvent::relative(RelativeAxisCode::REL_WHEEL, notches));
                raw.push(RawEvent::relative(
                    RelativeAxisCode::REL_WHEEL_HI_RES,
                    notches.saturating_mul(HI_RES_PER_NOTCH),
                ));
            }
        }
    }
    if !raw.is_empty() {
        raw.push(RawEvent::SYN_REPORT);
    }
    raw
}

#[cfg(test)]
mod tests;
