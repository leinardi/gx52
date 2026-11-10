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

//! Creating, editing, saving and removing profiles: the store changes on the main thread, the
//! file work runs as jobs off it, one at a time per profile (docs/architecture.md,
//! "Persistence").
//!
//! The application stays alive while any of it is pending: it holds itself while the write queue
//! is not idle, drains the queue when its windows close, and asks the session to wait while
//! profiles are saved at logout.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use x52::clock::{Clock, SystemClock};
use x52::device::DeviceType;

use super::Application;
use crate::profile::exchange::ImportPreview;
use crate::profile::name::{self, NameProblem};
use crate::profile::removals::{DeleteEnd, PutBack};
use crate::profile::store::{Entry, SaveJob, StoreError};
use crate::profile::writes::{Command, DEBOUNCE, Op, Outcome};
use crate::profile::{Profile, ProfileId, ProfileKind, active, order};
use crate::window::Window;

/// Why a name was not given to a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RenameProblem {
    /// The name is empty or too long.
    Invalid(NameProblem),
    /// Another profile of the type has it, or a new one is about to.
    Taken(String),
    /// The profile cannot be renamed: it is gone or read-only.
    Unavailable,
}

impl RenameProblem {
    /// What the row shows below the name.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Invalid(NameProblem::Empty) => gettext("Enter a name"),
            Self::Invalid(NameProblem::TooLong(_)) => {
                gettext("The name is too long, {max} characters at most")
                    .replace("{max}", &name::MAX_CHARS.to_string())
            }
            Self::Taken(name) => {
                gettext("A profile named “{name}” already exists").replace("{name}", name)
            }
            Self::Unavailable => gettext("This profile cannot be renamed"),
        }
    }
}

/// Where a new profile goes in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Placement {
    /// Last, as the active profile, with its name open for editing: a new profile.
    LastAndActive,
    /// Right after the original: a duplicate.
    After(ProfileId),
}

/// The name a new profile is offered under.
fn new_profile_name() -> String {
    gettext("New profile")
}

impl Application {
    /// `id` was edited in memory: its save is due after the debounce.
    pub(crate) fn profile_edited(&self, id: &ProfileId) {
        let commands = self.imp().writes.borrow_mut().edited(id);
        self.run_commands(commands);
    }

    /// Starts every pending save and the delete of every removal still waiting for its toast:
    /// the window is closing, or the session is ending. Can be called any number of times.
    pub(crate) fn drain(&self) {
        let imp = self.imp();
        imp.draining.set(true);
        let commands = imp.writes.borrow_mut().flush();
        self.run_commands(commands);
        let commands = imp.removals.borrow().drain(&mut imp.writes.borrow_mut());
        self.run_commands(commands);
    }

    /// The session is ending: drains the queue and asks the session to wait until it is idle.
    /// Best effort: a forced logout can still end the process (the atomic writes then leave the
    /// old or the new file).
    pub(crate) fn on_query_end(&self) {
        self.drain();
        let imp = self.imp();
        if imp.writes.borrow().is_idle() || imp.inhibit_cookie.get().is_some() {
            return;
        }
        let cookie = self.inhibit(
            self.active_window().as_ref(),
            gtk::ApplicationInhibitFlags::LOGOUT,
            Some(&gettext("Saving profiles")),
        );
        imp.inhibit_cookie.set(Some(cookie));
    }

    /// Whether nothing is waiting to be written, for the log at shutdown.
    pub(crate) fn writes_idle(&self) -> bool {
        self.imp().writes.borrow().is_idle()
    }

    fn run_commands(&self, commands: Vec<Command>) {
        for command in commands {
            match command {
                Command::ArmDebounce(id) => self.arm_debounce(id),
                Command::CancelDebounce(id) => {
                    if let Some(source) = self.imp().debounces.borrow_mut().remove(&id) {
                        source.remove();
                    }
                }
                Command::Start(id, Op::Save) => self.start_save(id),
                Command::Start(id, Op::Delete) => self.start_delete(id),
            }
        }
        self.update_hold();
    }

    /// Holds the application while the queue is busy, so closing the last window never ends
    /// the process in the middle of a write; releases the hold and the logout inhibition when
    /// it is idle.
    fn update_hold(&self) {
        let imp = self.imp();
        if imp.writes.borrow().is_idle() {
            imp.draining.set(false);
            imp.hold.take();
            if let Some(cookie) = imp.inhibit_cookie.take() {
                self.uninhibit(cookie);
            }
        } else if imp.hold.borrow().is_none() {
            imp.hold.replace(Some(self.hold()));
        }
    }

