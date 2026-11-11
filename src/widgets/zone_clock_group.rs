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

//! The group of clock 2 or 3 in the Clocks view: a time zone or a fixed offset, and 12 or 24 h
//! (docs/ui.md, "Clocks view").

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use jiff::Timestamp;
use x52::clock::{Clock, FixedOffset, SystemClock};
use x52::command::OffsetClock;

use crate::profile::clocks::{self, ClocksPreview, ZoneTarget};
use crate::profile::{ProfileId, ZoneClock, ZoneSpec};
use crate::widgets::ZoneDialog;
use crate::zones;

/// The toggle of a time zone.
const ZONE: &str = "zone";
/// The toggle of a fixed offset.
const FIXED: &str = "fixed";

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use adw::prelude::*;
    use adw::subclass::prelude::*;
    use gtk::glib;
    use x52::command::OffsetClock;

    use crate::profile::{ProfileId, ZoneClock};
    use crate::widgets::ZoneDialog;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/zone-clock-group.ui")]
    pub struct ZoneClockGroup {
        #[template_child]
        pub(super) source_toggles: TemplateChild<adw::ToggleGroup>,
        #[template_child]
        pub(super) zone_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) offset_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub(super) h24_row: TemplateChild<adw::SwitchRow>,
        /// Clock 2 or 3, set once.
        pub(super) clock: OnceCell<OffsetClock>,
        /// The profile and clock shown.
        pub(super) shown: RefCell<Option<(ProfileId, ZoneClock)>>,
        /// The search dialog, while it is open.
        pub(super) dialog: glib::WeakRef<ZoneDialog>,
        /// Set while the group shows a state itself, so the change is not taken as the user's.
        pub(super) syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ZoneClockGroup {
        const NAME: &'static str = "GX52ZoneClockGroup";
        type Type = super::ZoneClockGroup;
        type ParentType = adw::PreferencesGroup;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ZoneClockGroup {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }

        fn dispose(&self) {
            if let Some(dialog) = self.dialog.upgrade() {
                dialog.force_close();
            }
        }
    }

    impl WidgetImpl for ZoneClockGroup {}
    impl PreferencesGroupImpl for ZoneClockGroup {}
}

