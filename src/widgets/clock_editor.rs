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

//! The Clocks view of the profile editor: clock 1, clocks 2 and 3 and the date format, shared by
//! every mode, each with its time now as the MFD shows it (docs/ui.md, "Clocks view").

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use jiff::tz::TimeZone;
use x52::clock::{Clock, Clock1Source, SystemClock, ZoneSource};
use x52::command::OffsetClock;

use crate::profile::ZoneSpec;
use crate::profile::clocks::{self, DATE_FORMATS};
use crate::profile::editor::EditorState;
use crate::profile::mfd;
use crate::widgets::ticker::{TickSources, Ticker, Ticking};

/// The toggle of clock 1 on local time.
const LOCAL: &str = "local";
/// The toggle of clock 1 on UTC.
const UTC: &str = "utc";

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;

    use crate::profile::editor::EditorState;
    use crate::widgets::ZoneClockGroup;
    use crate::widgets::ticker::Ticker;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/clock-editor.ui")]
    pub struct ClockEditor {
        #[template_child]
        pub(super) clock1_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) clock1_source: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub(super) clock1_h24: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub(super) clock2_group: TemplateChild<ZoneClockGroup>,
        #[template_child]
        pub(super) clock3_group: TemplateChild<ZoneClockGroup>,
        #[template_child]
        pub(super) date_group: TemplateChild<adw::PreferencesGroup>,
        /// The date format row, built in code.
        pub(super) date_row: OnceCell<adw::ComboRow>,
        /// The date format labels, with today's date as an example.
        pub(super) date_labels: OnceCell<gtk::StringList>,
        /// The state shown.
        pub(super) state: RefCell<Option<EditorState>>,
        /// The minute tick of the times shown.
        pub(super) ticker: Ticker,
        /// Set while the editor shows a state itself, so the change is not taken as the user's.
        pub(super) syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ClockEditor {
        const NAME: &'static str = "GX52ClockEditor";
        type Type = super::ClockEditor;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            ZoneClockGroup::ensure_type();
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ClockEditor {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup();
            // The zone is read once now, so the times are there the first time it is shown.
            Ticker::tick(&*obj);
        }
    }

    impl WidgetImpl for ClockEditor {
        fn map(&self) {
            self.parent_map();
            Ticker::tick(&*self.obj());
        }

        fn unmap(&self) {
            self.ticker.stop();
            self.parent_unmap();
        }
    }

    impl BinImpl for ClockEditor {}
}

