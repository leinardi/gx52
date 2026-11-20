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

//! The hold of a wheel notch on the MFD's "Last button" line (docs/profiles.md, "Control names"):
//! a notch is a press and a release a few milliseconds apart, too short to read, so its name stays
//! for [`NOTCH_HOLD`] unless another control is pressed first.
//!
//! The hold is driven by the kind of change, never by comparing held sets: a snapshot, sent each
//! time a node opens, says what is held but is not a press.

use std::time::Duration;

use x52::input::{HeldInputs, Input, Pov1Direction};

use super::reader::HeldChange;

/// How long the "Last button" line keeps a wheel notch.
pub(crate) const NOTCH_HOLD: Duration = Duration::from_millis(700);

/// What a change did to the hold's timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldTimer {
    /// A notch was pressed: the timer starts, or starts again.
    Arm,
    /// The hold ended: the timer stops.
    Cancel,
    /// The timer runs on, or there is none.
    Keep,
}

/// The wheel notch the "Last button" line holds, if any.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct NotchHold {
    /// The notch held on the line.
    notch: Option<Input>,
}

impl NotchHold {
    /// Applies `change`, about to be applied to `held`, to the hold. A notch pressed arms it;
    /// any other press ends it: a key, or a POV 1 direction that was not held before (going from
    /// Up+Right to Up is a release). A release keeps it, and a snapshot ends it without arming it.
    pub(crate) fn apply(&mut self, change: &HeldChange, held: &HeldInputs) -> HoldTimer {
        match (change, pressed_input(change, held)) {
            (HeldChange::Snapshot(_), _) => self.cancel(),
            (_, Some(Input::Button(button))) if button.is_wheel_notch() => {
                self.notch = Some(Input::Button(button));
                HoldTimer::Arm
            }
            (_, Some(_)) => self.cancel(),
            (_, None) => HoldTimer::Keep,
        }
    }

    /// Ends the hold: its time is up, or the held inputs are cleared.
    pub(crate) fn cancel(&mut self) -> HoldTimer {
        self.notch = None;
        HoldTimer::Cancel
    }

    /// The input the "Last button" line shows: the notch held, else the last input held.
    #[must_use]
    pub(crate) fn shown(self, held: &HeldInputs) -> Option<Input> {
        self.notch.or_else(|| held.last())
    }
}

/// The input `change`, about to be applied to `held`, presses: the key pressed, or the first POV
/// 1 direction (in [`Pov1Direction::ALL`] order) that `held` does not hold yet. `None` for a
/// release and for a snapshot, which says what is held but is not a press.
pub(crate) fn pressed_input(change: &HeldChange, held: &HeldInputs) -> Option<Input> {
    match change {
        HeldChange::Key {
            button,
            pressed: true,
        } => Some(Input::Button(*button)),
        HeldChange::Pov1 { x, y } => {
            Pov1Direction::ALL
                .into_iter()
                .map(Input::Pov1)
                .find(|input| {
                    matches!(input, Input::Pov1(direction) if direction.is_held(*x, *y))
                        && !held.is_held(*input)
                })
        }
        HeldChange::Key { pressed: false, .. } | HeldChange::Snapshot(_) => None,
    }
}

#[cfg(test)]
mod tests;