glib::wrapper! {
    pub struct ZoneClockGroup(ObjectSubclass<imp::ZoneClockGroup>)
        @extends adw::PreferencesGroup, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl ZoneClockGroup {
    /// Makes the group edit `clock`; called once, before it is shown.
    pub(crate) fn set_clock(&self, clock: OffsetClock) {
        let imp = self.imp();
        if imp.clock.set(clock).is_err() {
            tracing::debug!("The clock of a zone clock group is set once");
            return;
        }
        let number = clocks::clock_number(clock).to_string();
        // Translators: the title of the group of clock 2 or 3; {number} is 2 or 3.
        self.set_title(&gettext("Clock {number}").replace("{number}", &number));
        let label = |template: String| template.replace("{number}", &number);
        imp.source_toggles
            .update_property(&[gtk::accessible::Property::Label(&label(
                // Translators: {number} is 2 or 3.
                gettext("Clock {number} source"),
            ))]);
        imp.zone_row
            .update_property(&[gtk::accessible::Property::Label(&label(
                // Translators: {number} is 2 or 3.
                gettext("Clock {number} time zone"),
            ))]);
        imp.offset_row
            .update_property(&[gtk::accessible::Property::Label(&label(
                // Translators: {number} is 2 or 3.
                gettext("Clock {number} offset from UTC"),
            ))]);
        imp.h24_row
            .update_property(&[gtk::accessible::Property::Label(&label(
                // Translators: {number} is 2 or 3.
                gettext("Clock {number} 24-hour format"),
            ))]);
    }

    /// The clock the group edits.
    fn clock(&self) -> Option<OffsetClock> {
        self.imp().clock.get().copied()
    }

    /// Connects the rows.
    fn setup(&self) {
        let imp = self.imp();
        imp.source_toggles.connect_active_name_notify(glib::clone!(
            #[weak(rename_to = group)]
            self,
            move |toggles| {
                if group.imp().syncing.get() {
                    return;
                }
                let Some(name) = toggles.active_name() else {
                    return;
                };
                group.source_picked(&name);
            }
        ));
        imp.zone_row.connect_activated(glib::clone!(
            #[weak(rename_to = group)]
            self,
            move |_| group.open_dialog()
        ));
        imp.offset_row.connect_output(|row| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the adjustment is bounded to −720–840 minutes"
            )]
            let minutes = row.value().round() as i32;
            row.set_text(&clocks::offset_text(minutes));
            true
        });
        imp.offset_row
            .connect_input(|row| Some(clocks::parse_offset(&row.text()).map(f64::from).ok_or(())));
        imp.offset_row.connect_value_notify(glib::clone!(
            #[weak(rename_to = group)]
            self,
            move |row| {
                if group.imp().syncing.get() {
                    return;
                }
                group.offset_changed(row.value());
            }
        ));
        imp.h24_row.connect_active_notify(glib::clone!(
            #[weak(rename_to = group)]
            self,
            move |row| {
                if group.imp().syncing.get() {
                    return;
                }
                if let (Some(app), Some(clock)) = (crate::widgets::app_of(&group), group.clock()) {
                    app.set_zone_clock_h24(clock, row.is_active());
                }
            }
        ));
    }

    /// The user picked the time zone or the fixed offset toggle.
    fn source_picked(&self, name: &str) {
        let (Some(app), Some(clock)) = (crate::widgets::app_of(self), self.clock()) else {
            return;
        };
        match name {
            FIXED => app.switch_zone_clock_to_fixed(clock, SystemClock.now()),
            // With no zone kept for this profile and clock, the toggle goes back to the fixed
            // offset (the edit changed nothing) and the dialog asks for one.
            ZONE => {
                let switched = app.switch_zone_clock_to_zone(clock);
                if !switched {
                    self.open_dialog();
                }
            }
            _ => {}
        }
    }

    /// The user changed the offset.
    fn offset_changed(&self, value: f64) {
        let (Some(app), Some(clock)) = (crate::widgets::app_of(self), self.clock()) else {
            return;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the adjustment is bounded to −720–840 minutes"
        )]
        let minutes = value.round() as i16;
        match FixedOffset::new(minutes) {
            Ok(offset) => app.set_zone_clock_offset(clock, offset),
            Err(error) => tracing::debug!("Offset {minutes} refused: {error}"),
        }
    }

    /// Opens the search dialog for the clock of the profile shown.
    fn open_dialog(&self) {
        let imp = self.imp();
        let (Some(app), Some(clock)) = (crate::widgets::app_of(self), self.clock()) else {
            return;
        };
        let Some((profile, shown)) = imp.shown.borrow().clone() else {
            return;
        };
        let current = match &shown.source {
            ZoneSpec::Iana(zone) => Some(zone.name().to_owned()),
            ZoneSpec::Fixed(_) => None,
        };
        let dialog = ZoneDialog::new(&app, ZoneTarget { profile, clock }, current);
        imp.dialog.set(Some(&dialog));
        dialog.present(Some(self));
    }

    /// Closes the search dialog, if open.
    pub(crate) fn close_dialog(&self) {
        if let Some(dialog) = self.imp().dialog.upgrade() {
            dialog.force_close();
        }
    }

    /// Shows `clock` of `profile`. A search dialog open for another profile closes.
    pub(crate) fn show(&self, profile: &ProfileId, clock: &ZoneClock) {
        let imp = self.imp();
        if let Some(dialog) = imp.dialog.upgrade()
            && dialog
                .target()
                .is_none_or(|target| &target.profile != profile)
        {
            dialog.force_close();
        }
        imp.syncing.set(true);
        let zone = matches!(clock.source, ZoneSpec::Iana(_));
        imp.source_toggles
            .set_active_name(Some(if zone { ZONE } else { FIXED }));
        if let ZoneSpec::Fixed(offset) = clock.source {
            imp.offset_row.set_value(f64::from(offset.minutes()));
        }
        imp.h24_row.set_active(clock.h24);
        imp.syncing.set(false);
        imp.zone_row.set_visible(zone);
        imp.offset_row.set_visible(!zone);
        imp.shown.replace(Some((profile.clone(), clock.clone())));
    }

    /// Shows the times of `preview` at `now`.
    pub(crate) fn show_times(&self, preview: &ClocksPreview, now: Timestamp) {
        let imp = self.imp();
        let Some(clock) = self.clock() else {
            return;
        };
        let shown = imp.shown.borrow();
        let Some((_, zone_clock)) = shown.as_ref() else {
            return;
        };
        let time = preview.zone_clock(clock).text();
        let description = match &zone_clock.source {
            ZoneSpec::Iana(zone) => {
                let offset = clocks::offset_text(clocks::utc_offset_minutes(zone.zone(), now));
                // Translators: under clock 2 or 3: {time} is its time now, {offset} its offset
                // from UTC ("Shows 21:45 · UTC+09:00").
                let text = gettext("Shows {time} · {offset}")
                    .replace("{time}", &time)
                    .replace("{offset}", &offset);
                // Translators: the time zone row: {place} names the zone, {time} is its time now
                // ("Tokyo · 21:45").
                imp.zone_row.set_subtitle(
                    &gettext("{place} · {time}")
                        .replace("{place}", &zones::city_of(zone.name()))
                        .replace("{time}", &time),
                );
                text
            }
            ZoneSpec::Fixed(offset) => {
                // Translators: under clock 2 or 3 on a fixed offset: {time} is its time now,
                // {offset} the offset from UTC ("Shows 18:30 · UTC+05:45 all year").
                gettext("Shows {time} · {offset} all year")
                    .replace("{time}", &time)
                    .replace(
                        "{offset}",
                        &clocks::offset_text(i32::from(offset.minutes())),
                    )
            }
        };
        self.set_description(Some(&description));
    }
}
