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

//! The figures of the input tester's axes, drawn as the prototype draws them: a square with a dot
//! over a crosshair for a two-axis control (the stick, the ministick) and a bar filled from its
//! start for one axis. The background, the corners and the size come from CSS (`axis-pad`,
//! `axis-bar`); the crosshair is drawn in the widget's colour and the dot and the fill in the
//! accent colour, which they follow when it changes.

use adw::prelude::*;
use gtk::glib;
use gtk::subclass::prelude::*;

/// The diameter of the dot, from the prototype.
const DOT: f32 = 12.0;

/// The opacity of the crosshair against the widget's colour, as `--border-color`.
const CROSSHAIR_ALPHA: f32 = 0.15;

/// Redraws `widget` whenever the accent colour changes, until it is disposed.
fn follow_accent(widget: &gtk::Widget) -> glib::SignalHandlerId {
    adw::StyleManager::default().connect_accent_color_rgba_notify(glib::clone!(
        #[weak]
        widget,
        move |_| widget.queue_draw()
    ))
}

/// Stops following the accent colour.
fn stop_following(handler: Option<glib::SignalHandlerId>) {
    if let Some(handler) = handler {
        adw::StyleManager::default().disconnect(handler);
    }
}

/// The widget's size as floats.
fn size_of(widget: &impl IsA<gtk::Widget>) -> (f32, f32) {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a widget is never wide enough to lose precision as an f32"
    )]
    let size = (widget.width() as f32, widget.height() as f32);
    size
}

/// `fraction` (0 to 1) of `length`.
fn part_of(fraction: f64, length: f32) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a fraction from 0 to 1 of a widget's size fits an f32"
    )]
    let part = fraction.clamp(0.0, 1.0) as f32 * length;
    part
}

mod pad {
    use std::cell::{Cell, RefCell};

    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use gtk::{glib, graphene, gsk};

    #[derive(Debug, Default)]
    pub struct AxisPad {
        /// Where the dot is, each from `0.0` (left, top) to `1.0` (right, bottom); `None` hides
        /// it.
        pub(super) position: Cell<Option<(f64, f64)>>,
        pub(super) accent: RefCell<Option<glib::SignalHandlerId>>,
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
            obj.set_halign(gtk::Align::Center);
            obj.set_valign(gtk::Align::Start);
            self.accent
                .replace(Some(super::follow_accent(obj.upcast_ref())));
        }

        fn dispose(&self) {
            super::stop_following(self.accent.take());
        }
    }

    impl WidgetImpl for AxisPad {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (width, height) = super::size_of(&*obj);
            let mut line = obj.color();
            line.set_alpha(line.alpha() * super::CROSSHAIR_ALPHA);
            snapshot.append_color(
                &line,
                &graphene::Rect::new((width / 2.0).floor(), 0.0, 1.0, height),
            );
            snapshot.append_color(
                &line,
                &graphene::Rect::new(0.0, (height / 2.0).floor(), width, 1.0),
            );
            let Some((x, y)) = self.position.get() else {
                return;
            };
            let (x, y) = (super::part_of(x, width), super::part_of(y, height));
            let radius = super::DOT / 2.0;
            let dot = graphene::Rect::new(x - radius, y - radius, super::DOT, super::DOT);
            snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(dot, radius));
            snapshot.append_color(&adw::StyleManager::default().accent_color_rgba(), &dot);
            snapshot.pop();
        }
    }
}

mod bar {
    use std::cell::{Cell, RefCell};

    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use gtk::{glib, graphene};

    #[derive(Debug, Default)]
    pub struct AxisBar {
        /// How much of the bar is filled, from `0.0` to `1.0`.
        pub(super) fraction: Cell<f64>,
        /// Filled upwards when vertical, from the left otherwise.
        pub(super) vertical: Cell<bool>,
        pub(super) accent: RefCell<Option<glib::SignalHandlerId>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AxisBar {
        const NAME: &'static str = "GX52AxisBar";
        type Type = super::AxisBar;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("axis-bar");
        }
    }

    impl ObjectImpl for AxisBar {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            self.accent
                .replace(Some(super::follow_accent(obj.upcast_ref())));
        }

        fn dispose(&self) {
            super::stop_following(self.accent.take());
        }
    }

    impl WidgetImpl for AxisBar {
        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (width, height) = super::size_of(&*obj);
            let fraction = self.fraction.get();
            // Filled from the left, or upwards from the bottom; the CSS corners clip it.
            let fill = if self.vertical.get() {
                let filled = super::part_of(fraction, height);
                graphene::Rect::new(0.0, height - filled, width, filled)
            } else {
                graphene::Rect::new(0.0, 0.0, super::part_of(fraction, width), height)
            };
            snapshot.append_color(&adw::StyleManager::default().accent_color_rgba(), &fill);
        }
    }
}

glib::wrapper! {
    /// A two-axis control as a dot over a crosshair.
    pub struct AxisPad(ObjectSubclass<pad::AxisPad>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AxisPad {
    /// A pad of `size` × `size` pixels.
    pub(crate) fn new(size: i32) -> Self {
        let pad: Self = glib::Object::builder()
            .property("width-request", size)
            .property("height-request", size)
            .property("overflow", gtk::Overflow::Hidden)
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

glib::wrapper! {
    /// One axis as a bar filled from its start.
    pub struct AxisBar(ObjectSubclass<bar::AxisBar>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl AxisBar {
    /// A bar along `orientation`, with the matching `horizontal` or `vertical` style class; CSS
    /// gives its thickness, the layout its length.
    pub(crate) fn new(orientation: gtk::Orientation) -> Self {
        let bar: Self = glib::Object::builder()
            .property("overflow", gtk::Overflow::Hidden)
            .build();
        let vertical = orientation == gtk::Orientation::Vertical;
        bar.imp().vertical.set(vertical);
        bar.add_css_class(if vertical { "vertical" } else { "horizontal" });
        bar.update_state(&[gtk::accessible::State::Hidden(true)]);
        bar
    }

    /// Fills `fraction` (`0.0` to `1.0`) of the bar.
    pub(crate) fn set_fraction(&self, fraction: f64) {
        if self.imp().fraction.replace(fraction).to_bits() != fraction.to_bits() {
            self.queue_draw();
        }
    }
}
