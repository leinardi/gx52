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

//! The minute tick of the views that show the clocks: the system time zone read off the main
//! thread, then a refresh just after each minute boundary or time zone change while the view is
//! mapped.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gio, glib};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use x52::clock::{self, Clock, Clock1Source, SystemClock, ZoneSource};

/// How long after a minute boundary a view ticks, so the system clock is surely past it.
const TICK_SLACK: Duration = Duration::from_millis(50);

/// The delay before a tick when the next one cannot be computed.
const FALLBACK_DELAY: Duration = Duration::from_secs(60);

/// The clocks a view shows, which decide when it ticks: each minute boundary of clock 1, and each
/// offset change of clock 1's zone or of a zone of `zones`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TickSources {
    /// What clock 1 shows.
    pub(crate) clock1: Clock1Source,
    /// The sources of the other clocks shown.
    pub(crate) zones: Vec<ZoneSource>,
}

impl Default for TickSources {
    /// Clock 1 on local time, and no other zone.
    fn default() -> Self {
        Self {
            clock1: Clock1Source::Local,
            zones: Vec::new(),
        }
    }
}

impl TickSources {
    /// How long from `now` until the next tick, with `local` as the system time zone: until the
    /// next minute boundary or offset change, whichever comes first.
    pub(crate) fn delay(&self, now: Timestamp, local: &TimeZone) -> Duration {
        let zones: Vec<&ZoneSource> = self.zones.iter().collect();
        clock::next_resend(now, local, self.clock1, &zones)
            .ok()
            .and_then(|next| Duration::try_from(next.duration_since(now)).ok())
            .unwrap_or(FALLBACK_DELAY)
    }
}

/// The tick state of one view: the system time zone last read, and the timer of the next tick.
#[derive(Debug, Default)]
pub(crate) struct Ticker {
    zone: RefCell<Option<TimeZone>>,
    timer: RefCell<Option<glib::SourceId>>,
    /// The sources the pending timer was computed for.
    sources: RefCell<Option<TickSources>>,
    /// Counts the timers scheduled, so a timer that fires clears only its own id.
    scheduled: Cell<u64>,
}

/// A view that shows the clocks and ticks with a [`Ticker`].
pub(crate) trait Ticking: IsA<gtk::Widget> {
    /// The view's ticker.
    fn ticker(&self) -> &Ticker;

    /// Shows the clocks again, the system time zone being `zone`.
    fn on_tick(&self, zone: &TimeZone);

    /// The clocks the view shows: a tick also comes when one of their zones changes its offset.
    /// Clock 1 on local time, and no other zone, by default.
    fn tick_sources(&self) -> TickSources {
        TickSources::default()
    }
}

impl Ticker {
    /// The system time zone last read; `None` before the first read completes.
    pub(crate) fn zone(&self) -> Option<TimeZone> {
        self.zone.borrow().clone()
    }

    /// Reads the system time zone off the main thread, shows `view` and, while it is mapped,
    /// schedules the next tick.
    pub(crate) fn tick<W: Ticking>(view: &W) {
        let weak = view.downgrade();
        glib::spawn_future_local(async move {
            // TimeZone::system may read /etc/localtime.
            let Ok(zone) = gio::spawn_blocking(TimeZone::system).await else {
                tracing::error!("The task reading the system time zone panicked");
                return;
            };
            let Some(view) = weak.upgrade() else {
                return;
            };
            view.ticker().zone.replace(Some(zone.clone()));
            view.on_tick(&zone);
            if view.is_mapped() {
                Self::schedule(&view, &zone);
            }
        });
    }

    /// Computes the next tick again when the clocks `view` shows changed since the pending timer
    /// was scheduled: a new zone may change its offset before that timer fires. Call it after the
    /// view shows new clocks.
    pub(crate) fn follow<W: Ticking>(view: &W) {
        let ticker = view.ticker();
        let Some(zone) = ticker.zone() else {
            // The first read schedules the timer with the sources then shown.
            return;
        };
        if view.is_mapped() && ticker.needs_reschedule(&view.tick_sources()) {
            Self::schedule(view, &zone);
        }
    }

    /// Whether the pending timer was computed for other sources than `sources`, or there is none
    /// (the sources are cleared with the timer).
    fn needs_reschedule(&self, sources: &TickSources) -> bool {
        self.sources.borrow().as_ref() != Some(sources)
    }

    /// Ticks `view` again just after the next minute boundary or time zone change.
    fn schedule<W: Ticking>(view: &W, zone: &TimeZone) {
        let sources = view.tick_sources();
        let delay = sources.delay(SystemClock.now(), zone);
        let ticker = view.ticker();
        ticker.sources.replace(Some(sources));
        let number = ticker.scheduled.get().wrapping_add(1);
        ticker.scheduled.set(number);
        let weak = view.downgrade();
        let source = glib::timeout_add_local_once(delay + TICK_SLACK, move || {
            let Some(view) = weak.upgrade() else {
                return;
            };
            let ticker = view.ticker();
            // The timer has fired: it is no longer there to remove. A newer one stays.
            if ticker.scheduled.get() == number {
                ticker.timer.take();
                ticker.sources.take();
            }
            Self::tick(&view);
        });
        if let Some(previous) = view.ticker().timer.replace(Some(source)) {
            previous.remove();
        }
    }

    /// Stops the ticks while the view is not shown.
    pub(crate) fn stop(&self) {
        if let Some(source) = self.timer.take() {
            source.remove();
        }
        self.sources.take();
    }
}

#[cfg(test)]
mod tests;
