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

//! The time zone search dialog of clocks 2 and 3: the zones of the tz database by city, country
//! and name, with their time now (docs/ui.md, "Clocks view").
//!
//! A pick looks the zone up off the main thread and is applied to the profile and clock the
//! dialog was opened for, only while the dialog is still open for them
//! ([`ZoneRequests`](crate::profile::clocks::ZoneRequests)).

use std::sync::Arc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gdk, gio, glib};
use jiff::tz::TimeZone;
use x52::clock::{Clock, SystemClock};

use crate::application::Application;
use crate::profile::IanaZone;
use crate::profile::clocks::{self, ZoneTarget};
use crate::widgets::ticker::{Ticker, Ticking};
use crate::zones::{self, ZoneEntry, ZoneTime};

mod imp {
    use std::cell::{OnceCell, RefCell};
    use std::sync::Arc;

    use adw::subclass::prelude::*;
    use gtk::glib;

    use crate::application::Application;
    use crate::profile::clocks::ZoneRequests;
    use crate::widgets::ticker::Ticker;
    use crate::zones::ZoneEntry;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/zone-dialog.ui")]
    pub struct ZoneDialog {
        #[template_child]
        pub(super) search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub(super) results_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) zone_list: TemplateChild<gtk::ListBox>,
        /// The application the picked zone goes to.
        pub(super) app: glib::WeakRef<Application>,
        /// The IANA name of the zone the clock shows now, marked in the list.
        pub(super) current: RefCell<Option<String>>,
        /// The zones listed, in the order of the list's rows.
        pub(super) entries: OnceCell<Arc<[ZoneEntry]>>,
        /// The row of each zone, in the same order.
        pub(super) rows: RefCell<Vec<adw::ActionRow>>,
        /// The clock the dialog is open for, and its picks.
        pub(super) requests: RefCell<ZoneRequests>,
        /// The minute tick of the times listed.
        pub(super) ticker: Ticker,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ZoneDialog {
        const NAME: &'static str = "GX52ZoneDialog";
        type Type = super::ZoneDialog;
        type ParentType = adw::Dialog;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ZoneDialog {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for ZoneDialog {
        fn map(&self) {
            self.parent_map();
            Ticker::tick(&*self.obj());
        }

        fn unmap(&self) {
            self.ticker.stop();
            self.parent_unmap();
        }
    }

    impl AdwDialogImpl for ZoneDialog {
        fn closed(&self) {
            // A pick still looking its zone up is dropped.
            self.requests.borrow_mut().close();
            self.parent_closed();
        }
    }
}

glib::wrapper! {
    pub struct ZoneDialog(ObjectSubclass<imp::ZoneDialog>)
        @extends adw::Dialog, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::ShortcutManager;
}

impl ZoneDialog {
    /// A dialog that picks the zone of `target` for `app`; `current` is the IANA name of the zone
    /// the clock shows now, if any.
    pub(crate) fn new(app: &Application, target: ZoneTarget, current: Option<String>) -> Self {
        let dialog: Self = glib::Object::new();
        let imp = dialog.imp();
        imp.app.set(Some(app));
        imp.current.replace(current);
        imp.requests.borrow_mut().open(target);
        dialog.load(app);
        dialog
    }

    /// The clock the dialog picks a zone for, while it is open.
    pub(crate) fn target(&self) -> Option<ZoneTarget> {
        self.imp().requests.borrow().target().cloned()
    }

    /// Connects the search and the list.
    fn setup(&self) {
        let imp = self.imp();
        imp.results_stack.set_visible_child_name("loading");
        imp.search_entry.set_key_capture_widget(Some(self));
        imp.zone_list.set_filter_func(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            #[upgrade_or]
            false,
            move |row| dialog.row_matches(row)
        ));
        imp.search_entry.connect_search_changed(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_| dialog.filter()
        ));
        // Enter in the search picks the first zone listed.
        imp.search_entry.connect_activate(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_| {
                if let Some(row) = dialog.first_shown_row() {
                    row.activate();
                }
            }
        ));
        imp.search_entry.connect_stop_search(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_| {
                dialog.close();
            }
        ));
        // Down in the search moves to the list.
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                if key != gdk::Key::Down {
                    return glib::Propagation::Proceed;
                }
                if let Some(row) = dialog.first_shown_row() {
                    row.grab_focus();
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            }
        ));
        imp.search_entry.add_controller(keys);
        imp.zone_list.connect_row_activated(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            move |_, row| {
                if let Ok(index) = usize::try_from(row.index()) {
                    dialog.pick(index);
                }
            }
        ));
    }

    /// Reads the zone list (once per application) and fills the list.
    fn load(&self, app: &Application) {
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            #[strong]
            app,
            async move {
                let entries = app.zone_catalog().await;
                dialog.fill(entries);
            }
        ));
    }

    /// Fills the list with `entries`.
    fn fill(&self, entries: Arc<[ZoneEntry]>) {
        let imp = self.imp();
        let current = imp.current.borrow().clone();
        let mut rows = Vec::with_capacity(entries.len());
        for entry in entries.iter() {
            let row = adw::ActionRow::builder()
                .title(&entry.city)
                .activatable(true)
                .use_markup(false)
                .build();
            if current.as_deref() == Some(entry.iana.as_str()) {
                let mark = gtk::Image::from_icon_name("object-select-symbolic");
                let label = gettext("Current time zone");
                mark.set_tooltip_text(Some(&label));
                mark.update_property(&[gtk::accessible::Property::Label(&label)]);
                row.add_suffix(&mark);
            }
            imp.zone_list.append(&row);
            rows.push(row);
        }
        imp.rows.replace(rows);
        if imp.entries.set(entries).is_err() {
            tracing::debug!("The zone list was filled twice");
        }
        self.filter();
        self.show_times();
    }

    /// Whether `row` matches the search.
    fn row_matches(&self, row: &gtk::ListBoxRow) -> bool {
        let imp = self.imp();
        let query = imp.search_entry.text();
        usize::try_from(row.index())
            .ok()
            .and_then(|index| imp.entries.get()?.get(index).cloned())
            .is_some_and(|entry| entry.matches(&query))
    }

    /// Filters the list by the search, and shows "No Results Found" when nothing matches.
    fn filter(&self) {
        let imp = self.imp();
        let Some(entries) = imp.entries.get() else {
            return;
        };
        imp.zone_list.invalidate_filter();
        let query = imp.search_entry.text();
        let any = entries.iter().any(|entry| entry.matches(&query));
        imp.results_stack
            .set_visible_child_name(if any { "list" } else { "empty" });
    }

    /// The first row the search shows.
    fn first_shown_row(&self) -> Option<gtk::ListBoxRow> {
        let imp = self.imp();
        let query = imp.search_entry.text();
        let index = imp
            .entries
            .get()?
            .iter()
            .position(|entry| entry.matches(&query))?;
        imp.zone_list.row_at_index(i32::try_from(index).ok()?)
    }

    /// Computes the time of every zone off the main thread, then shows it in the rows.
    fn show_times(&self) {
        let Some(entries) = self.imp().entries.get().cloned() else {
            return;
        };
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let now = SystemClock.now();
                let times = gio::spawn_blocking(move || {
                    entries
                        .iter()
                        .map(|entry| zones::zone_time(entry, now))
                        .collect::<Vec<_>>()
                })
                .await;
                let Ok(times) = times else {
                    tracing::error!("The task computing the zone times panicked");
                    return;
                };
                dialog.apply_times(&times);
            }
        ));
    }

    /// Shows `times`, one per row.
    fn apply_times(&self, times: &[Option<ZoneTime>]) {
        let imp = self.imp();
        let Some(entries) = imp.entries.get() else {
            return;
        };
        for ((row, entry), time) in imp.rows.borrow().iter().zip(entries.iter()).zip(times) {
            let (title, offset) = match time {
                Some(time) => (
                    // Translators: a time zone in the search: {city} is the place, {time} its
                    // time now ("Tokyo · 21:45").
                    gettext("{city} · {time}")
                        .replace("{city}", &entry.city)
                        .replace("{time}", &time.time),
                    Some(clocks::offset_text(time.offset_minutes)),
                ),
                None => (entry.city.clone(), None),
            };
            if row.title() != title {
                row.set_title(&title);
            }
            let subtitle = match (entry.country.is_empty(), offset) {
                (false, Some(offset)) => {
                    // Translators: under a time zone in the search: {country} is its country,
                    // {offset} its offset from UTC ("Japan · UTC+09:00").
                    gettext("{country} · {offset}")
                        .replace("{country}", &entry.country)
                        .replace("{offset}", &offset)
                }
                (true, Some(offset)) => offset,
                (false, None) => entry.country.clone(),
                (true, None) => entry.iana.clone(),
            };
            if row.subtitle().as_deref() != Some(subtitle.as_str()) {
                row.set_subtitle(&subtitle);
            }
        }
    }

    /// Picks the zone of row `index`: looks it up off the main thread, then applies it to the
    /// clock the dialog is open for, if it still is, and closes the dialog.
    fn pick(&self, index: usize) {
        let imp = self.imp();
        let Some(entry) = imp.entries.get().and_then(|entries| entries.get(index)) else {
            return;
        };
        let Some(request) = imp.requests.borrow_mut().pick() else {
            return;
        };
        let name = entry.iana.clone();
        glib::spawn_future_local(glib::clone!(
            #[weak(rename_to = dialog)]
            self,
            async move {
                let lookup = {
                    let name = name.clone();
                    gio::spawn_blocking(move || IanaZone::new(&name)).await
                };
                // The dialog may have closed, or another zone been picked, meanwhile.
                if !dialog.imp().requests.borrow().is_current(&request) {
                    tracing::debug!("Zone {name} picked for a dialog no longer open: dropped");
                    return;
                }
                let zone = match lookup {
                    Ok(Ok(zone)) => zone,
                    Ok(Err(error)) => {
                        tracing::warn!("Cannot use the time zone {name}: {error}");
                        return;
                    }
                    Err(_) => {
                        tracing::error!("The task looking up the time zone {name} panicked");
                        return;
                    }
                };
                if let Some(app) = dialog.imp().app.upgrade() {
                    let shown = app.editor_state().map(|state| state.profile);
                    if shown.as_ref() == Some(&request.target.profile) {
                        app.set_zone_clock_zone(&request.target, zone);
                    } else {
                        tracing::debug!(
                            "Zone {name} picked for a profile no longer shown: dropped"
                        );
                    }
                }
                dialog.close();
            }
        ));
    }
}

impl Ticking for ZoneDialog {
    fn ticker(&self) -> &Ticker {
        &self.imp().ticker
    }

    fn on_tick(&self, _zone: &TimeZone) {
        self.show_times();
    }
}
