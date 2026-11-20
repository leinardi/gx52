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

//! The custom control names of the Names view (docs/ui.md, "Names view"): the groups of controls,
//! the texts that sum the names up, and the bookkeeping that keeps an Undo of Copy and Clear from
//! overwriting a later edit.

use std::collections::BTreeMap;

use gettextrs::{gettext, ngettext};
use x52::input::{Button, Input, InputMap, Mode, Pov1Direction};

use super::mfd::SettingsSet;
use super::{ProfileId, mode_index};
use crate::device::tester;

/// A group of nameable controls, by part of the device, as the Names view lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameGroup {
    /// The buttons of the stick.
    Stick,
    /// The buttons of the throttle.
    Throttle,
    /// The three toggle switches, up and down.
    Toggles,
    /// The lower hat of the stick, a pair of axes.
    Pov1,
    /// The upper hat of the stick.
    Pov2,
    /// The hat of the throttle.
    ThrottleHat,
    /// The MFD's buttons and wheels.
    Mfd,
}

impl NameGroup {
    /// Every group, in the order the view shows them.
    pub(crate) const ALL: [Self; 7] = [
        Self::Stick,
        Self::Throttle,
        Self::Toggles,
        Self::Pov1,
        Self::Pov2,
        Self::ThrottleHat,
        Self::Mfd,
    ];

    /// The title of the group's row, translated.
    pub(crate) fn title(self) -> String {
        match self {
            Self::Stick => gettext("Stick"),
            Self::Throttle => gettext("Throttle"),
            Self::Toggles => gettext("Toggle Switches"),
            Self::Pov1 => gettext("POV 1"),
            Self::Pov2 => gettext("POV 2"),
            Self::ThrottleHat => gettext("Throttle Hat"),
            // Translators: the multi-function display and its buttons.
            Self::Mfd => gettext("MFD"),
        }
    }

    /// Every control of the group, in the order it is shown; a device type may lack some.
    pub(crate) const fn controls(self) -> &'static [Input] {
        match self {
            Self::Stick => &[
                Input::Button(Button::TriggerStage1),
                Input::Button(Button::TriggerStage2),
                Input::Button(Button::Fire),
                Input::Button(Button::ButtonA),
                Input::Button(Button::ButtonB),
                Input::Button(Button::ButtonC),
                Input::Button(Button::Pinkie),
            ],
            Self::Throttle => &[
                Input::Button(Button::ButtonD),
                Input::Button(Button::ButtonE),
                Input::Button(Button::Clutch),
                Input::Button(Button::MouseButton),
                Input::Button(Button::ScrollPress),
                Input::Button(Button::ScrollUp),
                Input::Button(Button::ScrollDown),
            ],
            Self::Toggles => &[
                Input::Button(Button::T1),
                Input::Button(Button::T2),
                Input::Button(Button::T3),
                Input::Button(Button::T4),
                Input::Button(Button::T5),
                Input::Button(Button::T6),
            ],
            Self::Pov1 => &[
                Input::Pov1(Pov1Direction::Up),
                Input::Pov1(Pov1Direction::Right),
                Input::Pov1(Pov1Direction::Down),
                Input::Pov1(Pov1Direction::Left),
            ],
            Self::Pov2 => &[
                Input::Button(Button::Pov2Up),
                Input::Button(Button::Pov2Right),
                Input::Button(Button::Pov2Down),
                Input::Button(Button::Pov2Left),
            ],
            Self::ThrottleHat => &[
                Input::Button(Button::ThrottleHatUp),
                Input::Button(Button::ThrottleHatRight),
                Input::Button(Button::ThrottleHatDown),
                Input::Button(Button::ThrottleHatLeft),
            ],
            Self::Mfd => &[
                Input::Button(Button::Function),
                Input::Button(Button::StartStop),
                Input::Button(Button::Reset),
                Input::Button(Button::PageUp),
                Input::Button(Button::PageDown),
                Input::Button(Button::Up),
                Input::Button(Button::Down),
                Input::Button(Button::Select),
            ],
        }
    }

    /// The controls of the group `map`'s device type can name.
    pub(crate) fn nameable(self, map: &InputMap) -> impl Iterator<Item = Input> {
        self.controls()
            .iter()
            .copied()
            .filter(|input| map.is_nameable(*input))
    }
}

/// The badge at the start of `input`'s row: the number games show for a button of `map`, or
/// the arrow of a POV 1 direction. `None` for a button `map` does not have.
pub(crate) fn badge(map: &InputMap, input: Input) -> Option<String> {
    match input {
        Input::Button(button) => map
            .code_of(button)
            .and_then(|code| map.button_number(code))
            .map(|number| number.to_string()),
        Input::Pov1(direction) => Some(tester::arrow(direction).to_string()),
    }
}

