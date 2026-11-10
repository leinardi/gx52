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

//! A row of the profile list: the name, the active marker, the "1 2 3" badges of a per-mode
//! profile and the warning of a profile that failed to load (docs/ui.md, "Sidebar").

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::prelude::*;

use crate::model::{ProfileObject, ProfileState};

mod imp {
    use std::cell::{OnceCell, RefCell};

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;

    use crate::model::ProfileObject;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/profile-row.ui")]
    pub struct ProfileRow {
        #[template_child]
        pub(super) active_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) warning_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) badges: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) badge_one: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) badge_two: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) badge_three: TemplateChild<gtk::Label>,
        /// The profile shown, set once.
        pub(super) item: OnceCell<ProfileObject>,
        /// The handlers on `item`, which outlives the row.
        pub(super) handlers: RefCell<Vec<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileRow {
        const NAME: &'static str = "GX52ProfileRow";
        type Type = super::ProfileRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ProfileRow {
        fn constructed(&self) {
            self.parent_constructed();
            // The row's label says it all; the icons and badges would repeat it.
            for child in [
                self.active_icon.upcast_ref::<gtk::Widget>(),
                self.warning_icon.upcast_ref(),
                self.title_label.upcast_ref(),
                self.badges.upcast_ref(),
            ] {
                child.update_state(&[gtk::accessible::State::Hidden(true)]);
            }
        }

        fn dispose(&self) {
            if let Some(item) = self.item.get() {
                for handler in self.handlers.take() {
                    item.disconnect(handler);
                }
            }
        }
    }

    impl WidgetImpl for ProfileRow {}
    impl ListBoxRowImpl for ProfileRow {}
}

glib::wrapper! {
    pub struct ProfileRow(ObjectSubclass<imp::ProfileRow>)
        @extends gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl ProfileRow {
    /// A row showing `item`, following its changes.
    pub(crate) fn new(item: &ProfileObject) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();
        // Set once, right after construction.
        let _ = imp.item.set(item.clone());
        item.bind_property("title", &*imp.title_label, "label")
            .sync_create()
            .build();
        item.bind_property("per-mode", &*imp.badges, "visible")
            .sync_create()
            .build();
        let handlers = ["title", "state", "per-mode", "active", "mode-shown"]
            .into_iter()
            .map(|property| {
                item.connect_notify_local(
                    Some(property),
                    glib::clone!(
                        #[weak]
                        row,
                        move |_, _| row.refresh()
                    ),
                )
            })
            .collect();
        imp.handlers.replace(handlers);
        row.refresh();
        row
    }

    fn refresh(&self) {
        let imp = self.imp();
        let Some(item) = imp.item.get() else {
            return;
        };
        imp.active_icon
            .set_opacity(if item.active() { 1.0 } else { 0.0 });
        let usable = item.is_usable();
        imp.warning_icon.set_visible(!usable);
        if usable {
            imp.title_label.remove_css_class("dim-label");
        } else {
            imp.title_label.add_css_class("dim-label");
        }
        for (badge, mode) in [
            (&*imp.badge_one, 1),
            (&*imp.badge_two, 2),
            (&*imp.badge_three, 3),
        ] {
            if item.mode_shown() == mode {
                badge.add_css_class("current");
            } else {
                badge.remove_css_class("current");
            }
        }
        self.update_property(&[gtk::accessible::Property::Label(&accessible_label(item))]);
    }
}

/// What a screen reader says for the row of `item`.
fn accessible_label(item: &ProfileObject) -> String {
    let mut parts = vec![item.title()];
    if item.active() {
        parts.push(gettext("active"));
    }
    if item.per_mode() {
        parts.push(match item.mode_shown() {
            0 => gettext("per-mode"),
            // Translators: {mode} is the mode the device shows, 1, 2 or 3.
            mode => gettext("per-mode, mode {mode}").replace("{mode}", &mode.to_string()),
        });
    }
    match item.state() {
        ProfileState::Usable => {}
        ProfileState::Newer => parts.push(gettext("made by a newer GX52")),
        ProfileState::Damaged => parts.push(gettext("damaged")),
    }
    parts.join(", ")
}