    fn arm_debounce(&self, id: ProfileId) {
        let key = id.clone();
        let source = glib::timeout_add_local_once(
            DEBOUNCE,
            glib::clone!(
                #[weak(rename_to = app)]
                self,
                move || {
                    let imp = app.imp();
                    imp.debounces.borrow_mut().remove(&id);
                    let commands = imp.writes.borrow_mut().debounce_fired(&id);
                    app.run_commands(commands);
                }
            ),
        );
        if let Some(earlier) = self.imp().debounces.borrow_mut().insert(key, source) {
            earlier.remove();
        }
    }

    fn finish(&self, id: &ProfileId, outcome: Outcome) {
        let commands = self.imp().writes.borrow_mut().finished(id, outcome);
        self.run_commands(commands);
    }

    fn start_save(&self, id: ProfileId) {
        let job = {
            let profiles = self.imp().profiles.borrow();
            profiles
                .as_ref()
                .ok_or_else(|| StoreError::Unknown(id.clone()))
                .and_then(|store| store.save_job(&id))
        };
        match job {
            Ok(job) => self.run_save(id, job),
            Err(StoreError::NeedsConfirmation(_)) => {
                glib::spawn_future_local(glib::clone!(
                    #[weak(rename_to = app)]
                    self,
                    async move { app.confirm_bad_default(id).await }
                ));
            }
            Err(error) => {
                tracing::warn!("Cannot save profile {id}: {error}");
                self.finish(&id, Outcome::Failed);
            }
        }
    }

