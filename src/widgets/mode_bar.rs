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

//! The mode bar of a per-mode profile: the mode shown, which follows the selector on the stick
//! until another one is picked (docs/ui.md, "Mode bar").

use adw::subclass::prelude::*;
use gtk::glib;
use x52::input::Mode;

/// The name of `mode`'s toggle.
const fn toggle_name(mode: Mode) -> &'static str {
    match mode {
        Mode::One => "1",
        Mode::Two => "2",
        Mode::Three => "3",
    }
}

mod imp {
    use std::cell::Cell;

    use adw::subclass::prelude::*;
    use gtk::glib;
    use x52::input::Mode;

    use super::toggle_name;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/mode-bar.ui")]
    pub struct ModeBar {
        #[template_child]
        pub(super) toggles: TemplateChild<adw::ToggleGroup>,
        /// Set while the bar shows a mode itself, so the change is not taken as the user's.
        pub(super) syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ModeBar {
        const NAME: &'static str = "GX52ModeBar";
        type Type = super::ModeBar;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ModeBar {
        fn constructed(&self) {
            self.parent_constructed();
            self.toggles.connect_active_name_notify(glib::clone!(
                #[weak(rename_to = bar)]
                self.obj(),
                move |toggles| {
                    if bar.imp().syncing.get() {
                        return;
                    }
                    let name = toggles.active_name();
                    let picked = Mode::ALL
                        .into_iter()
                        .find(|mode| name.as_deref() == Some(toggle_name(*mode)));
                    if let Some(mode) = picked
                        && let Some(app) = crate::widgets::app_of(&bar)
                    {
                        app.pick_editing_mode(mode);
                    }
                }
            ));
        }
    }

    impl WidgetImpl for ModeBar {}
    impl BoxImpl for ModeBar {}
}

glib::wrapper! {
    pub struct ModeBar(ObjectSubclass<imp::ModeBar>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl ModeBar {
    /// Shows `mode` as the mode shown.
    pub(crate) fn show(&self, mode: Mode) {
        let imp = self.imp();
        imp.syncing.set(true);
        imp.toggles.set_active_name(Some(toggle_name(mode)));
        imp.syncing.set(false);
    }
}
