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

//! The MFD view of the profile editor: the mode bar, the simulated display with its clock
//! switcher, the line rows, and the brightness and shift rows (docs/ui.md, "MFD view").

use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use jiff::tz::TimeZone;
use x52::clock::{self, Clock, Clock1Source, SystemClock};

use crate::profile::editor::EditorState;
use crate::profile::mfd::{self, LineSource, ShownClock, TextInfo};
use crate::profile::{MfdLineSource, ProfileKind};
use crate::widgets::brightness_at;

/// How long after a minute boundary the display ticks, so the system clock is surely past it.
const TICK_SLACK: Duration = Duration::from_millis(50);

/// The widgets of one MFD line.
#[derive(Debug)]
struct LineRow {
    /// The source of the line.
    source: adw::ComboRow,
    /// The text of a custom text line.
    text: adw::EntryRow,
    /// "n/16".
    counter: gtk::Label,
    /// The characters the MFD shows as a replacement.
    substitutions: gtk::Label,
}

/// The byte offset of character `position` in `text`, or the end.
fn byte_offset(text: &str, position: i32) -> usize {
    usize::try_from(position)
        .ok()
        .and_then(|position| text.char_indices().nth(position))
        .map_or(text.len(), |(index, _)| index)
}

mod imp {
    use std::cell::{Cell, RefCell};

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;
    use jiff::tz::TimeZone;

    use super::LineRow;
    use crate::profile::mfd::ShownClock;
    use crate::widgets::{MfdDisplay, ModeBar};

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/mfd-editor.ui")]
    pub struct MfdEditor {
        #[template_child]
        pub(super) mode_bar: TemplateChild<ModeBar>,
        #[template_child]
        pub(super) display: TemplateChild<MfdDisplay>,
        #[template_child]
        pub(super) clock_toggles: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub(super) lines_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) brightness_scale: TemplateChild<gtk::Scale>,
        #[template_child]
        pub(super) brightness_value: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) shift_row: TemplateChild<adw::SwitchRow>,
        /// The rows of each line.
        pub(super) rows: RefCell<Vec<LineRow>>,
        /// The clock the display shows: the user's choice, never the device's.
        pub(super) clock: Cell<ShownClock>,
        /// The system time zone, read at each tick.
        pub(super) zone: RefCell<Option<TimeZone>>,
        /// The timer of the next tick, while the editor is mapped.
        pub(super) tick: RefCell<Option<glib::SourceId>>,
        /// Set while the editor shows a state itself, so the change is not taken as the user's.
        pub(super) syncing: Cell<bool>,
        /// Set while the editor inserts the part of a typed text that fits.
        pub(super) inserting: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MfdEditor {
        const NAME: &'static str = "GX52MfdEditor";
        type Type = super::MfdEditor;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            ModeBar::ensure_type();
            MfdDisplay::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for MfdEditor {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.build_line_rows();
            obj.setup_rows();
            // The zone is read once now, so the display is complete the first time it is shown.
            obj.tick();
        }
    }

    impl WidgetImpl for MfdEditor {
        fn map(&self) {
            self.parent_map();
            self.obj().tick();
        }

        fn unmap(&self) {
            self.obj().stop_ticking();
            self.parent_unmap();
        }
    }

    impl BinImpl for MfdEditor {}
}

