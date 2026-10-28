use adw::{prelude::*, subclass::prelude::*};
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::{config, window::Window};

mod imp {
    use super::*;

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
            // Chain up first: it initializes GTK and libadwaita (styles, icons, shortcuts).
            self.parent_startup();
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
                // so windows could not save their state or veto (unsaved changes). The
                // application exits by itself once its last window is gone.
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
        self.set_accels_for_action("win.say-hello", &["<Control>h"]);
    }

    fn show_about_dialog(&self) {
        let dialog = adw::AboutDialog::from_appdata(
            &format!("{}/metainfo.xml", config::RESOURCE_BASE_PATH),
            Some(config::VERSION.split('-').next().unwrap_or(config::VERSION)),
        );
        dialog.set_version(config::VERSION);
        dialog.set_developers(&["Roberto Leinardi"]);
        // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
        dialog.set_translator_credits(&gettext("translator-credits"));
        dialog.present(self.active_window().as_ref());
    }
}
