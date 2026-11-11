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

//! The udev guidance page (docs/ui.md, "Guidance pages"): shown in place of the editor while the
//! device is present but GX52 may not open it.

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::prelude::*;
use x52::device::DeviceType;

use crate::device::status;
use crate::window::Window;

mod imp {
    use adw::prelude::*;
    use adw::subclass::prelude::*;
    use gettextrs::gettext;
    use gtk::glib;

    use crate::device::udev;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/udev-guide.ui")]
    pub struct UdevGuide {
        #[template_child]
        pub(super) body: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) rule_group: TemplateChild<adw::PreferencesGroup>,
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
    impl ObjectSubclass for UdevGuide {
        const NAME: &'static str = "GX52UdevGuide";
        type Type = super::UdevGuide;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for UdevGuide {
        fn constructed(&self) {
            self.parent_constructed();
            self.rule_group.set_description(Some(
                // Translators: {path} is the rule file's path, /etc/udev/rules.d/70-gx52.rules.
                &gettext("Save this line as {path}").replace("{path}", udev::RULE_PATH),
            ));
            self.rule_row.set_title(udev::USB_RULE);
            self.command_row.set_title(udev::LOAD_COMMAND);
            for (button, text) in [
                (&*self.copy_rule, udev::USB_RULE),
                (&*self.copy_command, udev::LOAD_COMMAND),
            ] {
                button.connect_clicked(move |button| super::copy(button, text));
            }
        }
    }

    impl WidgetImpl for UdevGuide {}
    impl BinImpl for UdevGuide {}
}

glib::wrapper! {
    pub struct UdevGuide(ObjectSubclass<imp::UdevGuide>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl UdevGuide {
    /// Names `device_type` in the page's text.
    pub(crate) fn set_device_type(&self, device_type: DeviceType) {
        self.imp().body.set_label(
            // Translators: {device} is the joystick's product name, "X52 Pro" or "X52".
            &gettext(
                "GX52 found your {device} but cannot open it. Install this udev rule so it can set the LEDs, the MFD text and the clocks.",
            )
            .replace("{device}", status::device_name(device_type)),
        );
    }
}

/// Copies `text` and says so in the window's toast.
fn copy(button: &gtk::Button, text: &str) {
    button.clipboard().set_text(text);
    if let Some(window) = button.root().and_downcast::<Window>() {
        window.show_toast(&gettext("Copied to clipboard"));
    }
}