glib::wrapper! {
    pub struct ClockEditor(ObjectSubclass<imp::ClockEditor>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ClockEditor {
    /// Builds the date row and connects the rows.
    fn setup(&self) {
        let imp = self.imp();
        imp.clock2_group.set_clock(OffsetClock::Clock2);
        imp.clock3_group.set_clock(OffsetClock::Clock3);

        let labels: Vec<String> = DATE_FORMATS
            .into_iter()
            .map(clocks::date_format_label)
            .collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        let model = gtk::StringList::new(&labels);
        let date_row = adw::ComboRow::builder()
            .title(gettext("Date Format"))
            .model(&model)
            .build();
        imp.date_group.add(&date_row);
        date_row.connect_selected_notify(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |row| {
                if editor.imp().syncing.get() {
                    return;
                }
                let Some(format) = usize::try_from(row.selected())
                    .ok()
                    .and_then(|index| DATE_FORMATS.get(index).copied())
                else {
                    return;
                };
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_date_format(format);
                }
            }
        ));
        if imp.date_row.set(date_row).is_err() {
            tracing::debug!("The date row is built once");
        }
        if imp.date_labels.set(model).is_err() {
            tracing::debug!("The date labels are built once");
        }

        imp.clock1_source.connect_active_name_notify(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |toggles| {
                if editor.imp().syncing.get() {
                    return;
                }
                let source = match toggles.active_name().as_deref() {
                    Some(LOCAL) => Clock1Source::Local,
                    Some(UTC) => Clock1Source::Utc,
                    _ => return,
                };
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_clock1_source(source);
                }
            }
        ));
        imp.clock1_h24.connect_active_notify(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |row| {
                if editor.imp().syncing.get() {
                    return;
                }
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_clock1_h24(row.is_active());
                }
            }
        ));
    }

    /// Shows the clocks of `state`.
    pub(crate) fn show(&self, state: &EditorState) {
        let imp = self.imp();
        let clocks = &state.clocks;
        imp.syncing.set(true);
        imp.clock1_source
            .set_active_name(Some(match clocks.clock1.source {
                Clock1Source::Local => LOCAL,
                Clock1Source::Utc => UTC,
            }));
        imp.clock1_h24.set_active(clocks.clock1.h24);
        if let Some(row) = imp.date_row.get() {
            let position = DATE_FORMATS
                .iter()
                .position(|format| *format == clocks.date_format)
                .and_then(|index| u32::try_from(index).ok())
                .unwrap_or(gtk::INVALID_LIST_POSITION);
            row.set_selected(position);
        }
        imp.syncing.set(false);
        imp.clock2_group.show(&state.profile, &clocks.clock2);
        imp.clock3_group.show(&state.profile, &clocks.clock3);
        imp.state.replace(Some(state.clone()));
        self.refresh_times();
        // A clock may now follow another zone, whose offset can change before the minute.
        Ticker::follow(self);
    }

    /// Closes the time zone search dialog of clock 2 or 3, if open: the editor no longer shows a
    /// profile.
    pub(crate) fn close_zone_dialogs(&self) {
        let imp = self.imp();
        imp.clock2_group.close_dialog();
        imp.clock3_group.close_dialog();
    }

    /// Shows the times and the date again.
    fn refresh_times(&self) {
        let imp = self.imp();
        let Some(zone) = imp.ticker.zone() else {
            // The zone is read when the editor is mapped; the times wait for it.
            return;
        };
        let Some(app) = crate::widgets::app_of(self) else {
            return;
        };
        let Some(source) = imp
            .state
            .borrow()
            .as_ref()
            .map(|state| state.clocks.clock1.source)
        else {
            return;
        };
        let now = SystemClock.now();
        if let Some(preview) = app.clocks_preview(now, &zone) {
            let description = match source {
                // Translators: under clock 1: {time} is its time now, {date} the date the MFD
                // shows ("Shows 14:05 · 11.11.26 · system time zone").
                Clock1Source::Local => gettext("Shows {time} · {date} · system time zone"),
                // Translators: under clock 1 on UTC: {time} is its time now, {date} the date the
                // MFD shows ("Shows 13:05 · 11.11.26 · UTC").
                Clock1Source::Utc => gettext("Shows {time} · {date} · UTC"),
            }
            .replace("{time}", &preview.clock1().text())
            .replace("{date}", &preview.date);
            imp.clock1_group.set_description(Some(&description));
            imp.clock2_group.show_times(&preview, now);
            imp.clock3_group.show_times(&preview, now);
        }
        self.refresh_date_labels(source, now, &zone);
    }

    /// Shows today's date in each date format label, in clock 1's zone.
    fn refresh_date_labels(&self, source: Clock1Source, now: jiff::Timestamp, zone: &TimeZone) {
        let imp = self.imp();
        let (Some(row), Some(model)) = (imp.date_row.get(), imp.date_labels.get()) else {
            return;
        };
        let labels: Vec<String> = DATE_FORMATS
            .into_iter()
            .map(|format| {
                let label = clocks::date_format_label(format);
                match mfd::format_date(format, source, now, zone) {
                    // Translators: a date format and today's date in it ("DD-MM-YY · 11.11.26").
                    Ok(date) => gettext("{format} · {date}")
                        .replace("{format}", &label)
                        .replace("{date}", &date),
                    Err(_) => label,
                }
            })
            .collect();
        let same = labels.iter().enumerate().all(|(index, label)| {
            u32::try_from(index)
                .ok()
                .and_then(|index| model.string(index))
                .is_some_and(|current| current == label.as_str())
        });
        if same {
            return;
        }
        let selected = row.selected();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        imp.syncing.set(true);
        model.splice(0, model.n_items(), &labels);
        row.set_selected(selected);
        imp.syncing.set(false);
    }
}

impl Ticking for ClockEditor {
    fn ticker(&self) -> &Ticker {
        &self.imp().ticker
    }

    fn on_tick(&self, _zone: &TimeZone) {
        self.refresh_times();
    }

    fn tick_sources(&self) -> TickSources {
        let state = self.imp().state.borrow();
        let Some(clocks) = state.as_ref().map(|state| &state.clocks) else {
            return TickSources::default();
        };
        let sources = [&clocks.clock2, &clocks.clock3]
            .into_iter()
            .filter_map(|clock| match &clock.source {
                ZoneSpec::Iana(zone) => Some(ZoneSource::Zone(zone.zone().clone())),
                ZoneSpec::Fixed(_) => None,
            })
            .collect();
        TickSources {
            clock1: clocks.clock1.source,
            zones: sources,
        }
    }
}
