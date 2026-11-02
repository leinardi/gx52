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

//! The `AdwApplication` subclass: application actions, accelerators and the About dialog.

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config;

/// The version of Cargo.toml and meson.build until the release workflow sets the first one
/// (docs/release.md, "One version source").
const UNRELEASED_VERSION: &str = "0.0.0";

mod imp {
    use adw::{prelude::*, subclass::prelude::*};
    use gettextrs::gettext;
    use gtk::glib;

    use crate::{config, window::Window};

    #[derive(Debug, Default)]
    pub struct Application;

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
            self.obj().setup_actions();
        }

        // Runs on every launch, including a second launch of the single instance:
        // present the existing window instead of opening a new one.
        fn activate(&self) {
            self.parent_activate();
            let app = self.obj();
            let window = app
                .active_window()
                .unwrap_or_else(|| Window::new(&app).upcast());
            window.present();
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
        self.add_action_entries([quit, about]);

        self.set_accels_for_action("app.quit", &["<Control>q"]);
        self.set_accels_for_action("window.close", &["<Control>w"]);
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
