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

//! The LEDs view of the profile editor: the mode bar, one card per part of the device with its
//! LED buttons, and the brightness and blink rows (docs/ui.md, "LEDs view").

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;

use crate::profile::ProfileKind;
use crate::profile::editor::EditorState;
use crate::profile::leds::{self, Card, PLACES};
use crate::widgets::{LedButton, brightness_at};

/// The width of a grid column of a card, from the prototype.
const COLUMN_WIDTH: i32 = 64;

mod imp {
    use std::cell::{Cell, RefCell};

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;
    use x52::input::Mode;

    use crate::profile::leds::EditorLed;
    use crate::widgets::{LedButton, ModeBar};

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/led-editor.ui")]
    pub struct LedEditor {
        #[template_child]
        pub(super) mode_bar: TemplateChild<ModeBar>,
        #[template_child]
        pub(super) cards: TemplateChild<adw::WrapBox>,
        #[template_child]
        pub(super) group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) brightness_scale: TemplateChild<gtk::Scale>,
        #[template_child]
        pub(super) brightness_value: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) blink_row: TemplateChild<adw::SwitchRow>,
        /// The button of every LED.
        pub(super) buttons: RefCell<Vec<(EditorLed, LedButton)>>,
        /// The mode last shown, so a change of mode closes an open popover.
        pub(super) mode: Cell<Option<Mode>>,
        /// Set while the editor shows a state itself, so the change is not taken as the user's.
        pub(super) syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LedEditor {
        const NAME: &'static str = "GX52LedEditor";
        type Type = super::LedEditor;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            ModeBar::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LedEditor {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.build_cards();
            obj.setup_rows();
        }
    }

    impl WidgetImpl for LedEditor {}
    impl BinImpl for LedEditor {}
}

glib::wrapper! {
    pub struct LedEditor(ObjectSubclass<imp::LedEditor>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LedEditor {
    /// Builds one card per part of the device from [`PLACES`].
    fn build_cards(&self) {
        let imp = self.imp();
        let mut buttons = Vec::with_capacity(PLACES.len());
        for card in Card::ALL {
            let heading = gtk::Label::builder()
                .label(card.title())
                .xalign(0.0)
                .margin_top(12)
                .margin_start(18)
                .margin_end(18)
                .css_classes(["heading"])
                .build();
            let grid = gtk::Grid::builder()
                .column_spacing(18)
                .row_spacing(14)
                .column_homogeneous(true)
                .halign(gtk::Align::Center)
                .margin_start(18)
                .margin_end(18)
                .margin_bottom(18)
                .build();
            for place in PLACES.iter().filter(|place| place.card == card) {
                let button = LedButton::new(place.led);
                let name = gtk::Label::builder()
                    .label(place.led.name())
                    .css_classes(["caption"])
                    .build();
                // The button's label already names the LED.
                name.update_state(&[gtk::accessible::State::Hidden(true)]);
                let cell = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(6)
                    .width_request(COLUMN_WIDTH)
                    .halign(gtk::Align::Center)
                    .build();
                cell.append(&button);
                cell.append(&name);
                grid.attach(&cell, place.column, place.row, place.width, 1);
                buttons.push((place.led, button));
            }
            let card_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(14)
                .accessible_role(gtk::AccessibleRole::Group)
                .css_classes(["card"])
                .build();
            card_box.append(&heading);
            card_box.append(&grid);
            card_box.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                heading.upcast_ref()
            ])]);
            imp.cards.append(&card_box);
        }
        imp.buttons.replace(buttons);
    }

    /// Sends the brightness and blink rows' changes to the application.
    fn setup_rows(&self) {
        let imp = self.imp();
        imp.brightness_scale.connect_value_changed(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |scale| {
                let imp = editor.imp();
                if imp.syncing.get() {
                    return;
                }
                if let Some(brightness) = brightness_at(scale.value())
                    && let Some(app) = crate::widgets::app_of(&editor)
                {
                    app.set_led_brightness(brightness);
                }
            }
        ));
        imp.blink_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |row| {
                if editor.imp().syncing.get() {
                    return;
                }
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_blink(row.is_active());
                }
            }
        ));
    }

    /// Shows `state`.
    pub(crate) fn show(&self, state: &EditorState) {
        let imp = self.imp();
        if imp.mode.replace(Some(state.mode)) != Some(state.mode) {
            // The popover of another mode's LED would change the new mode.
            for (_, button) in imp.buttons.borrow().iter() {
                button.close();
            }
        }
        imp.mode_bar.set_visible(state.kind == ProfileKind::PerMode);
        imp.mode_bar.show(state.mode);
        let has_leds = state.device_type.has_leds();
        imp.cards.set_visible(has_leds);
        if has_leds {
            for (led, button) in imp.buttons.borrow().iter() {
                button.show(&leds::led_view(state, *led));
            }
        }
        let level = state.settings.led_brightness.level();
        imp.syncing.set(true);
        imp.brightness_scale.set_value(f64::from(level));
        imp.blink_row.set_active(state.settings.blink);
        imp.syncing.set(false);
        imp.brightness_value.set_label(&level.to_string());
        imp.blink_row.set_visible(state.device_type.has_blink());
        imp.group
            .set_description(leds::brightness_note(state).as_deref());
    }
}
