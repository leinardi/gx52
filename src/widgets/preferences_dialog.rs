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

//! The preferences dialog (docs/ui.md, "Preferences"): every row bound to its `GSettings`
//! key.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};

use crate::preferences::{
    self, AUTOSTART, RUN_IN_BACKGROUND, VIRTUAL_MOUSE, VIRTUAL_MOUSE_CURVE, VIRTUAL_MOUSE_DEADZONE,
    VIRTUAL_MOUSE_INVERT_WHEEL, VIRTUAL_MOUSE_SPEED,
};

mod imp {
    use adw::subclass::prelude::*;
    use gtk::glib;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/preferences-dialog.ui")]
    pub struct PreferencesDialog {
        #[template_child]
        pub(super) run_in_background: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) autostart: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) virtual_mouse: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) speed: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) curve: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) deadzone: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) invert_wheel: TemplateChild<adw::SwitchRow>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PreferencesDialog {
        const NAME: &'static str = "GX52PreferencesDialog";
        type Type = super::PreferencesDialog;
        type ParentType = adw::PreferencesDialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PreferencesDialog {}
    impl WidgetImpl for PreferencesDialog {}
    impl AdwDialogImpl for PreferencesDialog {}
    impl PreferencesDialogImpl for PreferencesDialog {}
}

glib::wrapper! {
    pub struct PreferencesDialog(ObjectSubclass<imp::PreferencesDialog>)
        @extends adw::PreferencesDialog, adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl PreferencesDialog {
    /// A dialog showing and changing `settings`.
    pub(crate) fn new(settings: &gio::Settings) -> Self {
        let dialog: Self = glib::Object::new();
        dialog.bind(settings);
        dialog
    }

    /// Binds each row to its key; the bindings go with the rows.
    fn bind(&self, settings: &gio::Settings) {
        let imp = self.imp();
        for (key, row) in [
            (RUN_IN_BACKGROUND, &*imp.run_in_background),
            (AUTOSTART, &*imp.autostart),
            (VIRTUAL_MOUSE, &*imp.virtual_mouse),
            (VIRTUAL_MOUSE_INVERT_WHEEL, &*imp.invert_wheel),
        ] {
            settings.bind(key, row, "active").build();
        }
        // Starting hidden needs running in the background (preferences::follow_background turns
        // it off with it).
        settings
            .bind(RUN_IN_BACKGROUND, &*imp.autostart, "sensitive")
            .get_only()
            .build();

        let schema = settings.settings_schema();
        for (key, row) in [
            (VIRTUAL_MOUSE_SPEED, &*imp.speed),
            (VIRTUAL_MOUSE_CURVE, &*imp.curve),
            (VIRTUAL_MOUSE_DEADZONE, &*imp.deadzone),
        ] {
            // The bounds first: binding to an adjustment without them would clamp the value
            // and write it back to the key.
            let Some((lower, upper)) = schema
                .as_ref()
                .and_then(|schema| preferences::spin_bounds(&schema.key(key)))
            else {
                // The schema is compiled with the app; preferences::tests checks every range.
                tracing::warn!("The settings key {key} has no range; its row stays off");
                row.set_sensitive(false);
                continue;
            };
            let adjustment = row.adjustment();
            adjustment.set_lower(lower);
            adjustment.set_upper(upper);
            settings.bind(key, row, "value").build();
            settings
                .bind(VIRTUAL_MOUSE, row, "sensitive")
                .get_only()
                .build();
        }
        settings
            .bind(VIRTUAL_MOUSE, &*imp.invert_wheel, "sensitive")
            .get_only()
            .build();
    }
}
