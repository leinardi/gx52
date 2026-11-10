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

//! The sidebar footer: the device status as a dot and two lines (docs/ui.md, "Device status").

use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;

use crate::device::status::{Dot, Footer};

mod imp {
    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/device-footer.ui")]
    pub struct DeviceFooter {
        #[template_child]
        pub(super) dot: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) name_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) warning_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) line_label: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for DeviceFooter {
        const NAME: &'static str = "GX52DeviceFooter";
        type Type = super::DeviceFooter;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            // One element for a screen reader, which reads the whole line.
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for DeviceFooter {
        fn constructed(&self) {
            self.parent_constructed();
            for child in [
                self.name_label.upcast_ref::<gtk::Widget>(),
                self.line_label.upcast_ref(),
                self.warning_icon.upcast_ref(),
                self.dot.upcast_ref(),
            ] {
                child.update_state(&[gtk::accessible::State::Hidden(true)]);
            }
        }
    }

    impl WidgetImpl for DeviceFooter {}
    impl BoxImpl for DeviceFooter {}
}

glib::wrapper! {
    pub struct DeviceFooter(ObjectSubclass<imp::DeviceFooter>)
        @extends gtk::Box, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl DeviceFooter {
    /// Shows `footer`.
    pub(crate) fn show(&self, footer: &Footer) {
        let imp = self.imp();
        imp.name_label.set_label(&footer.name);
        imp.line_label.set_label(&footer.line);
        imp.warning_icon.set_visible(footer.warning);
        for (class, dot) in [
            ("connected", Dot::Connected),
            ("problem", Dot::Problem),
            ("waiting", Dot::Waiting),
        ] {
            if footer.dot == dot {
                imp.dot.add_css_class(class);
            } else {
                imp.dot.remove_css_class(class);
            }
        }
        let text = format!("{}, {}", footer.name, footer.line);
        self.update_property(&[gtk::accessible::Property::Label(&text)]);
    }
}
