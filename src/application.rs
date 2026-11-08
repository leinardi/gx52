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

use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gio, glib};
use x52::clock::SystemClock;
use x52::device::DeviceType;
use x52::input::Mode;
use x52::state::DeviceState;

use crate::config;
use crate::device::{DeviceEvent, DeviceService, access};
use crate::profile::legacy::{self, FileWriter};
use crate::profile::startup::{self, ImportOutcome};
use crate::profile::store::{Entry, LoadReport, ProfileStore};
use crate::profile::{self, mapping};
use crate::window::Window;

/// The settings key that records the one-time GX52 0.7 import as done.
const LEGACY_IMPORT_DONE: &str = "legacy-import-done";

/// The version of Cargo.toml and meson.build until the release workflow sets the first one
/// (docs/release.md, "One version source").
const UNRELEASED_VERSION: &str = "0.0.0";

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use adw::{prelude::*, subclass::prelude::*};
    use gettextrs::gettext;
    use gtk::{gio, glib};
    use x52::input::Mode;

    use crate::device::DeviceService;
    use crate::device::status::DeviceStatus;
    use crate::profile::store::ProfileStore;
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
        /// Messages for a toast, kept until a window can show them.
        pub(crate) pending_toasts: RefCell<Vec<String>>,
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
                window.show_device_status(self.device_status.get());
                window.upcast()
            });
            window.present();
            app.show_pending_toasts();
        }

        fn shutdown(&self) {
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
    }

    /// Creates the settings and follows the mode mappings.
    fn setup_settings(&self) {
        let settings = gio::Settings::new(config::APP_ID);
        for device_type in DeviceType::ALL {
            settings.connect_changed(
                Some(mapping::settings_key(device_type)),
                glib::clone!(
                    #[weak(rename_to = app)]
                    self,
                    move |_, _| app.push_desired_state()
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
                    ("show-tray", legacy.show_app_indicator),
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
                let message = if report.problems.is_empty() && report.conflicts.is_empty() {
                    gettextrs::ngettext(
                        "Imported {count} profile from GX52 0.7",
                        "Imported {count} profiles from GX52 0.7",
                        count,
                    )
                } else {
                    gettextrs::ngettext(
                        "Imported {count} profile from GX52 0.7, with problems listed in the log",
                        "Imported {count} profiles from GX52 0.7, with problems listed in the log",
                        count,
                    )
                };
                self.toast(message.replace("{count}", &count.to_string()));
            }
            ImportOutcome::DatabaseFailed(error) => {
                tracing::warn!("GX52 0.7 profiles not imported: {error}");
                self.toast(gettext(
                    "The GX52 0.7 profiles could not be imported: their database cannot be read",
                ));
            }
            ImportOutcome::WriteFailed(error) => {
                tracing::warn!("GX52 0.7 import interrupted: {error}");
                self.toast(gettext(
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
            Entry::Loaded(_) | Entry::Newer(_) => None,
        }));
        self.imp().profiles.replace(Some(store));
        self.push_desired_state();
        if report.unlisted.is_some() {
            self.toast(gettext("The profile folder could not be read"));
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
            self.toast(message);
        }
    }

    /// Shows `message` in the active window, or keeps it until a window is presented.
    fn toast(&self, message: String) {
        self.imp().pending_toasts.borrow_mut().push(message);
        self.show_pending_toasts();
    }

    fn show_pending_toasts(&self) {
        let Some(window) = self
            .active_window()
            .and_then(|window| window.downcast::<Window>().ok())
        else {
            return;
        };
        let messages = self.imp().pending_toasts.take();
        for message in messages {
            window.show_toast(&message);
        }
    }

    /// Applies a device event: shows a changed status in every window and pushes the desired
    /// state when the device or its mode changed.
    fn on_device_event(&self, event: &DeviceEvent) {
        let imp = self.imp();
        let mut status = imp.device_status.get();
        if status.apply(event) {
            tracing::info!("Device status: {status:?}");
            imp.device_status.set(status);
            for window in self.windows() {
                if let Ok(window) = window.downcast::<Window>() {
                    window.show_device_status(status);
                }
            }
        }
        match event {
            DeviceEvent::Selected(_) => self.push_desired_state(),
            DeviceEvent::Mode(mode) => {
                if imp.active_mode.replace(Some(*mode)) != Some(*mode) {
                    self.push_desired_state();
                }
            }
            DeviceEvent::Gone(_) => imp.active_mode.set(None),
            DeviceEvent::Access(_)
            | DeviceEvent::InputNode(_)
            | DeviceEvent::X52dRunning(_)
            | DeviceEvent::Error(_)
            | DeviceEvent::Sent(_) => {}
        }
    }

    /// Sends the device the profile its type's mapping gives the active mode (mode 1 until the
    /// mode is known), or the built-in default until the profiles are loaded.
    fn push_desired_state(&self) {
        let imp = self.imp();
        let Some(device_type) = imp.device_status.get().device_type else {
            return;
        };
        let device = imp.device.borrow();
        let Some(device) = device.as_ref() else {
            return;
        };
        let mode = imp.active_mode.get().unwrap_or(Mode::One);
        let profiles = imp.profiles.borrow();
        let state = match (profiles.as_ref(), imp.settings.get()) {
            (Some(store), Some(settings)) => {
                let entries: Vec<String> = settings
                    .strv(mapping::settings_key(device_type))
                    .iter()
                    .map(ToString::to_string)
                    .collect();
                let id = mapping::resolve(&entries, device_type, mode, store);
                tracing::info!("Mode {mode:?} on the {device_type}: profile {id}");
                store
                    .loaded(&id)
                    .map(|loaded| loaded.profile.to_device_state())
            }
            _ => None,
        };
        device.set_state(state.unwrap_or_else(|| DeviceState::default_for(device_type)));
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
