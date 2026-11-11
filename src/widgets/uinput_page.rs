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

//! The uinput guidance (docs/ui.md, "Guidance pages"): a subpage of the preferences dialog with
//! the rule line the virtual mouse needs and its security note.

use adw::prelude::*;
use gettextrs::gettext;
use gtk::glib;

use crate::window;

mod imp {
    use adw::prelude::*;
    use adw::subclass::prelude::*;
    use gettextrs::gettext;
    use gtk::glib;

    use crate::device::udev;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/uinput-page.ui")]
    pub struct UinputPage {
        #[template_child]
        pub(super) page: TemplateChild<adw::PreferencesPage>,
        #[template_child]
        pub(super) rule_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) copy_rule: TemplateChild<gtk::Button>,
        #[template_child]
        pub(super) command_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) copy_command: TemplateChild<gtk::Button>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for UinputPage {
        const NAME: &'static str = "GX52UinputPage";
        type Type = super::UinputPage;
        type ParentType = adw::NavigationPage;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for UinputPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.page.set_description(
                // Translators: {path} is the rule file's path, /etc/udev/rules.d/70-gx52.rules.
                &gettext(
                    "The virtual mouse needs write access to /dev/uinput. Add this line to the same rule file as the device rule, {path}:",
                )
                .replace("{path}", udev::RULE_PATH),
            );
            self.rule_row.set_title(udev::UINPUT_RULE);
            self.command_row.set_title(udev::LOAD_COMMAND);
            for (button, text) in [
                (&*self.copy_rule, udev::UINPUT_RULE),
                (&*self.copy_command, udev::LOAD_COMMAND),
            ] {
                button.connect_clicked(move |button| super::copy(button, text));
            }
        }
    }

    impl WidgetImpl for UinputPage {}
    impl NavigationPageImpl for UinputPage {}
}

glib::wrapper! {
    pub struct UinputPage(ObjectSubclass<imp::UinputPage>)
        @extends adw::NavigationPage, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl UinputPage {
    /// A new page, to push onto the preferences dialog.
    pub(crate) fn new() -> Self {
        glib::Object::new()
    }
}

/// Copies `text` and says so in the preferences dialog's toast.
fn copy(button: &gtk::Button, text: &str) {
    button.clipboard().set_text(text);
    if let Some(dialog) = button
        .ancestor(adw::PreferencesDialog::static_type())
        .and_downcast::<adw::PreferencesDialog>()
    {
        dialog.add_toast(window::plain_toast(&gettext("Copied to clipboard")));
    }
}
