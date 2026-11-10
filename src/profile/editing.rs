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

//! The mode the editor shows and the device shows: the one picked in the mode bar, else the one
//! the selector on the stick is at (docs/ui.md, "Mode bar").

use x52::input::Mode;

/// The mode picked in the mode bar, kept until the selector moves, the active profile changes or
/// becomes single, or the device goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct EditingMode {
    picked: Option<Mode>,
}

impl EditingMode {
    /// The mode shown when the selector is at `active`: the picked one, else `active`; `None`
    /// while neither is known, when mode 1 is shown.
    pub(crate) const fn shown(self, active: Option<Mode>) -> Option<Mode> {
        match self.picked {
            Some(mode) => Some(mode),
            None => active,
        }
    }

    /// Picks `mode` in the bar while the selector is at `active`. Picking the mode the selector
    /// shows anyway drops the pick, so the selector takes over again.
    pub(crate) fn pick(&mut self, mode: Mode, active: Option<Mode>) {
        self.picked = (mode != active.unwrap_or(Mode::One)).then_some(mode);
    }

    /// Drops the pick: the selector moved, the active profile changed or became single, or the
    /// device went.
    pub(crate) const fn clear(&mut self) {
        self.picked = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pick_shows_its_mode_while_the_selector_stays() {
        let mut editing = EditingMode::default();
        editing.pick(Mode::Two, Some(Mode::One));
        assert_eq!(editing.shown(Some(Mode::One)), Some(Mode::Two));
    }

    #[test]
    fn without_a_pick_the_selector_is_shown() {
        let editing = EditingMode::default();
        assert_eq!(editing.shown(Some(Mode::Three)), Some(Mode::Three));
        assert_eq!(editing.shown(None), None);
    }

    #[test]
    fn the_selector_moving_takes_over() {
        let mut editing = EditingMode::default();
        editing.pick(Mode::Two, Some(Mode::One));
        // The application clears the pick when the selector reports a new mode.
        editing.clear();
        assert_eq!(editing.shown(Some(Mode::Three)), Some(Mode::Three));
    }

    #[test]
    fn picking_the_selectors_mode_drops_the_pick() {
        let mut editing = EditingMode::default();
        editing.pick(Mode::Two, Some(Mode::One));
        editing.pick(Mode::One, Some(Mode::One));
        assert_eq!(editing, EditingMode::default());
        // Mode 1 is shown while the selector is unknown.
        editing.pick(Mode::One, None);
        assert_eq!(editing, EditingMode::default());
        editing.pick(Mode::Three, None);
        assert_eq!(editing.shown(None), Some(Mode::Three));
    }
}
