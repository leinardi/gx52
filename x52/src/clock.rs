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
//
// The Clock trait, SystemClock and FakeClock are adapted from
// swarm-scheduler-exporter/src/clock.rs at 3bb5ab8530b7, originally MIT-licensed,
// Copyright (c) 2026 Roberto Leinardi

//! The MFD clocks: what clock 1, clocks 2 and 3 and the date show at a given instant, and when
//! they have to be sent again.
//!
//! The device clock does not tick and clocks 2 and 3 are offsets from clock 1, not from UTC
//! (docs/protocol.md, "USB vendor commands"). Every function here is pure over the current
//! instant and the system time zone, which the caller passes in: the instant comes from a
//! [`Clock`], the one place that reads the system time.

use jiff::Timestamp;
use jiff::tz::TimeZone;

use crate::command::{ClockOffset, ClockTime, Command, DateFields, OffsetClock};
use crate::error::RangeError;

/// Minutes in a day: offsets that differ by a day show the same time.
const DAY_MINUTES: i32 = 1440;

/// A source of the current time.
pub trait Clock: Send + Sync {
    /// The current time.
    fn now(&self) -> Timestamp;
}

/// A closure is a clock, so a test can hand one in directly.
impl<F> Clock for F
where
    F: Fn() -> Timestamp + Send + Sync,
{
    fn now(&self) -> Timestamp {
        self()
    }
}

/// The system's wall clock.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    #[expect(
        clippy::disallowed_methods,
        reason = "the one wall-clock read: every other time comes from an injected Clock"
    )]
    fn now(&self) -> Timestamp {
        Timestamp::now()
    }
}

#[cfg(any(test, feature = "test-support"))]
pub use fake::FakeClock;

#[cfg(any(test, feature = "test-support"))]
mod fake {
    use std::sync::{Mutex, PoisonError};

    use jiff::{SignedDuration, Timestamp};

    use super::Clock;

    /// A clock set by hand, for tests.
    #[derive(Debug)]
    pub struct FakeClock {
        now: Mutex<Timestamp>,
    }

    impl FakeClock {
        /// A clock reading `now` until it is changed.
        #[must_use]
        pub fn new(now: Timestamp) -> Self {
            Self {
                now: Mutex::new(now),
            }
        }

        /// Sets the time.
        pub fn set(&self, now: Timestamp) {
            *self.now.lock().unwrap_or_else(PoisonError::into_inner) = now;
        }

        /// Moves the time by `by`, saturating at the ends of the range.
        pub fn advance(&self, by: SignedDuration) {
            let mut now = self.now.lock().unwrap_or_else(PoisonError::into_inner);
            // Adding a SignedDuration saturates; only a span with calendar units could fail.
            *now = now.saturating_add(by).unwrap_or(*now);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Timestamp {
            *self.now.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }
}

/// Why a clock value could not be computed.
#[derive(Debug, thiserror::Error)]
pub enum ClockError {
    /// An instant that cannot be represented, near the ends of the timestamp range.
    #[error("the {what} is outside the range of representable instants")]
    OutOfRange {
        /// What could not be computed.
        what: &'static str,
    },
    /// A time zone name that the system's time zone database does not have.
    #[error("unknown time zone {name:?}")]
    UnknownZone {
        /// The name given.
        name: String,
        /// Why the lookup failed.
        #[source]
        source: jiff::Error,
    },
    /// A computed value outside the range of its command.
    #[error(transparent)]
    Value(#[from] RangeError),
}

/// What clock 1 shows: the system's local time, or UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Clock1Source {
    /// The system time zone, following its daylight saving time.
    Local,
    /// UTC.
    Utc,
}

/// A fixed offset from UTC in minutes, `-720`–`+840` (UTC−12:00 to UTC+14:00), with no daylight
/// saving time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FixedOffset(i16);

impl FixedOffset {
    /// The smallest offset, UTC−12:00.
    pub const MIN_MINUTES: i16 = -720;
    /// The largest offset, UTC+14:00.
    pub const MAX_MINUTES: i16 = 840;

    /// An offset from UTC in minutes.
    ///
    /// # Errors
    ///
    /// [`RangeError::OutOfRange`] outside `-720`–`+840`.
    pub fn new(minutes: i16) -> Result<Self, RangeError> {
        Self::checked(minutes).ok_or_else(|| {
            RangeError::out_of_range(
                "fixed offset",
                minutes,
                Self::MIN_MINUTES,
                Self::MAX_MINUTES,
            )
        })
    }

