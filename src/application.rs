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
//
// Adapted from gtk-rs-skills/skills/gtk-rs-new-app/assets/template/src/application.rs at 42eb5e9,
// originally MIT-licensed, Copyright (c) 2026 Roberto Leinardi

//! The `AdwApplication` subclass: application actions, accelerators, the About dialog, and the
//! device service, whose events it turns into the device status the windows show.

use std::path::PathBuf;

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gio, glib};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use x52::clock::SystemClock;
use x52::device::DeviceType;
use x52::input::{HeldInputs, Mode};
use x52::state::DeviceState;

use crate::config;
use crate::device::status::DeviceStatus;
use crate::device::{DeviceEvent, DeviceService, access, names};
use crate::model::{ProfileObject, splice};
use crate::profile::legacy::{self, FileWriter};
use crate::profile::mfd::{self, MfdPreview, ShownClock};
use crate::profile::startup::{self, ImportOutcome};
use crate::profile::store::{Entry, LoadReport, ProfileStore};
use crate::profile::{
    self, LiveValues, MfdLineSource, Profile, ProfileId, ProfileKind, active, list, order,
};
use crate::window::{self, Window};

mod editing;
mod persistence;

pub(crate) use self::persistence::RenameProblem;

/// The settings key that records the one-time GX52 0.7 import as done.
const LEGACY_IMPORT_DONE: &str = "legacy-import-done";

/// The settings key with the type of the last device seen, whose profiles are shown while no
/// device is connected.
const LAST_DEVICE_TYPE: &str = "last-device-type";

/// A running profile import: see [`Application::begin_profile_import`].
pub(crate) struct ImportGuard(glib::WeakRef<Application>);

impl Drop for ImportGuard {
    fn drop(&mut self) {
        if let Some(app) = self.0.upgrade() {
            app.imp().importing.set(false);
        }
    }
}

/// Marks the legacy import's plan completed once the imported settings and the done flag are on
/// disk, so an exit before that point runs the plan again at the next start instead of losing the
/// settings.
fn finish_legacy_import(plan: legacy::Plan) {
    glib::spawn_future_local(async move {
        let finished = gio::spawn_blocking(move || {
            // Waits for the settings backend to write the pending changes.
            gio::Settings::sync();
            startup::finish_import(&profile::config_dir(), &plan, &mut FileWriter);
        })
        .await;
        if finished.is_err() {
            tracing::error!("The task completing the GX52 0.7 import panicked");
        }
    });
}