glib::wrapper! {
    pub struct MfdEditor(ObjectSubclass<imp::MfdEditor>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl MfdEditor {
    /// Builds the source and text rows of each line.
    fn build_line_rows(&self) {
        let imp = self.imp();
        let labels: Vec<String> = LineSource::ALL.into_iter().map(LineSource::label).collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let rows = (0..crate::profile::MFD_LINES)
            .map(|line| {
                let number = (line + 1).to_string();
                let source = adw::ComboRow::builder()
                    // Translators: {number} is the number of an MFD line, 1 to 3.
                    .title(gettext("Line {number}").replace("{number}", &number))
                    .model(&gtk::StringList::new(&labels))
                    .build();
                let text = adw::EntryRow::builder()
                    // Translators: {number} is the number of an MFD line, 1 to 3.
                    .title(gettext("Line {number} text").replace("{number}", &number))
                    .build();
                // Up to 16 replaced characters: cut, so the row fits the narrowest window. The
                // label's text, which screen readers read, keeps the whole list.
                let substitutions = gtk::Label::builder()
                    .valign(gtk::Align::Center)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(12)
                    .tooltip_text(gettext(
                        "These characters are not on the MFD and are shown as their replacement",
                    ))
                    .css_classes(["caption", "warning"])
                    .build();
                let counter = gtk::Label::builder()
                    .valign(gtk::Align::Center)
                    .css_classes(["caption", "numeric", "dim-label"])
                    .build();
                text.add_suffix(&substitutions);
                text.add_suffix(&counter);
                self.connect_line(line, &source, &text);
                imp.lines_group.add(&source);
                imp.lines_group.add(&text);
                LineRow {
                    source,
                    text,
                    counter,
                    substitutions,
                }
            })
            .collect();
        imp.rows.replace(rows);
    }

    /// Sends the changes of line `line`'s rows to the application, and keeps the typed text
    /// within the 16 bytes of the line.
    fn connect_line(&self, line: usize, source: &adw::ComboRow, text: &adw::EntryRow) {
        source.connect_selected_notify(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |row| {
                if editor.imp().syncing.get() {
                    return;
                }
                let picked = usize::try_from(row.selected())
                    .ok()
                    .and_then(|index| LineSource::ALL.get(index));
                if let Some(picked) = picked
                    && let Some(app) = crate::widgets::app_of(&editor)
                {
                    app.set_mfd_source(line, *picked);
                }
            }
        ));
        text.connect_changed(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |row| {
                if editor.imp().syncing.get() {
                    return;
                }
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_mfd_text(line, row.text().into());
                }
            }
        ));
        // Typing goes to the entry's text widget, which emits the insertion: only the part that
        // fits is let through, so the text already there and the cursor stay as they are.
        if let Some(delegate) = text.delegate() {
            delegate.connect_insert_text(glib::clone!(
                #[weak(rename_to = editor)]
                self,
                move |delegate, inserted, position| {
                    let imp = editor.imp();
                    if imp.syncing.get() || imp.inserting.get() {
                        return;
                    }
                    let current = delegate.text();
                    let (before, after) = current.split_at(byte_offset(&current, *position));
                    let accepted = mfd::accepted_insert(before, inserted, after);
                    if accepted.len() == inserted.len() {
                        return;
                    }
                    if !accepted.is_empty() {
                        imp.inserting.set(true);
                        delegate.insert_text(accepted, position);
                        imp.inserting.set(false);
                    }
                    delegate.stop_signal_emission_by_name("insert-text");
                }
            ));
        }
    }

    /// Sends the clock switcher, brightness and shift rows' changes on.
    fn setup_rows(&self) {
        let imp = self.imp();
        imp.clock_toggles.connect_active_name_notify(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |toggles| {
                let name = toggles.active_name();
                let picked = ShownClock::ALL
                    .into_iter()
                    .find(|clock| name.as_deref() == Some(clock.number().to_string().as_str()));
                if let Some(clock) = picked {
                    editor.imp().clock.set(clock);
                    editor.refresh_display();
                }
            }
        ));
        imp.brightness_scale.connect_value_changed(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |scale| {
                if editor.imp().syncing.get() {
                    return;
                }
                if let Some(brightness) = brightness_at(scale.value())
                    && let Some(app) = crate::widgets::app_of(&editor)
                {
                    app.set_mfd_brightness(brightness);
                }
            }
        ));
        imp.shift_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |row| {
                if editor.imp().syncing.get() {
                    return;
                }
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_shift(row.is_active());
                }
            }
        ));
    }

    /// Shows `state`.
    pub(crate) fn show(&self, state: &EditorState) {
        let imp = self.imp();
        imp.mode_bar.set_visible(state.kind == ProfileKind::PerMode);
        imp.mode_bar.show(state.mode);
        let settings = &state.settings;
        imp.syncing.set(true);
        for (row, line) in imp.rows.borrow().iter().zip(&settings.mfd_lines) {
            let source = LineSource::of(line);
            let position = LineSource::ALL
                .iter()
                .position(|candidate| *candidate == source)
                .and_then(|index| u32::try_from(index).ok())
                .unwrap_or(gtk::INVALID_LIST_POSITION);
            row.source.set_selected(position);
            let text = match line {
                MfdLineSource::Text(text) => text.as_str(),
                MfdLineSource::Empty
                | MfdLineSource::ProfileName
                | MfdLineSource::Mode
                | MfdLineSource::LastButton => "",
            };
            // Only a different text is set: setting it again would move the cursor.
            if row.text.text() != text {
                row.text.set_text(text);
            }
            row.text.set_visible(source == LineSource::CustomText);
            let info = TextInfo::of(text);
            row.counter.set_label(&info.counter());
            row.counter
                .update_property(&[gtk::accessible::Property::Label(&info.counter_label())]);
            row.substitutions.set_label(&info.substitutions);
            row.substitutions
                .set_visible(!info.substitutions.is_empty());
        }
        let level = settings.mfd_brightness.level();
        imp.brightness_scale.set_value(f64::from(level));
        imp.shift_row.set_active(settings.shift);
        imp.syncing.set(false);
        imp.brightness_value.set_label(&level.to_string());
        imp.shift_row.set_visible(state.device_type.has_shift());
        self.refresh_display();
    }

    /// Shows the display again: the dynamic lines and the time follow the device and the clock.
    pub(crate) fn refresh_display(&self) {
        let imp = self.imp();
        let Some(zone) = imp.zone.borrow().clone() else {
            // The zone is read when the editor is mapped; the display waits for it.
            return;
        };
        let Some(app) = crate::widgets::app_of(self) else {
            return;
        };
        if let Some(preview) = app.mfd_preview(imp.clock.get(), SystemClock.now(), &zone) {
            imp.display.show(&preview);
        }
    }

    /// Reads the system time zone off the main thread, shows the display and, while the editor is
    /// mapped, schedules the next tick at the next minute boundary.
    fn tick(&self) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            async move {
                // TimeZone::system may read /etc/localtime.
                let Ok(zone) = gio::spawn_blocking(TimeZone::system).await else {
                    tracing::error!("The task reading the system time zone panicked");
                    return;
                };
                editor.imp().zone.replace(Some(zone.clone()));
                editor.refresh_display();
                if editor.is_mapped() {
                    editor.schedule_tick(&zone);
                }
            }
        ));
    }

    /// Ticks again just after the next minute boundary in `zone`.
    fn schedule_tick(&self, zone: &TimeZone) {
        let now = SystemClock.now();
        let delay = clock::next_resend(now, zone, Clock1Source::Local, &[])
            .ok()
            .and_then(|next| Duration::try_from(next.duration_since(now)).ok())
            .unwrap_or(Duration::from_secs(60));
        let source = glib::timeout_add_local_once(
            delay + TICK_SLACK,
            glib::clone!(
                #[weak(rename_to = editor)]
                self,
                move || {
                    // The timer has fired: it is no longer there to remove.
                    editor.imp().tick.take();
                    editor.tick();
                }
            ),
        );
        if let Some(previous) = self.imp().tick.replace(Some(source)) {
            previous.remove();
        }
    }

    /// Stops the ticks while the editor is not shown.
    fn stop_ticking(&self) {
        if let Some(source) = self.imp().tick.take() {
            source.remove();
        }
    }
}