    /// An offset from UTC in minutes, or `None` outside `-720`–`+840`; usable in constants.
    pub(crate) const fn checked(minutes: i16) -> Option<Self> {
        if minutes < Self::MIN_MINUTES || minutes > Self::MAX_MINUTES {
            return None;
        }
        Some(Self(minutes))
    }

    /// The offset in minutes.
    #[must_use]
    pub const fn minutes(self) -> i16 {
        self.0
    }
}

/// What clock 2 or 3 shows: a time zone with its daylight saving time, or a fixed offset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoneSource {
    /// A time zone, usually from the system's time zone database.
    Zone(TimeZone),
    /// A fixed offset from UTC.
    Fixed(FixedOffset),
}

impl ZoneSource {
    /// The IANA time zone `name` (for example `Europe/Rome`) from the system's database.
    ///
    /// # Errors
    ///
    /// [`ClockError::UnknownZone`] when the database has no zone of that name.
    pub fn iana(name: &str) -> Result<Self, ClockError> {
        TimeZone::get(name)
            .map(Self::Zone)
            .map_err(|source| ClockError::UnknownZone {
                name: name.to_owned(),
                source,
            })
    }

    /// The offset from UTC at `now`, in seconds.
    fn offset_seconds(&self, now: Timestamp) -> i32 {
        match self {
            Self::Zone(zone) => zone.to_offset(now).seconds(),
            Self::Fixed(offset) => i32::from(offset.minutes()) * 60,
        }
    }
}

/// Which of day, month and year goes in which date field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DateFormat {
    /// Day, month, year.
    DdMmYy,
    /// Month, day, year.
    MmDdYy,
    /// Year, month, day.
    YyMmDd,
}

/// The time zone clock 1 follows.
fn clock1_zone(local: &TimeZone, source: Clock1Source) -> TimeZone {
    match source {
        Clock1Source::Local => local.clone(),
        Clock1Source::Utc => TimeZone::UTC,
    }
}

/// The time of day clock 1 shows at `now`: the time in clock 1's zone.
///
/// # Errors
///
/// [`ClockError::Value`] if the time of day could not be carried by a command, which a valid time
/// zone never produces.
pub fn clock1_time(
    now: Timestamp,
    local: &TimeZone,
    source: Clock1Source,
) -> Result<ClockTime, ClockError> {
    let time = clock1_zone(local, source).to_datetime(now);
    let hour = u8::try_from(time.hour())
        .map_err(|_| RangeError::out_of_range("hour", time.hour(), 0, 23))?;
    let minute = u8::try_from(time.minute())
        .map_err(|_| RangeError::out_of_range("minute", time.minute(), 0, 59))?;
    Ok(ClockTime::new(hour, minute)?)
}

/// The clock 1 command for `now`: the time of day in clock 1's zone.
///
/// # Errors
///
/// [`ClockError::Value`] if the time of day could not be carried by the command, which a valid
/// time zone never produces.
pub fn clock1_command(
    now: Timestamp,
    local: &TimeZone,
    source: Clock1Source,
    h24: bool,
) -> Result<Command, ClockError> {
    Ok(Command::Clock1 {
        time: clock1_time(now, local, source)?,
        h24,
    })
}

/// The offset that makes clock 2 or 3 show `source` at `now`: the zone's offset minus clock 1's,
/// normalised into `-720`–`+720` minutes by whole days (docs/protocol.md, "USB vendor commands").
///
/// An offset with seconds (some zones before 1900) is rounded down to the minute.
///
/// # Errors
///
/// [`ClockError::Value`] if the normalised offset could not be carried by a command, which the
/// normalisation rules out.
pub fn clock_offset(
    now: Timestamp,
    local: &TimeZone,
    clock1_source: Clock1Source,
    source: &ZoneSource,
) -> Result<ClockOffset, ClockError> {
    let clock1_seconds = clock1_zone(local, clock1_source).to_offset(now).seconds();
    let minutes = (source.offset_seconds(now) - clock1_seconds).div_euclid(60);
    Ok(ClockOffset::new(normalise(minutes)?)?)
}

