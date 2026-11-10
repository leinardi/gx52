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

//! The main window: the profile list in the sidebar, the editor of the active profile, and the
//! problem states of docs/ui.md, with its size and maximized state kept in `GSettings`.

use adw::{prelude::*, subclass::prelude::*};
use gtk::{gio, glib};

use gettextrs::gettext;
use x52::device::DeviceType;

use crate::model::{ProfileObject, ProfileState};
use crate::profile::exchange::{self, ImportPreview};
use crate::profile::{self, ProfileId, files};
use crate::widgets::ProfileRow;
use crate::{application::Application, config};

/// The command that stops and disables libx52's daemon (docs/protocol.md, "libx52's x52d").
const X52D_DISABLE: &str = "sudo systemctl disable --now x52d";

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use adw::{prelude::*, subclass::prelude::*};
    use gtk::{gio, glib};

    use crate::config;
    use crate::widgets::DeviceFooter;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/window.ui")]
    pub struct Window {
        pub settings: OnceCell<gio::Settings>,
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub main_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub split_view: TemplateChild<adw::NavigationSplitView>,
        #[template_child]
        pub profile_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub footer: TemplateChild<DeviceFooter>,
        #[template_child]
        pub content_page: TemplateChild<adw::NavigationPage>,
        #[template_child]
        pub x52d_banner: TemplateChild<adw::Banner>,
        #[template_child]
        pub content_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub failed_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub status_page: TemplateChild<adw::StatusPage>,
        /// The profiles shown, from the application.
        pub profiles: OnceCell<gio::ListStore>,
        /// Set while the window selects a row itself, so the selection is not taken as the
        /// user's.
        pub syncing: Cell<bool>,
        /// The id of the profile that failed to load and is selected to show why; the active
        /// profile is selected otherwise.
        pub selected_failed: RefCell<Option<String>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Window {
        const NAME: &'static str = "GX52Window";
        type Type = super::Window;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            DeviceFooter::ensure_type();
            klass.bind_template();
            klass.install_action(
                "win.show-view",
                Some(glib::VariantTy::STRING),
                |window, _, parameter| {
                    if let Some(name) = parameter.and_then(glib::Variant::get::<String>) {
                        window.imp().view_stack.set_visible_child_name(&name);
                    }
                },
            );
            klass.install_action("win.x52d-help", None, |window, _, _| {
                window.show_x52d_help();
            });
            klass.install_action_async("win.show-selected-file", None, |window, _, _| async move {
                window.show_selected_file().await;
            });
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
            obj.setup_profile_list();
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

/// A toast showing `message` as text: profile and file names may contain `&` or `<`, which a
/// markup title would drop or misrender.
pub(crate) fn plain_toast(message: &str) -> adw::Toast {
    adw::Toast::builder()
        .title(message)
        .use_markup(false)
        .build()
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

    /// Shows `profiles`, the application's list of the shown device type's profiles.
    pub(crate) fn set_profiles(&self, profiles: &gio::ListStore) {
        let imp = self.imp();
        if imp.profiles.set(profiles.clone()).is_err() {
            return;
        }
        imp.profile_list.bind_model(Some(profiles), |item| {
            item.downcast_ref::<ProfileObject>().map_or_else(
                || gtk::Label::new(None).upcast(),
                |item| ProfileRow::new(item).upcast(),
            )
        });
        self.refresh();
    }

    /// Shows the application's state: which page, the selected profile, the device status.
    pub(crate) fn refresh(&self) {
        let Some(app) = self.app() else {
            return;
        };
        let imp = self.imp();
        let status = app.device_status();
        if app.shown_device_type().is_none() {
            imp.main_stack.set_visible_child_name("waiting");
            return;
        }
        imp.main_stack.set_visible_child_name("profiles");
        imp.footer.show(&status.footer(app.active_profile_kind()));
        self.select_current();
        self.show_selected();
    }

    /// Selects the failed profile the user picked while it is still listed, the active profile
    /// otherwise.
    fn select_current(&self) {
        let imp = self.imp();
        let failed = imp.selected_failed.borrow().clone();
        let target = failed
            .and_then(|id| self.position_of(|item| item.id() == id && !item.is_usable()))
            .or_else(|| self.position_of(ProfileObject::active));
        if target.is_none() {
            imp.selected_failed.replace(None);
        }
        imp.syncing.set(true);
        let row = target
            .and_then(|position| i32::try_from(position).ok())
            .and_then(|position| imp.profile_list.row_at_index(position));
        imp.profile_list.select_row(row.as_ref());
        imp.syncing.set(false);
    }

    /// The position of the first profile `matches` accepts.
    fn position_of(&self, matches: impl Fn(&ProfileObject) -> bool) -> Option<u32> {
        let profiles = self.imp().profiles.get()?;
        (0..profiles.n_items()).find(|position| {
            profiles
                .item(*position)
                .and_downcast::<ProfileObject>()
                .is_some_and(|item| matches(&item))
        })
    }

    /// The selected profile.
    fn selected(&self) -> Option<ProfileObject> {
        let imp = self.imp();
        let row = imp.profile_list.selected_row()?;
        let position = u32::try_from(row.index()).ok()?;
        imp.profiles.get()?.item(position).and_downcast()
    }

    /// Shows the content of the selected profile: its editor, why it failed to load, or the
    /// missing permission.
    fn show_selected(&self) {
        let imp = self.imp();
        let Some(app) = self.app() else {
            return;
        };
        let status = app.device_status();
        let selected = self.selected();
        imp.content_page.set_title(
            &selected
                .as_ref()
                .map_or_else(|| gettext("Profile"), ProfileObject::title),
        );
        let page = if status.offers_retry() {
            imp.status_page.set_description(Some(&gettext(
                "GX52 is not allowed to control the joystick. Install the udev rule, then try again.",
            )));
            "no-permission"
        } else {
            match selected.as_ref().map(ProfileObject::state) {
                Some(ProfileState::Newer) => {
                    imp.failed_page.set_title(&gettext("Made by a Newer GX52"));
                    imp.failed_page.set_description(Some(&gettext(
                        "This profile can be used once GX52 is updated. It is never changed here.",
                    )));
                    "failed"
                }
                Some(ProfileState::Damaged) => {
                    imp.failed_page
                        .set_title(&gettext("This Profile File Is Damaged"));
                    imp.failed_page.set_description(Some(&gettext(
                        "GX52 cannot read it. The file is kept as it is.",
                    )));
                    "failed"
                }
                Some(ProfileState::Usable) | None => "editor",
            }
        };
        imp.content_stack.set_visible_child_name(page);
        imp.x52d_banner
            .set_revealed(status.x52d_running && page == "editor");
    }

    /// Follows the list: a usable profile becomes the active one, a failed one shows why.
    fn setup_profile_list(&self) {
        let imp = self.imp();
        imp.profile_list.connect_row_selected(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, row| {
                if row.is_none() || window.imp().syncing.get() {
                    return;
                }
                window.on_profile_selected();
            }
        ));
        imp.profile_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |_, _| window.imp().split_view.set_show_content(true)
        ));
    }

    fn on_profile_selected(&self) {
        let Some(item) = self.selected() else {
            return;
        };
        let imp = self.imp();
        if item.is_usable() {
            imp.selected_failed.replace(None);
            if let (Some(app), Some(id)) = (self.app(), item.profile_id()) {
                app.activate_profile(&id);
            }
        } else {
            imp.selected_failed.replace(Some(item.id()));
        }
        self.show_selected();
    }

    /// Explains `x52d` and how to stop it (docs/ui.md, "Problem states").
    fn show_x52d_help(&self) {
        let dialog = adw::AlertDialog::new(
            Some(&gettext("x52d Is Running")),
            Some(&gettext(
                "x52d is the daemon of libx52, another program for the X52. While it runs, it may \
                 overwrite the LEDs, the MFD and the clocks that GX52 sets. GX52 does not stop \
                 it; to stop and disable it, run:",
            )),
        );
        let command = gtk::Label::builder()
            .label(X52D_DISABLE)
            .selectable(true)
            .wrap(true)
            .hexpand(true)
            .xalign(0.0)
            .css_classes(["monospace"])
            .build();
        let copy = gtk::Button::builder()
            .icon_name("edit-copy-symbolic")
            .tooltip_text(gettext("Copy"))
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        copy.connect_clicked(glib::clone!(
            #[weak(rename_to = window)]
            self,
            move |button| {
                button.clipboard().set_text(X52D_DISABLE);
                window.show_toast(&gettext("Copied to clipboard"));
            }
        ));
        let line = gtk::Box::builder().spacing(6).css_classes(["card"]).build();
        line.set_margin_top(6);
        command.set_margin_start(12);
        command.set_margin_top(6);
        command.set_margin_bottom(6);
        line.append(&command);
        line.append(&copy);
        dialog.set_extra_child(Some(&line));
        dialog.add_response("close", &gettext("_Close"));
        dialog.present(Some(self));
    }

    /// Opens the folder of the selected profile's file, through the portal.
    async fn show_selected_file(&self) {
        let Some(path) = self
            .selected()
            .and_then(|item| item.profile_id())
            .and_then(|id| Some(self.app()?.profile_path(&id)))
        else {
            return;
        };
        let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(path)));
        if let Err(error) = launcher.open_containing_folder_future(Some(self)).await
            && !error.matches(gtk::DialogError::Dismissed)
        {
            tracing::warn!("Cannot show the profile file: {error}");
            self.show_toast(&gettext("The folder cannot be opened"));
        }
    }

    /// Shows `message` in a toast.
    pub(crate) fn show_toast(&self, message: &str) {
        self.add_toast(plain_toast(message));
    }

    /// Shows `toast`.
    pub(crate) fn add_toast(&self, toast: adw::Toast) {
        self.imp().toast_overlay.add_toast(toast);
    }

    /// `win.import-profile` (a device type id: `x52-pro` or `x52`) and `win.export-profile` (a
    /// profile id).
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
        // Held until the end of the import: two imports at once could both keep the same name.
        let Some(_import) = app.begin_profile_import() else {
            self.show_toast(&gettext("Another profile import is in progress"));
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
        let mut preview = match prepared {
            Ok(Ok(preview)) => preview,
            Ok(Err(refusal)) => {
                tracing::info!("Profile import refused: {refusal}");
                return self.show_toast(&refusal.message());
            }
            Err(_) => return tracing::error!("The profile import task panicked"),
        };
        // Confirmed against the names in the store right before the write; a profile added
        // while the dialog was open changes the name, and the user confirms the new one.
        loop {
            if !self.confirm_import(&preview).await {
                return;
            }
            let Some(current) = app.profile_names(device_type) else {
                return;
            };
            if !exchange::revalidate_name(&mut preview, &current) {
                break;
            }
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
