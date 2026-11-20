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
use gettextrs::gettext;
use gtk::glib;
use x52::command::Brightness;
use x52::input::{Input, InputMap, Mode};

use super::Application;
use crate::profile::clocks::ProfileClocks;
use crate::profile::editor::EditorState;
use crate::profile::leds::{self, EditorLed, LedChoice};
use crate::profile::mfd::{DraftKey, LineSource, SettingsSet};
use crate::profile::names::{self, NamesUndo};
use crate::profile::store::StoreError;
use crate::profile::{
    self, MFD_LINES, MfdLineSource, ModeSettings, Profile, ProfileId, ProfileKind,
};

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

    /// Sets the custom name of `input` in the mode shown to `text`; a blank text removes it. A
    /// name that cannot be used is refused, and the editor shows the stored name again.
    pub(crate) fn set_name(&self, input: Input, text: &str) {
        let nameable = self
            .shown_device_type()
            .is_some_and(|device_type| InputMap::for_device(device_type).is_nameable(input));
        let refused = if text.trim().is_empty() {
            None
        } else {
            profile::name_text(text)
                .err()
                .map(|error| error.to_string())
        };
        if !nameable || refused.is_some() {
            tracing::debug!("Name {text:?} of {input:?} refused: {refused:?}");
            self.update_windows();
            return;
        }
        self.edit_shown(|target, settings| {
            if names::set_name(&mut settings.names, input, text) {
                self.imp()
                    .name_generations
                    .borrow_mut()
                    .bump(&target.profile, target.set);
            }
        });
        self.expire_names_undo();
    }

    /// Copies the names of mode `from` into the mode shown of a per-mode profile, with a toast
    /// to undo it.
    pub(crate) fn copy_names_from(&self, from: Mode) {
        let undo = self.replace_shown_names(|profile, shown| {
            (profile.kind() == ProfileKind::PerMode && from != shown)
                .then(|| profile.settings.for_mode(from).names.clone())
        });
        if let Some(undo) = undo {
            // Translators: {mode} is the number of a mode of the mode selector, 1 to 3.
            let title = gettext("Names copied from mode {mode}")
                .replace("{mode}", &(profile::mode_index(from) + 1).to_string());
            self.toast_names_undo(undo, &title);
        }
    }

    /// Removes every name of the mode shown, with a toast to undo it.
    pub(crate) fn clear_names(&self) {
        if let Some(undo) =
            self.replace_shown_names(|_, _| Some(std::collections::BTreeMap::default()))
        {
            self.toast_names_undo(undo, &gettext("Names cleared"));
        }
    }

    /// Replaces the names of the mode shown with what `names` gives for the profile and that
    /// mode; returns the Undo when they changed.
    fn replace_shown_names(
        &self,
        names: impl FnOnce(&Profile, Mode) -> Option<std::collections::BTreeMap<Input, String>>,
    ) -> Option<NamesUndo> {
        let mode = self.shown_mode();
        let mut undo = None;
        self.edit_shown_profile(|id, profile| {
            let Some(after) = names(profile, mode) else {
                return;
            };
            let set = SettingsSet::of(profile.kind(), mode);
            let current = &mut profile.settings.for_mode_mut(mode).names;
            if *current == after {
                return;
            }
            let before = std::mem::replace(current, after.clone());
            undo = Some(NamesUndo::record(
                &mut self.imp().name_generations.borrow_mut(),
                id.clone(),
                set,
                before,
                after,
            ));
        });
        undo
    }

    /// Shows the toast of a Copy or Clear of names, with its Undo; the toast of an earlier one
    /// goes.
    fn toast_names_undo(&self, undo: NamesUndo, title: &str) {
        let toast = adw::Toast::builder()
            .title(title)
            .button_label(gettext("_Undo"))
            .use_markup(false)
            .build();
        toast.connect_button_clicked(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_| app.undo_names()
        ));
        toast.connect_dismissed(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |toast| {
                let imp = app.imp();
                let ours = imp
                    .names_undo
                    .borrow()
                    .as_ref()
                    .is_some_and(|(_, shown)| shown == toast);
                if ours {
                    imp.names_undo.take();
                }
            }
        ));
        let previous = self.imp().names_undo.replace(Some((undo, toast.clone())));
        if let Some((_, previous)) = previous {
            previous.dismiss();
        }
        self.queue_toast(toast);
    }

    /// Puts back the names the last Copy or Clear replaced, unless a names edit came after it.
    fn undo_names(&self) {
        let Some((undo, _)) = self.imp().names_undo.take() else {
            return;
        };
        let Some(before) = undo.restore(&mut self.imp().name_generations.borrow_mut()) else {
            return;
        };
        let mode = match undo.set {
            SettingsSet::Single => Mode::One,
            SettingsSet::Mode(mode) => mode,
        };
        self.edit_profile(&undo.profile, |profile| {
            if SettingsSet::of(profile.kind(), mode) == undo.set {
                profile.settings.for_mode_mut(mode).names = before;
            }
        });
    }

    /// Dismisses the toast of the last Copy or Clear once a later names edit makes its Undo do
    /// nothing.
    pub(super) fn expire_names_undo(&self) {
        let imp = self.imp();
        let expired = imp
            .names_undo
            .borrow()
            .as_ref()
            .is_some_and(|(undo, _)| !undo.is_current(&imp.name_generations.borrow()));
        if expired && let Some((_, toast)) = imp.names_undo.take() {
            toast.dismiss();
        }
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
