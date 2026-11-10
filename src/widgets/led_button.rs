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

//! The round button of one LED, coloured by its state; it opens a popover with the states the LED
//! can take (docs/ui.md, "LEDs view").

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use gtk::prelude::*;

use crate::profile::leds::{EditorLed, LedChoice, LedView};

/// The style class of a blinking LED.
const BLINKING: &str = "blinking";

mod imp {
    use std::cell::Cell;

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;

    use crate::profile::leds::{EditorLed, LedChoice};

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/led-button.ui")]
    pub struct LedButton {
        #[template_child]
        pub(super) menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub(super) popover: TemplateChild<gtk::Popover>,
        #[template_child]
        pub(super) title: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) toggles: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub(super) fixed_button: TemplateChild<gtk::Button>,
        /// The LED, from construction.
        pub(super) led: Cell<Option<EditorLed>>,
        /// Set while the button shows a state itself, so the change is not taken as the user's.
        pub(super) syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LedButton {
        const NAME: &'static str = "GX52LedButton";
        type Type = super::LedButton;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LedButton {
        fn constructed(&self) {
            self.parent_constructed();
            self.toggles.connect_active_name_notify(glib::clone!(
                #[weak(rename_to = button)]
                self.obj(),
                move |toggles| {
                    let imp = button.imp();
                    if imp.syncing.get() {
                        return;
                    }
                    let choice = toggles
                        .active_name()
                        .as_deref()
                        .and_then(LedChoice::from_id);
                    imp.popover.popdown();
                    if let (Some(led), Some(choice)) = (imp.led.get(), choice)
                        && let Some(app) = crate::widgets::app_of(&button)
                    {
                        app.set_led(led, choice);
                    }
                }
            ));
        }
    }

    impl WidgetImpl for LedButton {}
    impl BinImpl for LedButton {}
}

glib::wrapper! {
    pub struct LedButton(ObjectSubclass<imp::LedButton>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl LedButton {
    /// The button of `led`.
    pub(crate) fn new(led: EditorLed) -> Self {
        let button: Self = glib::Object::new();
        let imp = button.imp();
        imp.led.set(Some(led));
        if led.choices().is_empty() {
            imp.menu_button.set_visible(false);
            imp.fixed_button.set_visible(true);
            imp.fixed_button
                .update_state(&[gtk::accessible::State::Disabled(true)]);
        } else {
            let name = led.name();
            imp.title
                .set_label(&gettext("{led} LED").replace("{led}", &name));
            imp.toggles
                .update_property(&[gtk::accessible::Property::Label(
                    &gettext("{led} LED state").replace("{led}", &name),
                )]);
            for choice in led.choices() {
                imp.toggles.add(
                    adw::Toggle::builder()
                        .name(choice.id())
                        .label(choice.label())
                        .build(),
                );
            }
        }
        button
    }

    /// Shows `view`: the colour, the blinking, the tooltip, the accessible label and, in the
    /// popover, the state.
    pub(crate) fn show(&self, view: &LedView) {
        let imp = self.imp();
        imp.syncing.set(true);
        imp.toggles.set_active_name(Some(view.choice.id()));
        imp.syncing.set(false);
        let shown: &gtk::Widget = if imp.menu_button.is_visible() {
            imp.menu_button.upcast_ref()
        } else {
            imp.fixed_button.upcast_ref()
        };
        for choice in LedChoice::ALL {
            if choice.css_class() != view.class {
                shown.remove_css_class(choice.css_class());
            }
        }
        shown.add_css_class(view.class);
        if view.blinking {
            shown.add_css_class(BLINKING);
        } else {
            shown.remove_css_class(BLINKING);
        }
        if shown.tooltip_text().as_deref() != Some(view.tooltip.as_str()) {
            shown.set_tooltip_text(Some(&view.tooltip));
        }
        shown.update_property(&[gtk::accessible::Property::Label(&view.label)]);
    }

    /// Closes the popover, if open.
    pub(crate) fn close(&self) {
        self.imp().popover.popdown();
    }
}
