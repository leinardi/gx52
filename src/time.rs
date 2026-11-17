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

//! The application's clock and local time zone, read through one place so they can be replaced.

use std::sync::OnceLock;

use jiff::Timestamp;
use jiff::tz::TimeZone;
use x52::clock::{Clock, SystemClock};

/// The clock and local zone, fixed by the first read or by [`set`].
static SOURCES: OnceLock<TimeSources> = OnceLock::new();

/// Where the application reads the time and the local time zone from.
pub(crate) struct TimeSources {
    /// The current time.
    pub(crate) clock: Box<dyn Clock>,
    /// The local time zone. It may read `/etc/localtime`, so it never runs on the main thread.
    pub(crate) local_zone: fn() -> TimeZone,
}

impl std::fmt::Debug for TimeSources {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TimeSources")
            .finish_non_exhaustive()
    }
}

/// The application's sources: the system's clock and local zone, unless [`set`] replaced them
/// before the first read.
fn sources() -> &'static TimeSources {
    sources_in(&SOURCES)
}

/// The sources in `cell`, fixed to the system's by the first read.
fn sources_in(cell: &OnceLock<TimeSources>) -> &TimeSources {
    cell.get_or_init(|| TimeSources {
        clock: Box::new(SystemClock),
        local_zone: TimeZone::system,
    })
}

/// Replaces the application's clock and local zone for the whole process, for a mode that shows
/// a fixed time.
///
/// # Errors
///
/// `sources` back, when the sources were already set or read: a reader may already have shown
/// the system's time.
pub(crate) fn set(sources: TimeSources) -> Result<(), TimeSources> {
    SOURCES.set(sources)
}

/// The application's current time.
pub(crate) fn now() -> Timestamp {
    sources().clock.now()
}

/// The function that reads the application's local time zone. It may block on file I/O.
pub(crate) fn local_zone_source() -> fn() -> TimeZone {
    sources().local_zone
}

/// The application's clock, as a [`Clock`] the device service can own.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct AppClock;

impl Clock for AppClock {
    fn now(&self) -> Timestamp {
        now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXED: &str = "2026-06-15T10:09:00Z";

    fn fixed() -> TimeSources {
        TimeSources {
            clock: Box::new(|| FIXED.parse::<Timestamp>().expect("a valid instant")),
            local_zone: || TimeZone::UTC,
        }
    }

    #[test]
    fn set_sources_replace_the_system_ones() {
        let cell = OnceLock::new();
        assert!(cell.set(fixed()).is_ok());
        let sources = sources_in(&cell);
        assert_eq!(sources.clock.now().to_string(), FIXED);
        assert_eq!((sources.local_zone)(), TimeZone::UTC);
    }

    #[test]
    fn the_sources_cannot_be_set_once_read() {
        let cell = OnceLock::new();
        let _ = sources_in(&cell);
        assert!(cell.set(fixed()).is_err());
    }
}
