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

//! The simulated MFD of the MFD view: three lines of 16 cells, the SHIFT label, the clock and the
//! date, lit by the MFD brightness (docs/ui.md, "MFD view").

use adw::subclass::prelude::*;
use gtk::glib;
use gtk::prelude::*;
use x52::charset::LINE_BYTES;

use crate::profile::MFD_LINES;
use crate::profile::mfd::{self, Backlight, MfdPreview};

mod imp {
    use std::cell::RefCell;

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/mfd-display.ui")]
    pub struct MfdDisplay {
        #[template_child]
        pub(super) screen: TemplateChild<gtk::Overlay>,
        #[template_child]
        pub(super) lines: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) clock_tag: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) clock: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) date: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) shift: TemplateChild<gtk::Label>,
        /// The cells of each line, left to right.
        pub(super) cells: RefCell<Vec<Vec<gtk::Label>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MfdDisplay {
        const NAME: &'static str = "GX52MfdDisplay";
        type Type = super::MfdDisplay;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            // One image for assistive technologies, read by its label.
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MfdDisplay {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build_cells();
            // The label reads the whole display: its parts are not read one by one.
            self.screen
                .update_state(&[gtk::accessible::State::Hidden(true)]);
        }
    }

    impl WidgetImpl for MfdDisplay {}
    impl BinImpl for MfdDisplay {}
}

glib::wrapper! {
    pub struct MfdDisplay(ObjectSubclass<imp::MfdDisplay>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl MfdDisplay {
    /// Builds the 16 cells of each line.
    fn build_cells(&self) {
        let imp = self.imp();
        let cells = (0..MFD_LINES)
            .map(|_| {
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 1);
                let cells: Vec<gtk::Label> = (0..LINE_BYTES)
                    .map(|_| {
                        let cell = gtk::Label::builder()
                            .label(" ")
                            .width_chars(1)
                            .css_classes(["mfd-cell"])
                            .build();
                        line.append(&cell);
                        cell
                    })
                    .collect();
                imp.lines.append(&line);
                cells
            })
            .collect();
        imp.cells.replace(cells);
    }

    /// Shows `preview`.
    pub(crate) fn show(&self, preview: &MfdPreview) {
        let imp = self.imp();
        for (cells, line) in imp.cells.borrow().iter().zip(&preview.lines) {
            for (cell, character) in cells.iter().zip(line.chars()) {
                let text = character.to_string();
                if cell.label() != text {
                    cell.set_label(&text);
                }
            }
        }
        match preview.clock_tag {
            Some(number) => {
                imp.clock_tag.set_label(&number.to_string());
                imp.clock_tag.set_visible(true);
            }
            None => imp.clock_tag.set_visible(false),
        }
        imp.clock.set_label(&preview.clock);
        imp.date.set_label(&preview.date);
        imp.shift.set_visible(preview.shift);
        for backlight in Backlight::ALL {
            if backlight == preview.backlight {
                imp.screen.add_css_class(backlight.css_class());
            } else {
                imp.screen.remove_css_class(backlight.css_class());
            }
        }
        self.update_property(&[gtk::accessible::Property::Label(&mfd::accessible_label(
            preview,
        ))]);
    }
}
