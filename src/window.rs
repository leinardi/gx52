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
// Adapted from gtk-rs-skills/skills/gtk-rs-new-app/assets/template/src/window.rs at 42eb5e9,
// originally MIT-licensed, Copyright (c) 2026 Roberto Leinardi

//! The main window, with its size and maximized state kept in `GSettings`, showing the device
//! status.

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use gettextrs::gettext;
use x52::device::DeviceType;

use crate::profile::exchange::{self, ImportPreview};
use crate::profile::{self, ProfileId, files};
use crate::{application::Application, config, device::status::DeviceStatus};

mod imp {
    use std::cell::OnceCell;

    use adw::{prelude::*, subclass::prelude::*};
    use gtk::{gio, glib};

    use crate::config;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/window.ui")]
    pub struct Window {
        pub settings: OnceCell<gio::Settings>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub x52d_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub status_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub retry_button: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "GX52Window";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for Window {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            if config::is_devel() {
                obj.add_css_class("devel");
            }
            obj.setup_actions();
            obj.load_window_state();
        }
    }

    impl WidgetImpl for Window {}

    impl WindowImpl for Window {
        fn close_request(&self) -> glib::Propagation {
            if let Err(err) = self.obj().save_window_state() {
                tracing::warn!("Failed to save the window state: {err}");
            }
            self.parent_close_request()
        }
    }

    impl ApplicationWindowImpl for Window {}
    impl AdwApplicationWindowImpl for Window {}
}

/// The filter of the import and export dialogs: profile files.
fn profile_filter() -> gtk::FileFilter {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(&gettext("GX52 profiles")));
    filter.add_suffix("toml");
    filter
}

