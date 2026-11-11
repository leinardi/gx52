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

//! The input tester (docs/ui.md, "Input tester"): a live view of every axis, button, toggle, hat
//! and the mode selector, read from evdev without grabbing the device.
//!
//! The page asks the reader for the axes only while it is mapped, so nothing streams while the
//! window is hidden or another page is shown. What it shows comes from the application
//! ([`Application::held_inputs`], [`Application::axis_values`]) and every indicator is laid out
//! from the controls of [`crate::device::tester`], resolved through the device's input map.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use x52::device::DeviceType;
use x52::input::{Axis, AxisValues, Button, HeldInputs, Input, InputMap, Mode};

use crate::application::Application;
use crate::device::monotonic::{Monotonic, SystemMonotonic};
use crate::device::names;
use crate::device::tester::{self, ButtonGroup, Hat, Pulse, PulseStep};
use crate::profile;
use crate::widgets::{AxisBar, AxisPad, app_of};

/// The size of the stick's pad, from the prototype.
const STICK_PAD: i32 = 120;

/// The size of the ministick's pad.
const MINISTICK_PAD: i32 = 84;

/// The natural width of a line of buttons, from the prototype's Buttons card.
const BUTTONS_LINE: i32 = 420;

/// The text of a value not known yet.
const UNKNOWN: &str = "–";

mod imp {
    use std::cell::{Cell, RefCell};

    use adw::subclass::prelude::*;
    use gtk::glib;
    use x52::device::DeviceType;

    use super::View;
    use crate::application::Application;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/input-tester.ui")]
    pub struct InputTester {
        #[template_child]
        pub(super) stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) unverified: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) cards: TemplateChild<adw::WrapBox>,
        /// The indicators, built for one device type.
        pub(super) view: RefCell<Option<View>>,
        /// The device type the indicators were built for.
        pub(super) built_for: Cell<Option<DeviceType>>,
        /// The application the page asked for the axes, while mapped.
        pub(super) listening: RefCell<Option<glib::WeakRef<Application>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for InputTester {
        const NAME: &'static str = "GX52InputTester";
        type Type = super::InputTester;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for InputTester {
        fn dispose(&self) {
            self.obj().stop_listening();
        }
    }

    impl WidgetImpl for InputTester {
        fn map(&self) {
            self.parent_map();
            let obj = self.obj();
            obj.start_listening();
            obj.refresh();
        }

        fn unmap(&self) {
            self.obj().stop_listening();
            self.parent_unmap();
        }
    }

    impl BinImpl for InputTester {}
}