/// The version of Cargo.toml and meson.build until the release workflow sets the first one
/// (docs/release.md, "One version source").
const UNRELEASED_VERSION: &str = "0.0.0";

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};
    use std::collections::{BTreeMap, BTreeSet};

    use adw::{prelude::*, subclass::prelude::*};
    use gettextrs::gettext;
    use gtk::{gio, glib};
    use x52::input::{HeldInputs, Mode};

    use crate::device::status::DeviceStatus;
    use crate::device::{CurrentSelection, DeviceService};
    use crate::profile::ProfileId;
    use crate::profile::editing::EditingMode;
    use crate::profile::mfd::TextDrafts;
    use crate::profile::removals::Removals;
    use crate::profile::reservations::Reservations;
    use crate::profile::store::ProfileStore;
    use crate::profile::writes::WriteQueue;
    use crate::{config, window::Window};

    #[derive(Debug, Default)]
    pub struct Application {
        /// The running device service, from `startup` to `shutdown`.
        pub(crate) device: RefCell<Option<DeviceService>>,
        /// What the windows show about the device.
        pub(crate) device_status: Cell<DeviceStatus>,
        /// The app's settings, from `startup`.
        pub(crate) settings: OnceCell<gio::Settings>,
        /// The profiles, once loaded.
        pub(crate) profiles: RefCell<Option<ProfileStore>>,
        /// The selected device's mode: the last one reported, kept while the evdev node is
        /// reopened, cleared when the device goes.
        pub(crate) active_mode: Cell<Option<Mode>>,
        /// The mode picked in the mode bar, shown instead of the active mode.
        pub(crate) editing_mode: Cell<EditingMode>,
        /// The selection the device events are followed for; events of an earlier one are
        /// dropped.
        pub(crate) selection: Cell<CurrentSelection>,
        /// Messages for a toast, kept until a window can show them.
        pub(crate) pending_toasts: RefCell<Vec<adw::Toast>>,
        /// Whether a profile import is running, from the file chooser to the write.
        pub(crate) importing: Cell<bool>,
        /// The profile and mode last logged as applied, so a push logs only when they change.
        pub(crate) last_logged: RefCell<Option<(ProfileId, Mode)>>,
        /// The buttons and POV 1 directions held on the selected device.
        pub(crate) held: RefCell<HeldInputs>,
        /// Whether the set last pushed has a "last button" MFD line, so a change of the held
        /// inputs pushes only when the device shows it.
        pub(crate) shows_last_button: Cell<bool>,
        /// The shown device type's profiles, as the profile list shows them.
        pub(crate) profile_list: OnceCell<gio::ListStore>,
        /// Every pending save, delete and create.
        pub(crate) writes: RefCell<WriteQueue>,
        /// The names of profiles not in the store yet, or removed until their file is gone.
        pub(crate) reservations: RefCell<Reservations>,
        /// The running debounce timers, by profile.
        pub(crate) debounces: RefCell<BTreeMap<ProfileId, glib::SourceId>>,
        /// Holds the application while the write queue is busy.
        pub(crate) hold: RefCell<Option<gio::ApplicationHoldGuard>>,
        /// The logout inhibition taken at the end of the session while profiles are saved.
        pub(crate) inhibit_cookie: Cell<Option<u32>>,
        /// The removed profiles whose removal can still be undone.
        pub(crate) removals: RefCell<Removals>,
        /// The profiles whose last save failed, so the failure is reported once.
        pub(crate) failed_saves: RefCell<BTreeSet<ProfileId>>,
        /// The default profiles whose damaged file the user chose to keep this session.
        pub(crate) declined_replacements: RefCell<BTreeSet<ProfileId>>,
        /// Whether the window closed or the session is ending with writes pending, until the
        /// queue is idle or a window opens again: no dialog asks anything meanwhile.
        pub(crate) draining: Cell<bool>,
        /// The text of the custom text MFD lines switched to another source this session.
        pub(crate) text_drafts: RefCell<TextDrafts>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Application {
        const NAME: &'static str = "GX52Application";
        type Type = super::Application;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for Application {}

    impl ApplicationImpl for Application {
        // Runs once per process: register actions and accelerators here.
        fn startup(&self) {
            // Chain up first: it initializes GTK and libadwaita (locale, styles, icons,
            // shortcuts), so translated strings are only looked up after it.
            self.parent_startup();
            glib::set_application_name(&gettext("GX52"));
            gtk::Window::set_default_icon_name(config::APP_ID);
            let app = self.obj();
            app.setup_actions();
            app.setup_settings();
            app.connect_query_end(super::Application::on_query_end);
            app.start_device_service();
            app.load_profiles();
        }

        // Runs on every launch, including a second launch of the single instance:
        // present the existing window instead of opening a new one.
        fn activate(&self) {
            self.parent_activate();
            let app = self.obj();
            let window = app.active_window().unwrap_or_else(|| {
                let window = Window::new(&app);
                window.set_profiles(&app.profile_list());
                window.upcast()
            });
            app.imp().draining.set(false);
            window.present();
            app.show_pending_toasts();
        }

        fn shutdown(&self) {
            // The hold keeps the application alive until the queue is idle; only a forced end
            // gets here with work pending.
            if !self.obj().writes_idle() {
                tracing::warn!("Exiting with profile changes not written");
            }
            // Returns at once: the device threads are joined on a thread of their own.
            if let Some(device) = self.device.take() {
                device.shutdown();
            }
            self.parent_shutdown();
        }
    }

    impl GtkApplicationImpl for Application {}
    impl AdwApplicationImpl for Application {}
}

glib::wrapper! {
    pub struct Application(ObjectSubclass<imp::Application>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Application {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", config::APP_ID)
            // Without this, a .Devel app id would make libadwaita look for style.css,
            // icons and shortcuts-dialog.ui under the wrong resource path.
            .property("resource-base-path", config::RESOURCE_BASE_PATH)
            .build()
    }

    fn setup_actions(&self) {
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| {
                // Close the windows instead of calling quit(): quit() skips close-request,
                // so windows could not save their state. The application exits by itself
                // once its last window is gone.
                for window in app.windows() {
                    window.close();
                }
            })
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about_dialog())
            .build();
        let retry_device = gio::ActionEntry::builder("retry-device")
            .activate(|app: &Self, _, _| {
                if let Some(device) = app.imp().device.borrow().as_ref() {
                    device.retry();
                }
            })
            .build();
        self.add_action_entries([quit, about, retry_device]);

        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
        self.set_accels_for_action("win.new-profile", &["<Control>n"]);
        self.set_accels_for_action("win.import", &["<Control>o"]);
        self.set_accels_for_action("win.export-selected", &["<Control><Shift>e"]);
        for (view, accel) in [
            ("leds", "<Control>1"),
            ("mfd", "<Control>2"),
            ("clocks", "<Control>3"),
        ] {
            self.set_accels_for_action(&format!("win.show-view::{view}"), &[accel]);
        }
    }

    /// Creates the settings and follows the active profiles and the profile order.
    fn setup_settings(&self) {
        let settings = gio::Settings::new(config::APP_ID);
        for device_type in DeviceType::ALL {
            settings.connect_changed(
                Some(active::settings_key(device_type)),
                glib::clone!(
                    #[weak(rename_to = app)]
                    self,
                    move |_, _| {
                        // Another profile is active: the selector's mode is shown again.
                        app.clear_editing_mode();
                        app.push_desired_state();
                        app.refresh_profiles();
                    }
                ),
            );
            settings.connect_changed(
                Some(order::settings_key(device_type)),
                glib::clone!(
                    #[weak(rename_to = app)]
                    self,
                    move |_, _| app.refresh_profiles()
                ),
            );
        }
        // Set once, in startup.
        let _ = self.imp().settings.set(settings);
    }

    /// Starts the device service and follows its events on the main loop.
    fn start_device_service(&self) {
        match DeviceService::start(SystemClock) {
            Ok((device, events)) => {
                self.imp().device.replace(Some(device));
                glib::spawn_future_local(glib::clone!(
                    #[weak(rename_to = app)]
                    self,
                    async move {
                        while let Ok(event) = events.recv().await {
                            app.on_device_event(&event);
                        }
                    }
                ));
            }
            Err(error) => tracing::error!("Cannot start the device service: {error}"),
        }
        // The uinput access is reported on request; for now it is logged once, off the main
        // thread, until the virtual mouse (M7) asks for it.
        glib::spawn_future_local(async {
            if let Ok(access) = gio::spawn_blocking(access::check_uinput).await {
                tracing::info!("uinput access: {access:?}");
            }
        });
    }

    /// Loads the profile store off the main thread.
    fn load_profiles(&self) {
        let import_done = self
            .imp()
            .settings
            .get()
            .is_some_and(|settings| settings.boolean(LEGACY_IMPORT_DONE));
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = app)]
            self,
            async move {
                let prepared = gio::spawn_blocking(move || {
                    startup::prepare_profiles(
                        &profile::config_dir(),
                        &legacy::database_path(),
                        import_done,
                        &mut FileWriter,
                    )
                })
                .await;
                if let Ok(prepared) = prepared {
                    app.on_legacy_import(&prepared.import);
                    app.on_profiles_loaded(prepared.store, &prepared.load_report);
                } else {
                    tracing::error!("The profile loading task panicked");
                }
            }
        ));
    }

    /// Records what the legacy import did: the settings and the done flag when it is over, and
    /// one toast.
    fn on_legacy_import(&self, outcome: &ImportOutcome) {
        let settings = self.imp().settings.get();
        if outcome.is_done()
            && let Some(settings) = settings
        {
            if let ImportOutcome::Completed {
                settings: legacy, ..
            } = outcome
            {
                for (key, value) in [
                    ("autostart", legacy.launch_on_login),
                    ("run-in-background", legacy.run_in_background),
                ] {
                    if let Some(value) = value
                        && let Err(error) = settings.set_boolean(key, value)
                    {
                        tracing::warn!("Cannot import the GX52 0.7 setting {key}: {error}");
                    }
                }
            }
            if let Err(error) = settings.set_boolean(LEGACY_IMPORT_DONE, true) {
                tracing::warn!("Cannot record the GX52 0.7 import as done: {error}");
            }
            if let ImportOutcome::Completed { plan, .. } = outcome {
                finish_legacy_import(plan.clone());
            }
        }
        match outcome {
            ImportOutcome::NotDue => {}
            ImportOutcome::Completed { report, .. } => {
                for problem in &report.problems {
                    tracing::warn!("GX52 0.7 import: {problem}");
                }
                for conflict in &report.conflicts {
                    tracing::warn!(
                        "GX52 0.7 import: row {} of {} not imported, {} was changed meanwhile",
                        conflict.row_id,
                        conflict.table.name(),
                        conflict.target.file_name()
                    );
                }
                let count = u32::try_from(report.imported.len()).unwrap_or(u32::MAX);
                tracing::info!("Imported {count} profiles from GX52 0.7");
                let message = gettextrs::ngettext(
                    "Imported {count} profile from GX52 0.7",
                    "Imported {count} profiles from GX52 0.7",
                    count,
                )
                .replace("{count}", &count.to_string());
                let mut details = report.problems.clone();
                details.extend(report.conflicts.iter().map(|conflict| {
                    gettext(
                        "A profile was not imported because its file was changed meanwhile: {file}",
                    )
                    .replace("{file}", &conflict.target.file_name())
                }));
                if details.is_empty() {
                    self.toast(&message);
                } else {
                    self.toast_with_details(message, details);
                }
            }
            ImportOutcome::DatabaseFailed(error) => {
                tracing::warn!("GX52 0.7 profiles not imported: {error}");
                self.toast(&gettext(
                    "The GX52 0.7 profiles could not be imported: their database cannot be read",
                ));
            }
            ImportOutcome::Deferred(error) => {
                tracing::warn!("GX52 0.7 import deferred: {error}");
                self.toast(&gettext(
                    "The GX52 0.7 profiles could not be imported because GX52 0.7 is running; close it and start GX52 again",
                ));
            }
            ImportOutcome::WriteFailed(error) => {
                tracing::warn!("GX52 0.7 import interrupted: {error}");
                self.toast(&gettext(
                    "The GX52 0.7 profiles could not be imported yet; GX52 tries again at the next start",
                ));
            }
        }
    }

    /// Keeps the loaded store and reports its problems once.
    fn on_profiles_loaded(&self, store: ProfileStore, report: &LoadReport) {
        report.log(&store);
        let mut files: Vec<String> = report
            .failed_files()
            .into_iter()
            .map(str::to_owned)
            .collect();
        files.extend(store.entries().filter_map(|(id, entry)| match entry {
            Entry::Loaded(loaded) if !loaded.problems.is_empty() => Some(id.file_name()),
            Entry::Loaded(_) | Entry::Newer(_) | Entry::Damaged(_) => None,
        }));
        self.imp().profiles.replace(Some(store));
        self.push_desired_state();
        self.refresh_profiles();
        if report.unlisted.is_some() {
            self.toast(&gettext("The profile folder could not be read"));
        }
        if !files.is_empty() {
            let count = u32::try_from(files.len()).unwrap_or(u32::MAX);
            let message = gettextrs::ngettext(
                "A profile file has problems: {files}",
                "{count} profile files have problems: {files}",
                count,
            )
            .replace("{count}", &count.to_string())
            .replace("{files}", &files.join(", "));
            self.toast(&message);
        }
    }

    /// A copy of the usable profile `id`, once the profiles are loaded.
    pub(crate) fn profile(&self, id: &ProfileId) -> Option<Profile> {
        let profiles = self.imp().profiles.borrow();
        Some(profiles.as_ref()?.loaded(id)?.profile.clone())
    }

    /// Marks a profile import as running until the returned guard is dropped; `None` while
    /// another import runs, so two imports never pick the same name.
    pub(crate) fn begin_profile_import(&self) -> Option<ImportGuard> {
        if self.imp().importing.replace(true) {
            None
        } else {
            Some(ImportGuard(self.downgrade()))
        }
    }

    /// The device status the windows show.
    pub(crate) fn device_status(&self) -> DeviceStatus {
        self.imp().device_status.get()
    }

    /// The device type whose profiles are shown: the connected one, or the last one seen; `None`
    /// before any device was seen.
    pub(crate) fn shown_device_type(&self) -> Option<DeviceType> {
        self.device_status().device_type.or_else(|| {
            let settings = self.imp().settings.get()?;
            profile::device_type_from_id(&settings.string(LAST_DEVICE_TYPE))
        })
    }

    /// The shown device type's profiles, as the profile list shows them.
    pub(crate) fn profile_list(&self) -> gio::ListStore {
        self.imp()
            .profile_list
            .get_or_init(gio::ListStore::new::<ProfileObject>)
            .clone()
    }

    /// The id of `device_type`'s active profile, once the profiles are loaded.
    fn active_profile_id(&self, device_type: DeviceType) -> Option<ProfileId> {
        let imp = self.imp();
        let profiles = imp.profiles.borrow();
        let settings = imp.settings.get()?;
        Some(active::resolve(
            &settings.string(active::settings_key(device_type)),
            device_type,
            profiles.as_ref()?,
        ))
    }

    /// The kind of the shown device type's active profile.
    pub(crate) fn active_profile_kind(&self) -> ProfileKind {
        self.shown_device_type()
            .and_then(|device_type| self.active_profile_id(device_type))
            .and_then(|id| Some(self.profile(&id)?.kind()))
            .unwrap_or(ProfileKind::Single)
    }

    /// Makes the usable profile `id` its device type's active profile.
    pub(crate) fn activate_profile(&self, id: &ProfileId) {
        let device_type = {
            let profiles = self.imp().profiles.borrow();
            let Some(loaded) = profiles.as_ref().and_then(|store| store.loaded(id)) else {
                tracing::debug!("Profile {id} cannot be made active: it is not usable");
                return;
            };
            loaded.profile.device_type
        };
        let Some(settings) = self.imp().settings.get() else {
            return;
        };
        self.imp().removals.borrow_mut().forget_active(device_type);
        if let Err(error) = settings.set_string(active::settings_key(device_type), id.as_str()) {
            tracing::warn!("Cannot make profile {id} active: {error}");
        }
    }

    /// The path of the file of `id`.
    pub(crate) fn profile_path(&self, id: &ProfileId) -> PathBuf {
        let profiles = self.imp().profiles.borrow();
        profiles.as_ref().map_or_else(
            || profile::profiles_dir(&profile::config_dir()).join(id.file_name()),
            |store| store.path(id),
        )
    }

    /// Rebuilds the profile list from the store, the order and the active profile, keeping the
    /// objects of the profiles that stay, then refreshes the windows.
    fn refresh_profiles(&self) {
        let imp = self.imp();
        let list = self.profile_list();
        let rows = {
            let profiles = imp.profiles.borrow();
            match (
                self.shown_device_type(),
                profiles.as_ref(),
                imp.settings.get(),
            ) {
                (Some(device_type), Some(store), Some(settings)) => {
                    let entries = settings.strv(order::settings_key(device_type));
                    let entries: Vec<&str> = entries.iter().map(glib::GStringPtr::as_str).collect();
                    list::rows(store, device_type, &entries)
                }
                _ => Vec::new(),
            }
        };
        let current: Vec<ProfileObject> = list
            .iter::<ProfileObject>()
            .filter_map(Result::ok)
            .collect();
        let objects: Vec<ProfileObject> = rows
            .iter()
            .map(|row| {
                current
                    .iter()
                    .find(|object| object.id() == row.id.as_str())
                    .map_or_else(
                        || ProfileObject::new(row),
                        |object| {
                            object.update(row);
                            object.clone()
                        },
                    )
            })
            .collect();
        self.mark_active(&objects);
        // Only the rows that changed are recreated: a row being renamed or holding the focus
        // outside them is kept.
        if let Some(splice) = splice::between(&current, &objects) {
            let position = u32::try_from(splice.position).unwrap_or(u32::MAX);
            let removed = u32::try_from(splice.removed).unwrap_or(u32::MAX);
            list.splice(position, removed, &objects[splice.added]);
        }
        self.update_windows();
    }

    /// Marks the active profile among `objects`, with the mode shown.
    fn mark_active(&self, objects: &[ProfileObject]) {
        let active = self
            .shown_device_type()
            .and_then(|device_type| self.active_profile_id(device_type));
        // The mode the editor and the device show, mode 1 included while the selector is unknown.
        let mode = self.shown_mode();
        for object in objects {
            let is_active = active.as_ref().is_some_and(|id| object.id() == id.as_str());
            object.show_active(is_active, mode);
        }
    }

    /// Marks the active profile in the profile list, with the mode shown, without touching the
    /// list itself.
    fn update_markers(&self) {
        let objects: Vec<ProfileObject> = self
            .profile_list()
            .iter::<ProfileObject>()
            .filter_map(Result::ok)
            .collect();
        self.mark_active(&objects);
    }

    /// The mode whose settings the device and the editor show: the one picked in the mode bar,
    /// else the selector's, else mode 1. A pick counts only while the active profile is
    /// per-mode: one left over from another profile is ignored.
    pub(crate) fn shown_mode(&self) -> Mode {
        let imp = self.imp();
        let mut editing = imp.editing_mode.get();
        if self.active_profile_kind() != ProfileKind::PerMode {
            editing.clear();
        }
        editing.shown(imp.active_mode.get())
    }

    /// Drops the mode picked in the mode bar.
    fn clear_editing_mode(&self) {
        let imp = self.imp();
        let mut editing = imp.editing_mode.get();
        editing.clear();
        imp.editing_mode.set(editing);
    }

    /// Shows `mode`'s settings on the device and in the editor until the selector moves.
    pub(crate) fn pick_editing_mode(&self, mode: Mode) {
        let imp = self.imp();
        let mut editing = imp.editing_mode.get();
        editing.pick(mode, imp.active_mode.get());
        if editing == imp.editing_mode.get() {
            return;
        }
        imp.editing_mode.set(editing);
        self.push_desired_state();
        self.update_markers();
        self.update_windows();
    }

    /// Shows the current state in every window.
    fn update_windows(&self) {
        for window in self.windows() {
            if let Ok(window) = window.downcast::<Window>() {
                window.refresh();
            }
        }
    }

    /// Shows `message` in the active window, or keeps it until a window is presented.
    fn toast(&self, message: &str) {
        self.queue_toast(window::plain_toast(message));
    }

    /// Shows `message` with a "Details" button that lists `details` in a dialog.
    fn toast_with_details(&self, message: String, details: Vec<String>) {
        let toast = adw::Toast::builder()
            .title(message)
            .button_label(gettext("_Details"))
            .use_markup(false)
            .build();
        toast.connect_button_clicked(glib::clone!(
            #[weak(rename_to = app)]
            self,
            move |toast| {
                let dialog =
                    adw::AlertDialog::new(toast.title().as_deref(), Some(&details.join("\n")));
                dialog.add_response("close", &gettext("_Close"));
                dialog.present(app.active_window().as_ref());
            }
        ));
        self.queue_toast(toast);
    }

    fn queue_toast(&self, toast: adw::Toast) {
        self.imp().pending_toasts.borrow_mut().push(toast);
        self.show_pending_toasts();
    }

    fn show_pending_toasts(&self) {
        let Some(window) = self
            .active_window()
            .and_then(|window| window.downcast::<Window>().ok())
        else {
            return;
        };
        let toasts = self.imp().pending_toasts.take();
        for toast in toasts {
            window.add_toast(toast);
        }
    }

    /// Applies a device event: shows a changed status in every window and pushes the desired
    /// state when the device or its mode changed.
    fn on_device_event(&self, event: &DeviceEvent) {
        let imp = self.imp();
        let mut selection = imp.selection.get();
        let current = selection.accept(event);
        imp.selection.set(selection);
        if !current {
            tracing::debug!("Dropped an event of an earlier selection: {event:?}");
            return;
        }
        let mut status = imp.device_status.get();
        let changed = status.apply(event);
        if changed {
            tracing::info!("Device status: {status:?}");
            imp.device_status.set(status);
        }
        match event {
            DeviceEvent::Selected(device) => {
                // A new selection's mode and held inputs are unknown until its reader reports
                // them.
                imp.active_mode.set(None);
                self.clear_editing_mode();
                imp.held.borrow_mut().clear();
                if let Some(settings) = imp.settings.get()
                    && let Err(error) =
                        settings.set_string(LAST_DEVICE_TYPE, device.device_type.id())
                {
                    tracing::warn!("Cannot record the device type: {error}");
                }
                self.push_desired_state();
            }
            DeviceEvent::Mode { mode, .. } => {
                if imp.active_mode.replace(Some(*mode)) != Some(*mode) {
                    // The selector moved: it picks the mode shown again.
                    self.clear_editing_mode();
                    self.push_desired_state();
                }
            }
            DeviceEvent::Gone(_) => {
                imp.active_mode.set(None);
                self.clear_editing_mode();
                imp.held.borrow_mut().clear();
            }
            DeviceEvent::InputNode { node: None, .. } => self.change_held(HeldInputs::clear),
            DeviceEvent::Held { change, .. } => self.change_held(|held| change.apply(held)),
            DeviceEvent::Access(_)
            | DeviceEvent::InputNode { node: Some(_), .. }
            | DeviceEvent::X52dRunning(_)
            | DeviceEvent::Error(_)
            | DeviceEvent::Sent(_) => {}
        }
        if changed {
            // The device type, its mode or its state changed: the list and the footer follow.
            self.refresh_profiles();
        }
    }

    /// Applies `change` to the held inputs; pushes the state when the last held input changed and
    /// the device shows it, and the editor's display follows. Never logs: this runs per input
    /// event.
    fn change_held(&self, change: impl FnOnce(&mut HeldInputs)) {
        let imp = self.imp();
        let changed = {
            let mut held = imp.held.borrow_mut();
            let before = held.last();
            change(&mut held);
            held.last() != before
        };
        if changed && imp.shows_last_button.get() {
            self.push_desired_state();
            self.update_mfd_previews();
        }
    }

    /// The values the dynamic MFD lines show in `mode`.
    fn live_values(&self, mode: Mode) -> LiveValues {
        LiveValues {
            mode_shown: mode,
            last_button: self.imp().held.borrow().last().map(names::input_name),
        }
    }

    /// The desired state of `device_type`'s active profile in `mode`, once the profiles are
    /// loaded; `None` when the profile is not usable.
    fn desired_state(
        &self,
        device_type: DeviceType,
        mode: Mode,
    ) -> Option<(ProfileId, DeviceState)> {
        let imp = self.imp();
        let profiles = imp.profiles.borrow();
        let settings = imp.settings.get()?;
        active::state_for(
            profiles.as_ref()?,
            &settings.string(active::settings_key(device_type)),
            device_type,
            mode,
            &self.live_values(mode),
        )
    }

    /// What the editor's simulated MFD shows at `now`, with `local` as the system time zone and
    /// `shown_clock` in the clock area: the shown device type's active profile in the mode shown,
    /// with or without a device, as the device shows it.
    pub(crate) fn mfd_preview(
        &self,
        shown_clock: ShownClock,
        now: Timestamp,
        local: &TimeZone,
    ) -> Option<MfdPreview> {
        let device_type = self.shown_device_type()?;
        let (_, state) = self.desired_state(device_type, self.shown_mode())?;
        mfd::preview(&state, device_type, shown_clock, now, local)
            .inspect_err(|error| tracing::warn!("Cannot show the MFD clocks: {error}"))
            .ok()
    }

    /// Shows the dynamic MFD lines again in every window's display, and nothing else.
    fn update_mfd_previews(&self) {
        for window in self.windows() {
            if let Ok(window) = window.downcast::<Window>() {
                window.refresh_mfd();
            }
        }
    }

    /// Sends the device its type's active profile, in the set of the mode shown (mode 1 until the
    /// mode is known), or the built-in default until the profiles are loaded.
    ///
    /// Logs only when the profile or the mode changes, never per push.
    fn push_desired_state(&self) {
        let imp = self.imp();
        let Some(device_type) = imp.device_status.get().device_type else {
            return;
        };
        let device = imp.device.borrow();
        let Some(device) = device.as_ref() else {
            return;
        };
        let mode = self.shown_mode();
        let resolved = self.desired_state(device_type, mode);
        let shows_last_button = resolved.as_ref().is_some_and(|(id, _)| {
            self.profile(id).is_some_and(|profile| {
                profile
                    .settings_for(mode)
                    .mfd_lines
                    .contains(&MfdLineSource::LastButton)
            })
        });
        imp.shows_last_button.set(shows_last_button);
        let state = match resolved {
            Some((id, state)) => {
                let applied = (id, mode);
                if imp.last_logged.borrow().as_ref() != Some(&applied) {
                    tracing::info!("Mode {mode:?} on the {device_type}: profile {}", applied.0);
                    imp.last_logged.replace(Some(applied));
                }
                state
            }
            None => DeviceState::default_for(device_type),
        };
        device.set_state(state);
    }

    fn show_about_dialog(&self) {
        // The metainfo's <release> entries hold the plain version; a Devel build appends a
        // suffix (-devel or the commit hash) that from_appdata() would not find.
        let release = config::VERSION.split('-').next().unwrap_or(config::VERSION);
        // A build the release workflow has not versioned yet has no <release> entry to show the
        // notes of (from_appdata() logs a critical for a version it cannot find).
        let release_notes = (release != UNRELEASED_VERSION).then_some(release);
        let dialog = adw::AboutDialog::from_appdata(
            &format!("{}/metainfo.xml", config::RESOURCE_BASE_PATH),
            release_notes,
        );
        dialog.set_version(config::VERSION);
        dialog.set_developers(&["Roberto Leinardi"]);
        // Translators: Replace "translator-credits" with your name/username, and optionally an
        // email or URL.
        dialog.set_translator_credits(&gettext("translator-credits"));
        dialog.present(self.active_window().as_ref());
    }
}
