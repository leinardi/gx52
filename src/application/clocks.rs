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

//! The edits of the Clocks view: the clocks and the date of the shown device type's active
//! profile, shared by every mode (docs/ui.md, "Clocks view"), and the zone list of its search
//! dialog.

use std::sync::Arc;

use adw::subclass::prelude::*;
use gtk::gio;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use x52::clock::{Clock1Source, DateFormat, FixedOffset};
use x52::command::OffsetClock;

use super::Application;
use crate::profile::clocks::{self, ClocksPreview, ZoneTarget};
use crate::profile::{IanaZone, ZoneSpec};
use crate::zones::{self, ZoneEntry};

impl Application {
    /// Makes clock 1 show the local time or UTC.
    pub(crate) fn set_clock1_source(&self, source: Clock1Source) {
        self.edit_shown_profile(|_, profile| profile.clock1.source = source);
    }

    /// Shows clock 1 in 24 h when `h24`, in 12 h otherwise.
    pub(crate) fn set_clock1_h24(&self, h24: bool) {
        self.edit_shown_profile(|_, profile| profile.clock1.h24 = h24);
    }

    /// Shows clock 2 or 3 in 24 h when `h24`, in 12 h otherwise.
    pub(crate) fn set_zone_clock_h24(&self, clock: OffsetClock, h24: bool) {
        self.edit_shown_profile(|_, profile| profile.zone_clock_mut(clock).h24 = h24);
    }

    /// Sets the field order of the date.
    pub(crate) fn set_date_format(&self, format: DateFormat) {
        self.edit_shown_profile(|_, profile| profile.date_format = format);
    }

    /// Sets the fixed offset of clock 2 or 3, which already shows a fixed offset.
    pub(crate) fn set_zone_clock_offset(&self, clock: OffsetClock, offset: FixedOffset) {
        self.edit_shown_profile(|_, profile| {
            let source = &mut profile.zone_clock_mut(clock).source;
            if matches!(source, ZoneSpec::Fixed(_)) {
                *source = ZoneSpec::Fixed(offset);
            }
        });
    }

    /// Makes clock 2 or 3 show a fixed offset: its zone's offset at `now`, rounded to 15 minutes.
    /// The zone it leaves is kept for this profile and clock, for a switch back, once the edit is
    /// made.
    pub(crate) fn switch_zone_clock_to_fixed(&self, clock: OffsetClock, now: Timestamp) {
        let mut left = None;
        let edited = self.edit_shown_profile(|id, profile| {
            let source = &mut profile.zone_clock_mut(clock).source;
            if let ZoneSpec::Iana(zone) = source {
                let Some(fixed) = clocks::fixed_from_zone(zone.zone(), now) else {
                    tracing::warn!("No fixed offset for the time zone {}", zone.name());
                    return;
                };
                left = Some((
                    ZoneTarget {
                        profile: id.clone(),
                        clock,
                    },
                    zone.clone(),
                ));
                *source = ZoneSpec::Fixed(fixed);
            }
        });
        if edited && let Some((target, zone)) = left {
            self.imp().zone_memory.borrow_mut().remember(target, zone);
        }
    }

    /// Makes clock 2 or 3 show the time zone it had this session in this profile, and returns
    /// whether it shows a time zone now; with none kept, the clock stays as it is and the caller
    /// asks for a zone.
    pub(crate) fn switch_zone_clock_to_zone(&self, clock: OffsetClock) -> bool {
        let mut already = false;
        let edited = self.edit_shown_profile(|id, profile| {
            let source = &mut profile.zone_clock_mut(clock).source;
            if matches!(source, ZoneSpec::Iana(_)) {
                already = true;
                return;
            }
            let target = ZoneTarget {
                profile: id.clone(),
                clock,
            };
            if let Some(zone) = self.imp().zone_memory.borrow().recall(&target) {
                *source = ZoneSpec::Iana(zone.clone());
            }
        });
        already || edited
    }

    /// Makes the clock of `target` show `zone`: the profile it names, whichever profile is shown
    /// now. The zone is also kept for that profile and clock once the edit is made (or when the
    /// clock shows it already).
    pub(crate) fn set_zone_clock_zone(&self, target: &ZoneTarget, zone: IanaZone) {
        let mut shown = false;
        let edited = self.edit_profile(&target.profile, |profile| {
            let source = &mut profile.zone_clock_mut(target.clock).source;
            shown = *source == ZoneSpec::Iana(zone.clone());
            *source = ZoneSpec::Iana(zone.clone());
        });
        if edited || shown {
            self.imp()
                .zone_memory
                .borrow_mut()
                .remember(target.clone(), zone);
        }
    }

    /// What the Clocks view shows at `now`, with `local` as the system time zone: the shown
    /// device type's active profile, as the device shows it.
    pub(crate) fn clocks_preview(&self, now: Timestamp, local: &TimeZone) -> Option<ClocksPreview> {
        let device_type = self.shown_device_type()?;
        let (_, state) = self.desired_state(device_type, self.shown_mode())?;
        ClocksPreview::of(&state, now, local)
            .inspect_err(|error| tracing::warn!("Cannot show the clocks: {error}"))
            .ok()
    }

    /// The zones of the search dialog, read off the main thread the first time.
    pub(crate) async fn zone_catalog(&self) -> Arc<[ZoneEntry]> {
        if let Some(catalog) = self.imp().zone_catalog.borrow().clone() {
            return catalog;
        }
        let entries = gio::spawn_blocking(zones::load).await.unwrap_or_else(|_| {
            tracing::error!("The task reading the time zone list panicked");
            Vec::new()
        });
        let catalog: Arc<[ZoneEntry]> = entries.into();
        if !catalog.is_empty() {
            self.imp().zone_catalog.replace(Some(Arc::clone(&catalog)));
        }
        catalog
    }
}
