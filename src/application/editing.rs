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

//! The edits of the profile editor: they change the active profile's settings in the mode shown,
//! or its clocks, go to the device at once and are saved after the debounce (docs/ui.md,
//! "Profile editor").

use adw::subclass::prelude::*;
use x52::command::Brightness;

use super::Application;
use crate::profile::clocks::ProfileClocks;
use crate::profile::editor::EditorState;
use crate::profile::leds::{self, EditorLed, LedChoice};
use crate::profile::mfd::{DraftKey, LineSource, SettingsSet};
use crate::profile::store::StoreError;
use crate::profile::{self, MFD_LINES, MfdLineSource, ModeSettings, Profile, ProfileId};

/// The profile and the set of mode settings an edit changes.
struct EditTarget {
    /// The profile.
    profile: ProfileId,
    /// The set of mode settings.
    set: SettingsSet,
}

impl EditTarget {
    /// The draft key of `line` in this set.
    fn draft(&self, line: usize) -> DraftKey {
        DraftKey {
            profile: self.profile.clone(),
            set: self.set,
            line,
        }
    }
}

impl Application {
    /// What the profile editor shows: the shown device type's active profile in the mode shown,
    /// once the profiles are loaded.
    pub(crate) fn editor_state(&self) -> Option<EditorState> {
        let device_type = self.shown_device_type()?;
        let id = self.active_profile_id(device_type)?;
        let profile = self.profile(&id)?;
        let mode = self.shown_mode();
        Some(EditorState {
            profile: id,
            device_type,
            kind: profile.kind(),
            mode,
            settings: profile.settings_for(mode).clone(),
            clocks: ProfileClocks::of(&profile),
        })
    }

    /// Sets `led` to `choice` in the mode shown.
    pub(crate) fn set_led(&self, led: EditorLed, choice: LedChoice) {
        self.edit_shown_settings(|settings| {
            if !leds::set_choice(&mut settings.leds, led, choice) {
                tracing::debug!("{led:?} cannot be {choice:?}");
            }
        });
    }

    /// Sets the LED brightness of the mode shown.
    pub(crate) fn set_led_brightness(&self, brightness: Brightness) {
        self.edit_shown_settings(|settings| settings.led_brightness = brightness);
    }

    /// Turns the blinking of the i and POV 1 LEDs on or off in the mode shown.
    pub(crate) fn set_blink(&self, blink: bool) {
        self.edit_shown_settings(|settings| settings.blink = blink);
    }

    /// Makes MFD line `line` (from `0`) show `source` in the mode shown. A custom text line
    /// switched to another source keeps its text for the session, and gets it back when it
    /// switches to custom text again in the same profile, set and line.
    pub(crate) fn set_mfd_source(&self, line: usize, source: LineSource) {
        if line >= MFD_LINES {
            return;
        }
        self.edit_shown(|target, settings| {
            let current = &mut settings.mfd_lines[line];
            if LineSource::of(current) == source {
                return;
            }
            let mut drafts = self.imp().text_drafts.borrow_mut();
            if let MfdLineSource::Text(text) = std::mem::take(current) {
                drafts.leave(target.draft(line), text);
            }
            *current = source.to_line(|| drafts.resume(&target.draft(line)));
        });
    }

    /// Sets the text of custom text line `line` (from `0`) in the mode shown; text that does not
    /// fit the line is refused, and the editor shows the line's text again.
    pub(crate) fn set_mfd_text(&self, line: usize, text: String) {
        if let Err(error) = profile::line_text(&text) {
            tracing::debug!("MFD line {line} text {text:?} refused: {error}");
            self.update_windows();
            return;
        }
        self.edit_shown_settings(|settings| {
            if let Some(current @ MfdLineSource::Text(_)) = settings.mfd_lines.get_mut(line) {
                *current = MfdLineSource::Text(text);
            }
        });
    }

    /// Sets the MFD brightness of the mode shown.
    pub(crate) fn set_mfd_brightness(&self, brightness: Brightness) {
        self.edit_shown_settings(|settings| settings.mfd_brightness = brightness);
    }

    /// Turns the SHIFT label on or off in the mode shown.
    pub(crate) fn set_shift(&self, shift: bool) {
        self.edit_shown_settings(|settings| settings.shift = shift);
    }

    /// Applies `edit` to the settings of the mode shown of the shown device type's active profile;
    /// a change goes to the device and the windows at once and is saved after the debounce.
    fn edit_shown_settings(&self, edit: impl FnOnce(&mut ModeSettings)) {
        self.edit_shown(|_, settings| edit(settings));
    }

    /// [`Self::edit_shown_settings`], with the profile and the set the edit changes.
    fn edit_shown(&self, edit: impl FnOnce(&EditTarget, &mut ModeSettings)) {
        let mode = self.shown_mode();
        self.edit_shown_profile(|id, profile| {
            let target = EditTarget {
                profile: id.clone(),
                set: SettingsSet::of(profile.kind(), mode),
            };
            edit(&target, profile.settings.for_mode_mut(mode));
        });
    }

    /// Applies `edit` to the shown device type's active profile, with its id; returns whether
    /// the profile changed.
    pub(super) fn edit_shown_profile(&self, edit: impl FnOnce(&ProfileId, &mut Profile)) -> bool {
        let Some(device_type) = self.shown_device_type() else {
            return false;
        };
        let Some(id) = self.active_profile_id(device_type) else {
            return false;
        };
        self.edit_profile(&id, |profile| edit(&id, profile))
    }

    /// Applies `edit` to profile `id`, and only to it, whichever profile is shown; a change goes
    /// to the device (when `id` is the active profile) and the windows at once and is saved after
    /// the debounce. A profile that is gone or not usable is left as it is. Returns whether the
    /// profile changed.
    pub(super) fn edit_profile(&self, id: &ProfileId, edit: impl FnOnce(&mut Profile)) -> bool {
        let edited = {
            let mut profiles = self.imp().profiles.borrow_mut();
            let Some(store) = profiles.as_mut() else {
                return false;
            };
            match store.edit(id, edit) {
                Ok(changed) => changed,
                Err(error @ (StoreError::Unknown(_) | StoreError::ReadOnly(_))) => {
                    tracing::debug!("Profile {id} cannot be edited: {error}");
                    false
                }
                Err(error) => {
                    tracing::warn!("Cannot edit profile {id}: {error}");
                    false
                }
            }
        };
        if edited {
            self.profile_edited(id);
            self.push_desired_state();
        }
        // The list shows none of the edited values: only the editor follows, and shows the
        // profile's values again after an edit that changed nothing.
        self.update_windows();
        edited
    }
}