glib::wrapper! {
    pub struct InputTester(ObjectSubclass<imp::InputTester>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl InputTester {
    /// Asks the application for the axes, once per mapping.
    fn start_listening(&self) {
        let imp = self.imp();
        if imp.listening.borrow().is_some() {
            return;
        }
        if let Some(app) = app_of(self) {
            app.listen_to_axes();
            imp.listening.replace(Some(app.downgrade()));
        }
    }

    /// Tells the application the page no longer shows the axes, and puts out the indicators kept
    /// lit.
    fn stop_listening(&self) {
        let imp = self.imp();
        if let Some(app) = imp.listening.take().and_then(|app| app.upgrade()) {
            app.ignore_axes();
        }
        if let Some(view) = imp.view.borrow().as_ref() {
            view.forget();
        }
    }

    /// Shows the live input again; does nothing while the page is not mapped.
    pub(crate) fn refresh(&self) {
        if !self.is_mapped() {
            return;
        }
        let Some(app) = app_of(self) else {
            return;
        };
        let imp = self.imp();
        let status = app.device_status();
        let Some(device_type) = status.device_type.filter(|_| status.input_open) else {
            imp.stack.set_visible_child_name("none");
            return;
        };
        self.build_for(device_type);
        imp.stack.set_visible_child_name("live");
        if let Some(view) = imp.view.borrow().as_ref() {
            view.show(&app);
        }
    }

    /// Builds the indicators of `device_type`, unless they are already.
    fn build_for(&self, device_type: DeviceType) {
        let imp = self.imp();
        if imp.built_for.replace(Some(device_type)) == Some(device_type) {
            return;
        }
        if let Some(old) = imp.view.take() {
            old.forget();
        }
        imp.cards.remove_all();
        let map = InputMap::for_device(device_type);
        imp.unverified.set_visible(!map.is_verified());
        let view = View::build(map, &imp.cards);
        imp.view.replace(Some(view));
    }
}

/// An indicator of a button that lights while it is held, and for [`tester::MIN_LIT`] at least.
#[derive(Debug)]
struct Pill {
    input: Input,
    widget: gtk::Box,
    pulse: Cell<Pulse>,
    /// The timer that puts it out once [`tester::MIN_LIT`] has passed.
    timer: RefCell<Option<glib::SourceId>>,
}

impl Pill {
    /// The indicator of `button` (evdev `code` in `map`): its label and number, the full name in
    /// the tooltip.
    fn new(map: &InputMap, button: Button, code: u16) -> Rc<Self> {
        let widget = pill_box(&names::tester_label(button));
        if let Some(number) = map.button_number(code) {
            let badge = gtk::Label::builder()
                .label(number.to_string())
                .valign(gtk::Align::Center)
                .css_classes(["input-number"])
                .build();
            widget.prepend(&badge);
            widget.add_css_class("numbered");
        }
        widget.set_tooltip_text(Some(&tester::tooltip(map, button, code)));
        Rc::new(Self {
            input: Input::Button(button),
            widget,
            pulse: Cell::default(),
            timer: RefCell::default(),
        })
    }

    /// Shows whether the input is held at monotonic time `now`.
    fn show(self: &Rc<Self>, held: &HeldInputs, now: std::time::Duration) {
        let mut pulse = self.pulse.get();
        let step = pulse.show(held.is_held(self.input), now);
        self.pulse.set(pulse);
        match step {
            PulseStep::Keep => {}
            PulseStep::Light => {
                self.stop_timer();
                self.widget.add_css_class("on");
            }
            PulseStep::PutOut => self.widget.remove_css_class("on"),
            PulseStep::PutOutIn(left) => {
                self.stop_timer();
                let pill = Rc::downgrade(self);
                let timer = glib::timeout_add_local_once(left, move || {
                    if let Some(pill) = pill.upgrade() {
                        // The source has run: forget it without removing it.
                        pill.timer.take();
                        let mut pulse = pill.pulse.get();
                        if pulse.expire() {
                            pill.widget.remove_css_class("on");
                        }
                        pill.pulse.set(pulse);
                    }
                });
                self.timer.replace(Some(timer));
            }
        }
    }

    fn stop_timer(&self) {
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
    }

    /// Stops the timer and shows the indicator unlit until the next refresh.
    fn cancel(&self) {
        self.stop_timer();
        let mut pulse = self.pulse.get();
        pulse.reset();
        self.pulse.set(pulse);
        self.widget.remove_css_class("on");
    }
}

/// A pill-shaped indicator with `label`.
fn pill_box(label: &str) -> gtk::Box {
    let widget = gtk::Box::builder()
        .spacing(7)
        .valign(gtk::Align::Center)
        .css_classes(["input-pill"])
        .build();
    widget.append(&gtk::Label::new(Some(label)));
    widget
}

/// A row of the axes card: a name, a bar and the raw value.
#[derive(Debug)]
struct AxisRow {
    axis: Axis,
    bar: AxisBar,
    value: gtk::Label,
}

/// The axes card's indicators.
#[derive(Debug)]
struct AxesView {
    stick: AxisPad,
    throttle: AxisBar,
    rows: Vec<AxisRow>,
    raw: gtk::Label,
}

impl AxesView {
    /// The stick as a pad, the throttle as a vertical bar, the other axes of `map` as rows.
    fn build(map: &InputMap, cards: &adw::WrapBox) -> Self {
        let (axes_card, axes) = card(&gettext("Axes"));
        let stick = AxisPad::new(STICK_PAD);
        let throttle = AxisBar::new(gtk::Orientation::Vertical);
        // Wraps the rows under the pad and the bar when the card is narrow.
        let figures = adw::WrapBox::builder()
            .child_spacing(18)
            .line_spacing(12)
            .build();
        figures.append(&captioned(&stick, &gettext("X / Y")));
        figures.append(&captioned(&throttle, &gettext("Z")));
        let grid = gtk::Grid::builder()
            .column_spacing(12)
            .row_spacing(10)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let mut rows = Vec::new();
        for (row, (axis, name)) in [
            (Axis::RotaryI, gettext("X Rotation")),
            (Axis::RotaryE, gettext("Y Rotation")),
            (Axis::Twist, gettext("Z Rotation")),
            (Axis::Slider, gettext("Slider")),
        ]
        .into_iter()
        .filter(|(axis, _)| map.has_axis(*axis))
        .enumerate()
        {
            let row = i32::try_from(row).unwrap_or(i32::MAX);
            let label = gtk::Label::builder()
                .label(name)
                .xalign(0.0)
                .width_chars(10)
                .build();
            let bar = AxisBar::new(gtk::Orientation::Horizontal);
            bar.set_hexpand(true);
            bar.set_valign(gtk::Align::Center);
            let value = value_label();
            grid.attach(&label, 0, row, 1, 1);
            grid.attach(&bar, 1, row, 1, 1);
            grid.attach(&value, 2, row, 1, 1);
            rows.push(AxisRow { axis, bar, value });
        }
        figures.append(&grid);
        axes.append(&figures);
        // As wide as the widest values, so the card keeps its size while they change.
        let raw = gtk::Label::builder()
            .xalign(0.0)
            .width_chars(char_count(&raw_text("0000", "0000", "0000")))
            .css_classes(["caption", "numeric", "dimmed"])
            .build();
        axes.append(&raw);
        axes_card.add_css_class("tester-wide");
        cards.append(&axes_card);
        Self {
            stick,
            throttle,
            rows,
            raw,
        }
    }
}

/// The ministick's card, where the device's map has it: the pad and its caption.
fn build_ministick(map: &InputMap, cards: &adw::WrapBox) -> Option<(AxisPad, gtk::Label)> {
    if !(map.has_axis(Axis::MinistickX) && map.has_axis(Axis::MinistickY)) {
        return None;
    }
    let (card_box, content) = card(&gettext("Ministick"));
    let pad = AxisPad::new(MINISTICK_PAD);
    // As wide as two-digit values, so the card keeps its size while they change.
    let caption = gtk::Label::builder()
        .width_chars(char_count(&ministick_text("00", "00")))
        .css_classes(["caption", "numeric", "dimmed"])
        .build();
    content.append(&pad);
    content.append(&caption);
    cards.append(&card_box);
    Some((pad, caption))
}

/// The toggles card: the toggles in pairs into `pills`, then the mode selector; returns the card
/// and the mode indicators.
fn build_toggles(
    map: &InputMap,
    cards: &adw::WrapBox,
    pills: &mut Vec<Rc<Pill>>,
) -> (gtk::Box, Vec<(Mode, gtk::Box)>) {
    let (toggles_card, toggles) = card(&gettext("Toggles"));
    // The chips keep their size when the card is stretched to fill its line.
    let toggle_grid = gtk::Grid::builder()
        .column_spacing(6)
        .row_spacing(6)
        .halign(gtk::Align::Start)
        .build();
    for (index, (button, code)) in tester::toggles(map).into_iter().enumerate() {
        let index = i32::try_from(index).unwrap_or(i32::MAX);
        let pill = Pill::new(map, button, code);
        pill.widget.set_halign(gtk::Align::Start);
        toggle_grid.attach(&pill.widget, index % 3, index / 3, 1, 1);
        pills.push(pill);
    }
    toggles.append(&toggle_grid);
    let mode_heading = gtk::Label::builder()
        .label(gettext("Mode"))
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    toggles.append(&mode_heading);
    let mode_row = gtk::Box::builder()
        .spacing(6)
        .halign(gtk::Align::Start)
        .build();
    let modes = Mode::ALL
        .into_iter()
        .map(|mode| {
            let pill = pill_box(&mode_number(mode));
            pill.set_tooltip_text(Some(&profile::mode_text(mode)));
            mode_row.append(&pill);
            (mode, pill)
        })
        .collect();
    toggles.append(&mode_row);
    cards.append(&toggles_card);
    (toggles_card, modes)
}

/// The buttons card, grouped by part of the device, its indicators into `pills`.
fn build_buttons(map: &InputMap, cards: &adw::WrapBox, pills: &mut Vec<Rc<Pill>>) -> gtk::Box {
    let (buttons_card, buttons) = card(&gettext("Buttons"));
    for group in ButtonGroup::ALL {
        let present = tester::group_buttons(group, map);
        if present.is_empty() {
            continue;
        }
        let subhead = gtk::Label::builder()
            .label(group.title())
            .xalign(0.0)
            .css_classes(["caption", "dimmed"])
            .build();
        // A shorter natural width than one line of every button, so a narrower card (the
        // toggles) can share its line, as in the prototype.
        let row = adw::WrapBox::builder()
            .child_spacing(6)
            .line_spacing(6)
            .natural_line_length(BUTTONS_LINE)
            .build();
        for (button, code) in present {
            let pill = Pill::new(map, button, code);
            row.append(&pill.widget);
            pills.push(pill);
        }
        buttons.append(&subhead);
        buttons.append(&row);
    }
    buttons_card.add_css_class("tester-wide");
    cards.append(&buttons_card);
    buttons_card
}

/// The POVs card: each hat of `map` as a 3×3 grid of direction cells, in row order.
fn build_hats(map: &InputMap, cards: &adw::WrapBox) -> Vec<(Hat, Vec<gtk::Box>)> {
    let (povs_card, povs) = card(&gettext("POVs"));
    let hat_row = gtk::Box::builder().spacing(18).build();
    let mut hats = Vec::new();
    for hat in Hat::ALL.into_iter().filter(|hat| hat.is_on(map)) {
        let grid = gtk::Grid::builder()
            .column_spacing(3)
            .row_spacing(3)
            .halign(gtk::Align::Center)
            .build();
        let mut cells = Vec::with_capacity(9);
        for row in 0..3 {
            for column in 0..3 {
                let cell = gtk::Box::builder().css_classes(["pov-cell"]).build();
                if (column, row) == (1, 1) {
                    cell.add_css_class("centre");
                }
                grid.attach(&cell, column, row, 1, 1);
                cells.push(cell);
            }
        }
        hat_row.append(&captioned(&grid, &hat.title()));
        hats.push((hat, cells));
    }
    povs.append(&hat_row);
    cards.append(&povs_card);
    hats
}

/// Every indicator of the page, for one device type.
#[derive(Debug)]
struct View {
    map: InputMap,
    axes: AxesView,
    ministick: Option<(AxisPad, gtk::Label)>,
    pills: Vec<Rc<Pill>>,
    modes: Vec<(Mode, gtk::Box)>,
    hats: Vec<(Hat, Vec<gtk::Box>)>,
    buttons_card: gtk::Box,
    toggles_card: gtk::Box,
    /// Monotonic time, for how long an indicator has been lit.
    clock: SystemMonotonic,
    /// The summary last given to the screen reader.
    summary: RefCell<String>,
    /// The held inputs and the mode last shown, `None` until shown: an axis event changes
    /// neither, and the indicators are only gone over when they change.
    held: RefCell<Option<(HeldInputs, Option<Mode>)>>,
    /// The axes last shown.
    axes_shown: RefCell<Option<AxisValues>>,
}

impl View {
    /// Builds the cards of `map` into `cards`.
    fn build(map: InputMap, cards: &adw::WrapBox) -> Self {
        let mut pills = Vec::new();
        let axes = AxesView::build(&map, cards);
        let ministick = build_ministick(&map, cards);
        let (toggles_card, modes) = build_toggles(&map, cards, &mut pills);
        let buttons_card = build_buttons(&map, cards, &mut pills);
        let hats = build_hats(&map, cards);
        Self {
            map,
            axes,
            ministick,
            pills,
            modes,
            hats,
            buttons_card,
            toggles_card,
            clock: SystemMonotonic::new(),
            summary: RefCell::default(),
            held: RefCell::default(),
            axes_shown: RefCell::default(),
        }
    }

    /// Shows the application's live input.
    fn show(&self, app: &Application) {
        let axes = app.axis_values();
        if self.axes_shown.borrow().as_ref() != Some(&axes) {
            self.show_axes(&axes);
            self.axes_shown.replace(Some(axes));
        }
        let shown = (app.held_inputs(), app.selector_mode());
        if self.held.borrow().as_ref() == Some(&shown) {
            return;
        }
        let (held, mode) = &shown;
        let now = self.clock.elapsed();
        for pill in &self.pills {
            pill.show(held, now);
        }
        for (each, pill) in &self.modes {
            if Some(*each) == *mode {
                pill.add_css_class("on");
            } else {
                pill.remove_css_class("on");
            }
        }
        let description = mode.map(profile::mode_text).unwrap_or_default();
        self.toggles_card
            .update_property(&[gtk::accessible::Property::Description(&description)]);
        for (hat, cells) in &self.hats {
            let (column, row) = hat.cell(held);
            let lit = usize::try_from(row * 3 + column).ok();
            for (index, cell) in cells.iter().enumerate() {
                if Some(index) == lit && lit != Some(4) {
                    cell.add_css_class("on");
                } else {
                    cell.remove_css_class("on");
                }
            }
        }
        let summary = tester::summary(&self.map, held, *mode);
        if *self.summary.borrow() != summary {
            self.buttons_card
                .update_property(&[gtk::accessible::Property::Description(&summary)]);
            self.summary.replace(summary);
        }
        self.held.replace(Some(shown));
    }

    /// Puts out every indicator and forgets what was shown, so the next refresh shows it all.
    fn forget(&self) {
        for pill in &self.pills {
            pill.cancel();
        }
        self.held.replace(None);
        self.axes_shown.replace(None);
    }

    /// Shows the axes, without animating: the throttle and the rotaries flicker by ±1 at rest.
    fn show_axes(&self, axes: &AxisValues) {
        let fraction = |axis| {
            axes.get(axis)
                .map(|state| state.range.fraction(state.value))
        };
        // The stick and the ministick centre, so their rest value is the middle of the pad.
        let centred = |axis| {
            axes.get(axis)
                .map(|state| state.range.centred_fraction(state.value))
        };
        let text = |axis| {
            axes.get(axis)
                .map_or_else(|| UNKNOWN.to_owned(), |state| state.value.to_string())
        };
        self.axes
            .stick
            .set_position(centred(Axis::StickX).zip(centred(Axis::StickY)));
        self.axes
            .throttle
            .set_fraction(fraction(Axis::Throttle).map_or(0.0, |fraction| 1.0 - fraction));
        for row in &self.axes.rows {
            row.bar.set_fraction(fraction(row.axis).unwrap_or_default());
            row.value.set_label(&text(row.axis));
        }
        let raw = raw_text(
            &text(Axis::StickX),
            &text(Axis::StickY),
            &text(Axis::Throttle),
        );
        self.axes.raw.set_label(&raw);
        if let Some((pad, caption)) = &self.ministick {
            pad.set_position(centred(Axis::MinistickX).zip(centred(Axis::MinistickY)));
            caption.set_label(&ministick_text(
                &text(Axis::MinistickX),
                &text(Axis::MinistickY),
            ));
        }
    }
}

/// A card with `title`: the card, and the box its content goes in.
fn card(title: &str) -> (gtk::Box, gtk::Box) {
    let heading = gtk::Label::builder()
        .label(title)
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    let content = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .build();
    let card_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .accessible_role(gtk::AccessibleRole::Group)
        .css_classes(["card", "tester-card"])
        .build();
    card_box.append(&heading);
    card_box.append(&content);
    card_box.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
        heading.upcast_ref()
    ])]);
    (card_box, content)
}

