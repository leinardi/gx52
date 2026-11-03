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
use x52::state::DeviceState;

use crate::config;
use crate::device::{DeviceEvent, DeviceService, access};
use crate::window::Window;

/// The version of Cargo.toml and meson.build until the release workflow sets the first one
/// (docs/release.md, "One version source").
const UNRELEASED_VERSION: &str = "0.0.0";

mod imp {
    use std::cell::{Cell, RefCell};

    use adw::{prelude::*, subclass::prelude::*};
    use gettextrs::gettext;
    use gtk::glib;

    use crate::device::DeviceService;
    use crate::device::status::DeviceStatus;
    use crate::{config, window::Window};

    #[derive(Debug, Default)]
    pub struct Application {
        /// The running device service, from `startup` to `shutdown`.
        pub(crate) device: RefCell<Option<DeviceService>>,
        /// What the windows show about the device.
        pub(crate) device_status: Cell<DeviceStatus>,
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
            app.start_device_service();
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

    /// Applies a device event: pushes the default state of a newly selected device type and
    /// shows a changed status in every window.
    fn on_device_event(&self, event: &DeviceEvent) {
        let imp = self.imp();
        if let DeviceEvent::DeviceType(Some(device_type)) = event
            && let Some(device) = imp.device.borrow().as_ref()
        {
            device.set_state(DeviceState::default_for(*device_type));
        }
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