glib::wrapper! {
    pub struct Window(ObjectSubclass<imp::Window>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl Window {
    pub fn new(app: &Application) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    /// Shows `status`: the status line, the `x52d` warning and "Try again" when it applies.
    pub(crate) fn show_device_status(&self, status: DeviceStatus) {
        let imp = self.imp();
        imp.status_page.set_description(Some(&status.status_line()));
        imp.retry_button.set_visible(status.offers_retry());
        match status.x52d_warning() {
            Some(warning) => {
                imp.x52d_banner.set_title(&warning);
                imp.x52d_banner.set_revealed(true);
            }
            None => imp.x52d_banner.set_revealed(false),
        }
    }

    /// Shows `message` in a toast.
    pub(crate) fn show_toast(&self, message: &str) {
        self.imp().toast_overlay.add_toast(adw::Toast::new(message));
    }

    /// `win.import-profile` (a device type id: `x52-pro` or `x52`) and `win.export-profile` (a
    /// profile id). The profile editor of M5 puts them in its profile list; until then they are
    /// reached through the action names only.
    fn setup_actions(&self) {
        let import = gio::ActionEntry::builder("import-profile")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|window: &Self, _, parameter| {
                let Some(device_type) = parameter
                    .and_then(glib::Variant::get::<String>)
                    .and_then(|id| profile::device_type_from_id(&id))
                else {
                    return;
                };
                glib::spawn_future_local(glib::clone!(
                    #[weak]
                    window,
                    async move { window.import_profile(device_type).await }
                ));
            })
            .build();
        let export = gio::ActionEntry::builder("export-profile")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|window: &Self, _, parameter| {
                let Some(id) = parameter
                    .and_then(glib::Variant::get::<String>)
                    .and_then(|id| ProfileId::parse(&id))
                else {
                    return;
                };
                glib::spawn_future_local(glib::clone!(
                    #[weak]
                    window,
                    async move { window.export_profile(&id).await }
                ));
            })
            .build();
        self.add_action_entries([import, export]);
    }

    fn app(&self) -> Option<Application> {
        self.application()?.downcast::<Application>().ok()
    }

    /// Picks a file, checks it, asks for confirmation with the problems listed, then imports it
    /// into `device_type`'s profiles.
    async fn import_profile(&self, device_type: DeviceType) {
        let Some(app) = self.app() else {
            return;
        };
        let (Some(taken), Some(dir)) = (app.profile_names(device_type), app.profiles_dir()) else {
            self.show_toast(&gettext("The profiles are still loading"));
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Import Profile"))
            .modal(true)
            .default_filter(&profile_filter())
            .build();
        let file = match dialog.open_future(Some(self)).await {
            Ok(file) => file,
            Err(error) => return self.file_dialog_failed(&error),
        };
        let Some(path) = file.path() else {
            self.show_toast(&gettext("The file cannot be read"));
            return;
        };
        let prepared = gio::spawn_blocking(move || {
            let bytes = files::read_limited(&path, files::PROFILE_LIMIT)?;
            exchange::prepare_import(&bytes, device_type, &taken)
        })
        .await;
        let preview = match prepared {
            Ok(Ok(preview)) => preview,
            Ok(Err(refusal)) => {
                tracing::info!("Profile import refused: {refusal}");
                return self.show_toast(&refusal.message());
            }
            Err(_) => return tracing::error!("The profile import task panicked"),
        };
        if !self.confirm_import(&preview).await {
            return;
        }
        let name = preview.profile.name.clone();
        let written = gio::spawn_blocking(move || {
            exchange::write_import(&preview, &dir).map(|id| (id, preview.profile))
        })
        .await;
        match written {
            Ok(Ok((id, profile))) => {
                tracing::info!("Profile {id} imported");
                app.add_imported_profile(id, profile);
                self.show_toast(&gettext("Imported “{name}”").replace("{name}", &name));
            }
            Ok(Err(error)) => {
                tracing::warn!("Cannot write the imported profile: {error}");
                self.show_toast(&gettext("The profile could not be saved"));
            }
            Err(_) => tracing::error!("The profile import task panicked"),
        }
    }

    /// Shows what will be imported, with every problem the file had; true when confirmed.
    async fn confirm_import(&self, preview: &ImportPreview) -> bool {
        let mut body = Vec::new();
        if let Some(original) = &preview.renamed_from {
            body.push(
                gettext("A profile named “{name}” already exists.").replace("{name}", original),
            );
        }
        if !preview.problems.is_empty() {
            body.push(gettext(
                "Some values in the file are not valid and are replaced by the defaults:",
            ));
            body.extend(
                preview
                    .problems
                    .iter()
                    .map(|problem| format!("• {}", problem.message())),
            );
        }
        let heading = gettext("Import “{name}”?").replace("{name}", &preview.profile.name);
        let dialog = adw::AlertDialog::new(Some(&heading), Some(&body.join("\n")));
        dialog.add_responses(&[
            ("cancel", &gettext("_Cancel")),
            ("import", &gettext("_Import")),
        ]);
        dialog.set_response_appearance("import", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("import"));
        dialog.set_close_response("cancel");
        dialog.choose_future(Some(self)).await == "import"
    }

    /// Saves the profile `id` to a file the user picks.
    async fn export_profile(&self, id: &ProfileId) {
        let Some(profile) = self.app().and_then(|app| app.profile(id)) else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title(gettext("Export Profile"))
            .modal(true)
            .default_filter(&profile_filter())
            .initial_name(format!("{}.toml", profile.name.replace('/', "-")))
            .build();
        let file = match dialog.save_future(Some(self)).await {
            Ok(file) => file,
            Err(error) => return self.file_dialog_failed(&error),
        };
        let contents = exchange::export(&profile).into_bytes();
        match file
            .replace_contents_future(
                contents,
                None,
                false,
                gio::FileCreateFlags::REPLACE_DESTINATION,
            )
            .await
        {
            Ok(_) => {
                self.show_toast(&gettext("Exported “{name}”").replace("{name}", &profile.name));
            }
            Err((_, error)) => {
                tracing::warn!("Cannot export profile {id}: {error}");
                self.show_toast(&gettext("The profile could not be exported"));
            }
        }
    }

    /// A file dialog that did not return a file: nothing to do when the user dismissed it.
    fn file_dialog_failed(&self, error: &glib::Error) {
        if !error.matches(gtk::DialogError::Dismissed) {
            tracing::warn!("File dialog failed: {error}");
            self.show_toast(&gettext("The file cannot be opened"));
        }
    }

    fn settings(&self) -> &gio::Settings {
        self.imp()
            .settings
            .get_or_init(|| gio::Settings::new(config::APP_ID))
    }

    fn load_window_state(&self) {
        let settings = self.settings();
        self.set_default_size(settings.int("window-width"), settings.int("window-height"));
        if settings.boolean("is-maximized") {
            self.maximize();
        }
    }

    fn save_window_state(&self) -> Result<(), glib::BoolError> {
        let settings = self.settings();
        let (width, height) = self.default_size();
        settings.set_int("window-width", width)?;
        settings.set_int("window-height", height)?;
        settings.set_boolean("is-maximized", self.is_maximized())?;
        Ok(())
    }
}