/// The command that makes clock 2 or 3 show `source` at `now`, with the offset of
/// [`clock_offset`].
///
/// # Errors
///
/// [`ClockError::Value`] if the normalised offset could not be carried by the command, which the
/// normalisation rules out.
pub fn offset_command(
    now: Timestamp,
    local: &TimeZone,
    clock1_source: Clock1Source,
    clock: OffsetClock,
    source: &ZoneSource,
    h24: bool,
) -> Result<Command, ClockError> {
    Ok(Command::ClockOffset {
        clock,
        offset: clock_offset(now, local, clock1_source, source)?,
        h24,
    })
}

/// A difference in minutes as the same time of day within `-719`–`+720`.
fn normalise(minutes: i32) -> Result<i16, ClockError> {
    let mut within_a_day = minutes.rem_euclid(DAY_MINUTES);
    if within_a_day > DAY_MINUTES / 2 {
        within_a_day -= DAY_MINUTES;
    }
    i16::try_from(within_a_day).map_err(|_| {
        RangeError::out_of_range(
            "clock offset",
            within_a_day,
            -ClockOffset::MAX_MINUTES,
            ClockOffset::MAX_MINUTES,
        )
        .into()
    })
}

/// The date fields for `now`: clock 1's date in `format`, the year as two digits.
///
/// # Errors
///
/// [`ClockError::Value`] if a field could not be carried by a command, which a valid date never
/// produces.
pub fn date_fields(
    now: Timestamp,
    local: &TimeZone,
    clock1_source: Clock1Source,
    format: DateFormat,
) -> Result<DateFields, ClockError> {
    let date = clock1_zone(local, clock1_source).to_datetime(now).date();
    let field = |name: &'static str, value: i16, min: i16, max: i16| {
        u8::try_from(value).map_err(|_| RangeError::out_of_range(name, value, min, max))
    };
    let year = field("year", date.year().rem_euclid(100), 0, 99)?;
    let month = field("month", i16::from(date.month()), 1, 12)?;
    let day = field("day", i16::from(date.day()), 1, 31)?;
    let (first, second, third) = match format {
        DateFormat::DdMmYy => (day, month, year),
        DateFormat::MmDdYy => (month, day, year),
        DateFormat::YyMmDd => (year, month, day),
    };
    Ok(DateFields::new(first, second, third)?)
}

/// The date command for `now`, with the fields of [`date_fields`].
///
/// # Errors
///
/// [`ClockError::Value`] if a field could not be carried by the command, which a valid date never
/// produces.
pub fn date_command(
    now: Timestamp,
    local: &TimeZone,
    clock1_source: Clock1Source,
    format: DateFormat,
) -> Result<Command, ClockError> {
    Ok(Command::Date(date_fields(
        now,
        local,
        clock1_source,
        format,
    )?))
}

/// When the clocks have to be sent again after `now`: the next minute boundary, or the next
/// daylight saving transition of clock 1's zone or of a zone in `sources` if that comes first.
///
/// # Errors
///
/// [`ClockError::OutOfRange`] when the next minute boundary is past the end of the timestamp
/// range.
pub fn next_resend(
    now: Timestamp,
    local: &TimeZone,
    clock1_source: Clock1Source,
    sources: &[&ZoneSource],
) -> Result<Timestamp, ClockError> {
    let next_minute = next_minute(now)?;
    let clock1 = clock1_zone(local, clock1_source);
    let zones = sources.iter().filter_map(|source| match source {
        ZoneSource::Zone(zone) => Some(zone),
        ZoneSource::Fixed(_) => None,
    });
    let transition = std::iter::once(&clock1)
        .chain(zones)
        .filter_map(|zone| {
            zone.following(now)
                .map(|transition| transition.timestamp())
                .find(|at| *at > now)
        })
        .min();
    Ok(transition.map_or(next_minute, |at| at.min(next_minute)))
}

/// The first whole minute after `now`.
fn next_minute(now: Timestamp) -> Result<Timestamp, ClockError> {
    // as_second rounds towards zero; a negative instant with a fraction is in the second before.
    let second = now.as_second() - i64::from(now.subsec_nanosecond() < 0);
    second
        .div_euclid(60)
        .checked_add(1)
        .and_then(|minute| minute.checked_mul(60))
        .and_then(|second| Timestamp::from_second(second).ok())
        .ok_or(ClockError::OutOfRange {
            what: "next minute boundary",
        })
}

#[cfg(test)]
mod tests;