/// A count as gettext takes it.
fn count_of(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The first sentence of the view's description: how many controls have a custom name in
/// `mode`, or in the profile when it is single (`None`). Translated.
pub(crate) fn count_summary(count: usize, mode: Option<Mode>) -> String {
    let number = count.to_string();
    let text = match (count, mode) {
        (0, Some(_)) => {
            gettext("No custom names in mode {mode}: every control uses its built-in name.")
        }
        (0, None) => gettext("No custom names: every control uses its built-in name."),
        (_, Some(_)) => ngettext(
            "{count} control has a custom name in mode {mode}.",
            "{count} controls have custom names in mode {mode}.",
            count_of(count),
        ),
        (_, None) => ngettext(
            "{count} control has a custom name.",
            "{count} controls have custom names.",
            count_of(count),
        ),
    };
    text.replace("{count}", &number).replace(
        "{mode}",
        &mode.map_or(0, |mode| mode_index(mode) + 1).to_string(),
    )
}

/// The sentence of the view's description that names the MFD line showing the names, `line`
/// from `0`. Translated.
pub(crate) fn shown_on_line(line: usize) -> String {
    // Translators: {line} is the number of an MFD line, 1 to 3.
    gettext("Shown on MFD line {line} while the control is held.")
        .replace("{line}", &(line + 1).to_string())
}

/// The subtitle of a group: the custom names of its controls, in order. Translated.
pub(crate) fn group_summary(names: &[&str]) -> String {
    if names.is_empty() {
        return gettext("Built-in names");
    }
    // Translators: {names} is a list of custom control names, such as "Gear, Emerg. Lights".
    ngettext(
        "{count} custom: {names}",
        "{count} custom: {names}",
        count_of(names.len()),
    )
    .replace("{count}", &names.len().to_string())
    .replace("{names}", &names.join(", "))
}

/// How many names edits each set of mode settings has had, so an Undo can tell whether a later
/// edit came after it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct NameGenerations {
    /// The generation of each set edited so far; a set never edited is at 0.
    generations: BTreeMap<(ProfileId, SettingsSet), u64>,
}

impl NameGenerations {
    /// The generation of `profile`'s `set`.
    pub(crate) fn current(&self, profile: &ProfileId, set: SettingsSet) -> u64 {
        self.generations
            .get(&(profile.clone(), set))
            .copied()
            .unwrap_or(0)
    }

    /// A names edit of `profile`'s `set`: its generation moves on. Returns the new one.
    pub(crate) fn bump(&mut self, profile: &ProfileId, set: SettingsSet) -> u64 {
        let generation = self.generations.entry((profile.clone(), set)).or_insert(0);
        *generation += 1;
        *generation
    }

    /// `profile` changed its kind, which recreates its sets: every one moves on.
    pub(crate) fn bump_profile(&mut self, profile: &ProfileId) {
        let sets = std::iter::once(SettingsSet::Single).chain(Mode::ALL.map(SettingsSet::Mode));
        for set in sets {
            self.bump(profile, set);
        }
    }
}

/// What Copy or Clear changed, so its Undo can put it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NamesUndo {
    /// The profile edited.
    pub(crate) profile: ProfileId,
    /// Its set of mode settings edited.
    pub(crate) set: SettingsSet,
    /// The names before the operation.
    pub(crate) before: BTreeMap<Input, String>,
    /// The names after it.
    pub(crate) after: BTreeMap<Input, String>,
    /// The generation the operation left the set at.
    generation: u64,
}

impl NamesUndo {
    /// Records an operation that changed `profile`'s `set` from `before` to `after`, and bumps
    /// the set's generation.
    pub(crate) fn record(
        generations: &mut NameGenerations,
        profile: ProfileId,
        set: SettingsSet,
        before: BTreeMap<Input, String>,
        after: BTreeMap<Input, String>,
    ) -> Self {
        let generation = generations.bump(&profile, set);
        Self {
            profile,
            set,
            before,
            after,
            generation,
        }
    }

    /// Whether no names edit came after the operation, so its Undo still applies.
    pub(crate) fn is_current(&self, generations: &NameGenerations) -> bool {
        generations.current(&self.profile, self.set) == self.generation
    }

    /// The names to put back, when the Undo still applies; the Undo is a names edit itself, so the
    /// set's generation moves on and a second Undo does nothing.
    pub(crate) fn restore(
        &self,
        generations: &mut NameGenerations,
    ) -> Option<BTreeMap<Input, String>> {
        if !self.is_current(generations) {
            return None;
        }
        generations.bump(&self.profile, self.set);
        Some(self.before.clone())
    }
}

/// Sets the name of `input` in `names` to `text`, as typed; a blank text removes the name.
/// Returns whether `names` changed. The text is checked with [`super::name_text`] before.
pub(crate) fn set_name(names: &mut BTreeMap<Input, String>, input: Input, text: &str) -> bool {
    if text.trim().is_empty() {
        names.remove(&input).is_some()
    } else if names.get(&input).map(String::as_str) == Some(text) {
        false
    } else {
        names.insert(input, text.to_owned());
        true
    }
}

#[cfg(test)]
mod tests;