/// `widget` over a caption.
fn captioned(widget: &impl IsA<gtk::Widget>, caption: &str) -> gtk::Box {
    let column = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .valign(gtk::Align::Start)
        .build();
    column.append(widget);
    let label = gtk::Label::builder()
        .label(caption)
        .css_classes(["caption", "dimmed"])
        .build();
    column.append(&label);
    column
}

/// A label for a raw axis value.
fn value_label() -> gtk::Label {
    gtk::Label::builder()
        .label(UNKNOWN)
        .width_chars(4)
        .xalign(1.0)
        .css_classes(["caption", "numeric"])
        .build()
}

/// The raw values of the stick and the throttle: "X 511 · Y 512 · Z 82".
fn raw_text(x: &str, y: &str, z: &str) -> String {
    // Translators: the raw values of the stick (X, Y) and the throttle (Z) on the input tester.
    gettext("X {x} · Y {y} · Z {z}")
        .replace("{x}", x)
        .replace("{y}", y)
        .replace("{z}", z)
}

/// The ministick's raw values: "Mouse X / Y · 8, 8".
fn ministick_text(x: &str, y: &str) -> String {
    // Translators: the ministick's raw values on the input tester; it moves the mouse.
    gettext("Mouse X / Y · {x}, {y}")
        .replace("{x}", x)
        .replace("{y}", y)
}

/// The number of characters of `text`, as a label's width in characters.
fn char_count(text: &str) -> i32 {
    i32::try_from(text.chars().count()).unwrap_or(i32::MAX)
}

/// The digit of `mode` on its indicator.
fn mode_number(mode: Mode) -> String {
    match mode {
        Mode::One => "1",
        Mode::Two => "2",
        Mode::Three => "3",
    }
    .to_owned()
}
