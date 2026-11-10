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
//! go to the device at once and are saved after the debounce (docs/ui.md, "Profile editor").

#![expect(dead_code, reason = "the LED editor of the next commit uses it")]

use adw::subclass::prelude::*;
use x52::command::Brightness;

use super::Application;
use crate::profile::ModeSettings;
use crate::profile::leds::{self, EditorLed, EditorState, LedChoice};

impl Application {
    /// What the LED editor shows: the shown device type's active profile in the mode shown, once
    /// the profiles are loaded.
    pub(crate) fn editor_state(&self) -> Option<EditorState> {
        let device_type = self.shown_device_type()?;
        let id = self.active_profile_id(device_type)?;
        let profile = self.profile(&id)?;
        let mode = self.shown_mode();
        Some(EditorState {
            device_type,
            kind: profile.kind(),
            mode,
            settings: profile.settings_for(mode).clone(),
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

    /// Applies `edit` to the settings of the mode shown of the shown device type's active profile;
    /// a change goes to the device and the windows at once and is saved after the debounce.
    fn edit_shown_settings(&self, edit: impl FnOnce(&mut ModeSettings)) {
        let Some(device_type) = self.shown_device_type() else {
            return;
        };
        let Some(id) = self.active_profile_id(device_type) else {
            return;
        };
        let mode = self.shown_mode();
        {
            let mut profiles = self.imp().profiles.borrow_mut();
            let Some(store) = profiles.as_mut() else {
                return;
            };
            let Some(mut profile) = store.loaded(&id).map(|loaded| loaded.profile.clone()) else {
                return tracing::debug!("Profile {id} cannot be edited: it is not usable");
            };
            let settings = profile.settings.for_mode_mut(mode);
            let before = settings.clone();
            edit(settings);
            if *settings == before {
                return;
            }
            if let Err(error) = store.update(&id, profile) {
                return tracing::warn!("Cannot edit profile {id}: {error}");
            }
        }
        self.profile_edited(&id);
        self.push_desired_state();
        // The list shows no LED: only the editor follows.
        self.update_windows();
    }
}
