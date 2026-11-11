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

//! A square that shows a two-axis control (the stick, the ministick) as a dot over a crosshair, for
//! the input tester. The background and size come from CSS (`.axis-pad`); the crosshair and the
//! dot are drawn in the widget's colour, which the CSS sets to the accent colour.

use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// The diameter of the dot, from the prototype.
const DOT: f32 = 12.0;

/// The opacity of the crosshair, against the dot's full colour.
const CROSSHAIR_ALPHA: f32 = 0.25;

mod imp {
    use std::cell::Cell;

    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use gtk::{glib, graphene, gsk};

    #[derive(Debug, Default)]
    pub struct AxisPad {
        /// Where the dot is, each from `0.0` (left, top) to `1.0` (right, bottom); `None` hides
        /// it.
        pub(super) position: Cell<Option<(f64, f64)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AxisPad {
        const NAME: &'static str = "GX52AxisPad";
        type Type = super::AxisPad;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("axis-pad");
        }
    }

    impl ObjectImpl for AxisPad {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_focusable(false);
            obj.set_halign(gtk::Align::Center);
            obj.set_valign(gtk::Align::Center);
        }
    }

    impl WidgetImpl for AxisPad {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            #[expect(
                clippy::cast_precision_loss,
                reason = "a widget is never wide enough to lose precision as an f32"
            )]
            let (width, height) = (obj.width() as f32, obj.height() as f32);
            let color = obj.color();
            let mut faint = color;
            faint.set_alpha(color.alpha() * super::CROSSHAIR_ALPHA);
            snapshot.append_color(
                &faint,
                &graphene::Rect::new(width / 2.0 - 0.5, 0.0, 1.0, height),
            );
            snapshot.append_color(
                &faint,
                &graphene::Rect::new(0.0, height / 2.0 - 0.5, width, 1.0),
            );
            let Some((x, y)) = self.position.get() else {
                return;
            };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a fraction from 0 to 1 of a widget's size fits an f32"
            )]
            let (x, y) = (x as f32 * width, y as f32 * height);
            let radius = super::DOT / 2.0;
            let dot = graphene::Rect::new(x - radius, y - radius, super::DOT, super::DOT);
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(dot, radius));
            snapshot.append_color(&color, &dot);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct AxisPad(ObjectSubclass<imp::AxisPad>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AxisPad {
    /// A pad of `size` × `size` pixels.
    pub(crate) fn new(size: i32) -> Self {
        let pad: Self = glib::Object::builder()
            .property("width-request", size)
            .property("height-request", size)
            .build();
        // The group's label and description say what the pad shows.
        pad.update_state(&[gtk::accessible::State::Hidden(true)]);
        pad
    }

    /// Puts the dot at `x`, `y`, each from `0.0` to `1.0`, or hides it.
    pub(crate) fn set_position(&self, position: Option<(f64, f64)>) {
        if self.imp().position.replace(position) != position {
            self.queue_draw();
        }
    }
}