    fn run_save(&self, id: ProfileId, job: SaveJob) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = app)]
            self,
            async move {
                let outcome = match gio::spawn_blocking(move || job.run()).await {
                    Ok(Ok(saved)) => {
                        if let Some(backup) = saved.backup {
                            tracing::info!("Profile {id}: old file kept as {}", backup.display());
                        }
                        if let Some(store) = app.imp().profiles.borrow_mut().as_mut() {
                            store.saved(&id);
                        }
                        app.imp().failed_saves.borrow_mut().remove(&id);
                        Outcome::Done
                    }
                    Ok(Err(error)) => {
                        app.save_failed(&id, &error);
                        Outcome::Failed
                    }
                    Err(_) => {
                        tracing::error!("The task saving profile {id} panicked");
                        Outcome::Failed
                    }
                };
                app.finish(&id, outcome);
            }
        ));
    }

    /// Reports a failed save once per profile until one succeeds; the edits stay in memory and
    /// the next edit tries again.
    fn save_failed(&self, id: &ProfileId, error: &StoreError) {
        if !self.imp().failed_saves.borrow_mut().insert(id.clone()) {
            return;
        }
        tracing::warn!("Cannot save profile {id}: {error}");
        let name = self
            .profile(id)
            .map_or_else(|| id.file_name(), |profile| profile.name);
        self.toast(&gettext("“{name}” could not be saved").replace("{name}", &name));
    }

    /// Asks before a default profile is saved over its bad file. Without a window, while the
    /// window closes or the session ends (nobody would answer), or after the user said no for
    /// this profile, the edits stay in memory only.
    async fn confirm_bad_default(&self, id: ProfileId) {
        let imp = self.imp();
        let window = self.active_window().filter(|_| !imp.draining.get());
        let declined = imp.declined_replacements.borrow().contains(&id);
        let (Some(window), false) = (window, declined) else {
            tracing::warn!("Profile {id} not saved: its damaged file is replaced only on request");
            return self.finish(&id, Outcome::Failed);
        };
        let file_name = id.file_name();
        let dialog = adw::AlertDialog::new(
            Some(&gettext("Replace the Damaged File?")),
            Some(
                &gettext(
                    "The file {file} cannot be read. GX52 keeps a copy of it next to it and \
                     saves the profile in its place.",
                )
                .replace("{file}", &file_name),
            ),
        );
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("replace", &gettext("_Replace")),
        ]);
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
        dialog.set_close_response("cancel");
        if dialog.choose_future(Some(&window)).await != "replace" {
            tracing::info!("Profile {id}: the damaged file is kept; the edits stay in memory");
            self.imp()
                .declined_replacements
                .borrow_mut()
                .insert(id.clone());
            return self.finish(&id, Outcome::Failed);
        }
        let job = {
            let profiles = self.imp().profiles.borrow();
            profiles
                .as_ref()
                .ok_or_else(|| StoreError::Unknown(id.clone()))
                .and_then(|store| store.replace_bad_default_job(&id, SystemClock.now()))
        };
        match job {
            Ok(job) => self.run_save(id, job),
            Err(error) => {
                tracing::warn!("Cannot save profile {id}: {error}");
                self.finish(&id, Outcome::Failed);
            }
        }
    }

    fn start_delete(&self, id: ProfileId) {
        let job = {
            let profiles = self.imp().profiles.borrow();
            let Some(store) = profiles.as_ref() else {
                return;
            };
            store.delete_job(&id)
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = app)]
            self,
            async move {
                let result = gio::spawn_blocking(move || job.run()).await;
                let succeeded = match result {
                    Ok(Ok(())) => {
                        tracing::info!("Profile {id} removed");
                        true
                    }
                    Ok(Err(error)) => {
                        tracing::warn!("Cannot remove profile {id}: {error}");
                        false
                    }
                    Err(_) => {
                        tracing::error!("The task removing profile {id} panicked");
                        false
                    }
                };
                let end = {
                    let imp = app.imp();
                    let mut profiles = imp.profiles.borrow_mut();
                    profiles.as_mut().map(|store| {
                        imp.removals.borrow_mut().delete_finished(
                            store,
                            &mut imp.reservations.borrow_mut(),
                            &id,
                            succeeded,
                        )
                    })
                };
                match end {
                    Some(DeleteEnd::Gone(Some(device_type))) => app.deleted(&id, device_type),
                    Some(DeleteEnd::PutBack(put_back)) => {
                        let title = put_back.title.clone();
                        app.put_back(&id, &put_back);
                        app.toast(
                            &gettext("“{name}” could not be removed").replace("{name}", &title),
                        );
                    }
                    _ => {}
                }
                app.finish(
                    &id,
                    if succeeded {
                        Outcome::Done
                    } else {
                        Outcome::Failed
                    },
                );
                app.refresh_profiles();
                app.push_desired_state();
            }
        ));
    }

    /// The file of a removed profile is gone: drops it from the order.
    fn deleted(&self, id: &ProfileId, device_type: DeviceType) {
        let order = {
            let profiles = self.imp().profiles.borrow();
            let Some(store) = profiles.as_ref() else {
                return;
            };
            let entries = self.order_entries(device_type);
            let default = ProfileId::default_for(device_type);
            let mut order = order::ordered(&entries, device_type, store);
            order.retain(|listed| *listed != default && listed != id);
            order
        };
        self.store_order(device_type, &order);
    }

    /// A removed profile is back in the store (the removal was undone, or deleting its file
    /// failed): puts it back in its place, and makes it active again when it was and the default
    /// still holds its place.
    fn put_back(&self, id: &ProfileId, put_back: &PutBack) {
        let imp = self.imp();
        let Some(device_type) = put_back.device_type else {
            return;
        };
        let order = {
            let profiles = imp.profiles.borrow();
            profiles.as_ref().map(|store| {
                order::restored(
                    &self.order_entries(device_type),
                    device_type,
                    store,
                    id,
                    put_back.previous.as_ref(),
                    put_back.index,
                )
            })
        };
        if let Some(order) = order {
            self.store_order(device_type, &order);
        }
        if let (true, Some(settings)) = (put_back.reactivate, imp.settings.get()) {
            let key = active::settings_key(device_type);
            if settings.string(key) == ProfileId::default_for(device_type).as_str()
                && let Err(error) = settings.set_string(key, id.as_str())
            {
                tracing::warn!("Cannot make profile {id} active again: {error}");
            }
        }
    }

    /// Removes `id` after the user confirmed it: out of the store at once, its file deleted when
    /// the Undo toast goes away.
    pub(crate) fn remove_profile(&self, id: &ProfileId) {
        let imp = self.imp();
        let device_type = {
            let profiles = imp.profiles.borrow();
            profiles
                .as_ref()
                .and_then(|store| store.entry(id))
                .and_then(Entry::device_type)
        };
        let positions =
            device_type.map_or_else(Vec::new, |device_type| self.positions(device_type));
        let active = device_type
            .zip(imp.settings.get())
            .and_then(|(device_type, settings)| {
                ProfileId::parse(settings.string(active::settings_key(device_type)).as_str())
            });
        let removed = {
            let mut profiles = imp.profiles.borrow_mut();
            let Some(store) = profiles.as_mut() else {
                return;
            };
            imp.removals.borrow_mut().remove(
                store,
                &mut imp.reservations.borrow_mut(),
                &mut imp.writes.borrow_mut(),
                id,
                &positions,
                active.as_ref(),
            )
        };
        let removed = match removed {
            Ok(removed) => removed,
            Err(error) => return tracing::debug!("Profile {id} not removed: {error}"),
        };
        self.run_commands(removed.commands);
        if let (true, Some(device_type), Some(settings)) =
            (removed.was_active, removed.device_type, imp.settings.get())
            && let Err(error) = settings.set_string(
                active::settings_key(device_type),
                ProfileId::default_for(device_type).as_str(),
            )
        {
            tracing::warn!("Cannot make the default profile active: {error}");
        }
        self.refresh_profiles();
        self.push_desired_state();

        let title = removed.title;
        let serial = removed.serial;
        let toast = adw::Toast::builder()
            .title(gettext("“{name}” removed").replace("{name}", &title))
            .button_label(gettext("_Undo"))
            .use_markup(false)
            .build();
        let undo_id = id.clone();
        toast.connect_button_clicked(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_| app.undo_remove(&undo_id)
        ));
        let due_id = id.clone();
        toast.connect_dismissed(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |_| {
                let imp = app.imp();
                let commands =
                    imp.removals
                        .borrow()
                        .delete_due(&mut imp.writes.borrow_mut(), &due_id, serial);
                app.run_commands(commands);
            }
        ));
        self.queue_toast(toast);
    }

    /// Undoes the removal of `id`, unless its delete has started.
    fn undo_remove(&self, id: &ProfileId) {
        let imp = self.imp();
        let undone = {
            let mut profiles = imp.profiles.borrow_mut();
            profiles.as_mut().and_then(|store| {
                imp.removals.borrow_mut().undo(
                    store,
                    &mut imp.reservations.borrow_mut(),
                    &mut imp.writes.borrow_mut(),
                    id,
                )
            })
        };
        let Some((put_back, commands)) = undone else {
            return tracing::debug!("Profile {id}: too late to undo the removal");
        };
        self.put_back(id, &put_back);
        self.run_commands(commands);
        self.refresh_profiles();
        self.push_desired_state();
    }

    /// Creates a single or per-mode profile with the built-in values, last in the list, as the
    /// active profile, its name open for editing.
    pub(crate) fn new_profile(&self, kind: ProfileKind) {
        let Some(device_type) = self.shown_device_type() else {
            return;
        };
        let mut profile = Profile::builtin(device_type, String::new());
        if kind == ProfileKind::PerMode {
            profile.settings = profile.settings.into_per_mode();
        }
        self.add_profile(profile, &new_profile_name(), Placement::LastAndActive);
    }

    /// Duplicates `id` as "`<name> (2)`", right after it.
    pub(crate) fn duplicate_profile(&self, id: &ProfileId) {
        let Some(profile) = self.profile(id) else {
            return tracing::debug!("Profile {id} cannot be duplicated: it is not usable");
        };
        let base = profile.name.clone();
        self.add_profile(profile, &base, Placement::After(id.clone()));
    }

    /// Writes `profile` as a new profile under a free name based on `base`, keeping the name
    /// reserved and the application busy until it is in the store.
    fn add_profile(&self, mut profile: Profile, base: &str, placement: Placement) {
        let imp = self.imp();
        let device_type = profile.device_type;
        let (reservation, job) = {
            let profiles = imp.profiles.borrow();
            let Some(store) = profiles.as_ref() else {
                return;
            };
            let reservation = imp
                .reservations
                .borrow_mut()
                .claim_unique(store, device_type, base);
            reservation.name().clone_into(&mut profile.name);
            let job = store.add_job(&profile);
            (reservation, job)
        };
        let token = imp.writes.borrow_mut().create_started();
        self.update_hold();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = app)]
            self,
            async move {
                let result = gio::spawn_blocking(move || job.run()).await;
                let imp = app.imp();
                imp.reservations.borrow_mut().release(reservation);
                let added = match result {
                    Ok(Ok(id)) => Some(id),
                    Ok(Err(error)) => {
                        tracing::warn!("Cannot create a profile: {error}");
                        None
                    }
                    Err(_) => {
                        tracing::error!("The task creating a profile panicked");
                        None
                    }
                };
                if let Some(id) = &added {
                    tracing::info!("Profile {id} created");
                    app.added(id, profile, &placement);
                } else {
                    app.toast(&gettext("The profile could not be created"));
                }
                imp.writes.borrow_mut().create_committed(token);
                app.update_hold();
                app.refresh_profiles();
                app.push_desired_state();
                if let (Some(id), Placement::LastAndActive) = (added, placement)
                    && let Some(window) = app.active_window().and_downcast::<Window>()
                {
                    window.start_rename(&id);
                }
            }
        ));
    }

    /// Puts a profile whose file was just written into the store and the order.
    fn added(&self, id: &ProfileId, profile: Profile, placement: &Placement) {
        let imp = self.imp();
        let device_type = profile.device_type;
        let order = {
            let mut profiles = imp.profiles.borrow_mut();
            let Some(store) = profiles.as_mut() else {
                return;
            };
            store.added(id.clone(), profile);
            let entries = self.order_entries(device_type);
            match placement {
                Placement::LastAndActive => order::appended(&entries, device_type, store, id),
                Placement::After(original) => {
                    order::inserted_after(&entries, device_type, store, original, id)
                }
            }
        };
        self.store_order(device_type, &order);
        if *placement == Placement::LastAndActive {
            self.activate_profile(id);
        }
    }

    /// The names `device_type`'s profiles have or are about to get, for an import's checks.
    pub(crate) fn taken_names(&self, device_type: DeviceType) -> Option<Vec<String>> {
        let imp = self.imp();
        let profiles = imp.profiles.borrow();
        Some(
            imp.reservations
                .borrow()
                .names(profiles.as_ref()?, device_type),
        )
    }

    /// Reserves the name of a confirmed import and starts writing it, in one step with no
    /// `.await` between the check and the reservation. Gives the preview back when the name was
    /// taken meanwhile, for another round of checks.
    pub(crate) fn write_import(
        &self,
        preview: Box<ImportPreview>,
    ) -> Result<(), Box<ImportPreview>> {
        let imp = self.imp();
        let device_type = preview.profile.device_type;
        let (reservation, job) = {
            let profiles = imp.profiles.borrow();
            let Some(store) = profiles.as_ref() else {
                return Err(preview);
            };
            let claimed =
                imp.reservations
                    .borrow_mut()
                    .claim(store, device_type, &preview.profile.name);
            let Some(reservation) = claimed else {
                return Err(preview);
            };
            (reservation, store.add_job(&preview.profile))
        };
        let token = imp.writes.borrow_mut().create_started();
        self.update_hold();
        let profile = preview.profile.clone();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = app)]
            self,
            async move {
                let result = gio::spawn_blocking(move || job.run()).await;
                let imp = app.imp();
                imp.reservations.borrow_mut().release(reservation);
                let name = profile.name.clone();
                match result {
                    Ok(Ok(id)) => {
                        tracing::info!("Profile {id} imported");
                        let device_type = profile.device_type;
                        let order = {
                            let mut profiles = imp.profiles.borrow_mut();
                            profiles.as_mut().map(|store| {
                                store.added(id.clone(), profile);
                                let entries = app.order_entries(device_type);
                                order::appended(&entries, device_type, store, &id)
                            })
                        };
                        if let Some(order) = order {
                            app.store_order(device_type, &order);
                        }
                        app.toast(&gettext("Imported “{name}”").replace("{name}", &name));
                    }
                    Ok(Err(error)) => {
                        tracing::warn!("Cannot write the imported profile: {error}");
                        app.toast(&gettext("The profile could not be saved"));
                    }
                    Err(_) => tracing::error!("The profile import task panicked"),
                }
                imp.writes.borrow_mut().create_committed(token);
                app.update_hold();
                // The active id may name the profile that just came in.
                app.push_desired_state();
                app.refresh_profiles();
            }
        ));
        Ok(())
    }

    /// Renames `id` to `raw`, trimmed, when no other profile of its type has or is about to get
    /// that name.
    ///
    /// # Errors
    ///
    /// The [`RenameProblem`] the row shows.
    pub(crate) fn rename_profile(&self, id: &ProfileId, raw: &str) -> Result<(), RenameProblem> {
        let name = name::normalise(raw).map_err(RenameProblem::Invalid)?;
        let imp = self.imp();
        {
            let mut profiles = imp.profiles.borrow_mut();
            let store = profiles.as_mut().ok_or(RenameProblem::Unavailable)?;
            let mut profile = store
                .loaded(id)
                .ok_or(RenameProblem::Unavailable)?
                .profile
                .clone();
            if profile.name == name {
                return Ok(());
            }
            if imp
                .reservations
                .borrow()
                .taken(store, profile.device_type, &name, Some(id))
            {
                return Err(RenameProblem::Taken(name));
            }
            profile.name = name;
            store
                .update(id, profile)
                .map_err(|_| RenameProblem::Unavailable)?;
        }
        self.profile_edited(id);
        self.refresh_profiles();
        // The "profile name" MFD line shows the new name.
        self.push_desired_state();
        Ok(())
    }

    /// Changes the kind of `id`: per-mode copies its settings into the three modes, single keeps
    /// mode 1's.
    pub(crate) fn set_profile_kind(&self, id: &ProfileId, kind: ProfileKind) {
        {
            let mut profiles = self.imp().profiles.borrow_mut();
            let Some(store) = profiles.as_mut() else {
                return;
            };
            let Some(mut profile) = store.loaded(id).map(|loaded| loaded.profile.clone()) else {
                return tracing::debug!("Profile {id} cannot change its kind: it is not usable");
            };
            if profile.kind() == kind {
                return;
            }
            profile.settings = match kind {
                ProfileKind::PerMode => profile.settings.into_per_mode(),
                ProfileKind::Single => profile.settings.into_single(),
            };
            if let Err(error) = store.update(id, profile) {
                return tracing::warn!("Cannot change the kind of profile {id}: {error}");
            }
        }
        self.profile_edited(id);
        self.refresh_profiles();
        self.push_desired_state();
    }

    /// Moves `id` one place up (`-1`) or down (`1`) among the non-default profiles.
    pub(crate) fn move_profile_by(&self, id: &ProfileId, step: isize) {
        self.move_profile(id, |order, from| {
            from.checked_add_signed(step).filter(|to| *to < order)
        });
    }

    /// Moves `id` to the place of `target`: the first place for the default profile.
    pub(crate) fn move_profile_onto(&self, id: &ProfileId, target: &ProfileId) {
        let positions = self.order_positions();
        self.move_profile(id, |_, _| {
            if target.default_type().is_some() {
                Some(0)
            } else {
                positions.iter().position(|listed| listed == target)
            }
        });
    }

    /// The non-default profiles of the shown type, in the list's order.
    fn order_positions(&self) -> Vec<ProfileId> {
        self.shown_device_type()
            .map_or_else(Vec::new, |device_type| self.positions(device_type))
    }

    /// The non-default profiles of `device_type`, in the list's order.
    fn positions(&self, device_type: DeviceType) -> Vec<ProfileId> {
        let profiles = self.imp().profiles.borrow();
        let Some(store) = profiles.as_ref() else {
            return Vec::new();
        };
        let default = ProfileId::default_for(device_type);
        let mut order = order::ordered(&self.order_entries(device_type), device_type, store);
        order.retain(|listed| *listed != default);
        order
    }

    /// Moves `id` to the index `to` returns for its current index and the number of
    /// non-default profiles; nothing when it is not one of them.
    fn move_profile(&self, id: &ProfileId, to: impl FnOnce(usize, usize) -> Option<usize>) {
        let Some(device_type) = self.shown_device_type() else {
            return;
        };
        let positions = self.order_positions();
        let Some(from) = positions.iter().position(|listed| listed == id) else {
            return tracing::debug!("Profile {id} cannot move: it is not in the order");
        };
        let Some(to) = to(positions.len(), from) else {
            return;
        };
        let order = {
            let profiles = self.imp().profiles.borrow();
            let Some(store) = profiles.as_ref() else {
                return;
            };
            order::moved(&self.order_entries(device_type), device_type, store, id, to)
        };
        self.store_order(device_type, &order);
    }

    /// The ids of `device_type`'s order setting.
    fn order_entries(&self, device_type: DeviceType) -> Vec<String> {
        self.imp().settings.get().map_or_else(Vec::new, |settings| {
            settings
                .strv(order::settings_key(device_type))
                .iter()
                .map(|entry| entry.as_str().to_owned())
                .collect()
        })
    }

    /// Stores `order` as `device_type`'s profile order; the list follows the setting.
    fn store_order(&self, device_type: DeviceType, order: &[ProfileId]) {
        let Some(settings) = self.imp().settings.get() else {
            return;
        };
        let ids: Vec<&str> = order.iter().map(ProfileId::as_str).collect();
        if let Err(error) = settings.set_strv(order::settings_key(device_type), ids) {
            tracing::warn!("Cannot store the profile order: {error}");
        }
    }
}
