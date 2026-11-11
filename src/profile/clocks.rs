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

//! The clocks as the Clocks view shows them: the offset text, the fixed offset a clock gets when
//! it leaves its time zone, the zone kept for each profile and clock, and the guard that keeps a
//! late zone lookup away from a profile that is no longer shown (docs/ui.md, "Clocks view").
//!
//! Plain Rust: the widgets only apply what is computed here, so the rules are tested here.

use std::collections::BTreeMap;

use gettextrs::gettext;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use x52::clock::{DateFormat, FixedOffset};
use x52::command::OffsetClock;
use x52::state::Clock1Settings;

use x52::clock::ClockError;
use x52::state::DeviceState;

use super::mfd::{self, ClockDisplay, ShownClock};
use super::{IanaZone, Profile, ProfileId, ZoneClock};

/// The minus sign of an offset: U+2212, as GNOME writes it.
const MINUS: char = '\u{2212}';

/// The step of a fixed offset in the spin row, in minutes.
pub(crate) const OFFSET_STEP_MINUTES: i16 = 15;

/// The date formats, in the order the Clocks view lists them.
pub(crate) const DATE_FORMATS: [DateFormat; 3] =
    [DateFormat::DdMmYy, DateFormat::MmDdYy, DateFormat::YyMmDd];

/// The clocks of a profile, shared by every mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileClocks {
    /// The field order of the date.
    pub(crate) date_format: DateFormat,
    /// Clock 1.
    pub(crate) clock1: Clock1Settings,
    /// Clock 2.
    pub(crate) clock2: ZoneClock,
    /// Clock 3.
    pub(crate) clock3: ZoneClock,
}

impl ProfileClocks {
    /// The clocks of `profile`.
    pub(crate) fn of(profile: &Profile) -> Self {
        Self {
            date_format: profile.date_format,
            clock1: profile.clock1,
            clock2: profile.clock2.clone(),
            clock3: profile.clock3.clone(),
        }
    }
}

impl Profile {
    /// Clock 2 or 3, to change.
    pub(crate) const fn zone_clock_mut(&mut self, clock: OffsetClock) -> &mut ZoneClock {
        match clock {
            OffsetClock::Clock2 => &mut self.clock2,
            OffsetClock::Clock3 => &mut self.clock3,
        }
    }
}

/// The name of a date format, translated: `DD-MM-YY`.
pub(crate) fn date_format_label(format: DateFormat) -> String {
    match format {
        // Translators: a date format: day, month and year, two digits each.
        DateFormat::DdMmYy => gettext("DD-MM-YY"),
        // Translators: a date format: month, day and year, two digits each.
        DateFormat::MmDdYy => gettext("MM-DD-YY"),
        // Translators: a date format: year, month and day, two digits each.
        DateFormat::YyMmDd => gettext("YY-MM-DD"),
    }
}

/// The number of clock 2 or 3, which also names it in the UI.
pub(crate) const fn clock_number(clock: OffsetClock) -> u8 {
    match clock {
        OffsetClock::Clock2 => 2,
        OffsetClock::Clock3 => 3,
    }
}

/// `minutes` from UTC as the UI writes it: `UTC+05:45`, `UTC−03:30` (with U+2212), `UTC+00:00`.
pub(crate) fn offset_text(minutes: i32) -> String {
    let sign = if minutes < 0 { MINUS } else { '+' };
    let minutes = minutes.unsigned_abs();
    format!("UTC{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

/// The offset in minutes written in `text`, as typed into the offset row: `UTC+05:45`, `+5:45`,
/// `-3`, `−03:30`, `5`, with or without `UTC` and spaces; `None` for anything else. The range is
/// not checked here.
pub(crate) fn parse_offset(text: &str) -> Option<i32> {
    let text = text.trim();
    let text = text
        .get(..3)
        .filter(|prefix| prefix.eq_ignore_ascii_case("utc"))
        .map_or(text, |_| &text[3..])
        .trim_start();
    if text.is_empty() {
        return Some(0);
    }
    let (negative, rest) = match text.chars().next() {
        Some('-' | MINUS) => (true, &text[text.chars().next()?.len_utf8()..]),
        Some('+') => (false, &text[1..]),
        _ => (false, text),
    };
    let rest = rest.trim_start();
    let (hours, minutes) = match rest.split_once(':') {
        Some((hours, minutes)) => (hours, minutes),
        None => (rest, "0"),
    };
    let digits = |part: &str| {
        (!part.is_empty() && part.len() <= 2 && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse::<i32>().ok())
            .flatten()
    };
    let hours = digits(hours)?;
    let minutes = digits(minutes).filter(|minutes| *minutes < 60)?;
    let total = hours * 60 + minutes;
    Some(if negative { -total } else { total })
}

/// The fixed offset a clock gets when it leaves `zone` at `now`: the zone's offset then, rounded
/// to the nearest 15 minutes and kept within UTC−12:00 to UTC+14:00.
pub(crate) fn fixed_from_zone(zone: &TimeZone, now: Timestamp) -> Option<FixedOffset> {
    fixed_from_seconds(zone.to_offset(now).seconds())
}

/// [`fixed_from_zone`] for an offset of `seconds` from UTC.
fn fixed_from_seconds(seconds: i32) -> Option<FixedOffset> {
    let step = i32::from(OFFSET_STEP_MINUTES) * 60;
    let rounded = (seconds + step / 2).div_euclid(step) * i32::from(OFFSET_STEP_MINUTES);
    let clamped = rounded.clamp(
        i32::from(FixedOffset::MIN_MINUTES),
        i32::from(FixedOffset::MAX_MINUTES),
    );
    // Clamped to −720–840, so both always succeed.
    FixedOffset::new(i16::try_from(clamped).ok()?).ok()
}

/// The offset of `zone` from UTC at `now`, in whole minutes: what the UI writes next to a
/// zone ("UTC+09:00").
pub(crate) fn utc_offset_minutes(zone: &TimeZone, now: Timestamp) -> i32 {
    zone.to_offset(now).seconds().div_euclid(60)
}

/// What the Clocks view shows of the clocks at one moment: each clock's time as the MFD draws
/// it, and the date.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClocksPreview {
    /// Clocks 1, 2 and 3.
    pub(crate) clocks: [ClockDisplay; 3],
    /// The date, with dots: `09.11.26`.
    pub(crate) date: String,
}

impl ClocksPreview {
    /// The clocks of `state` at `now`, with `local` as the system time zone, exactly as the
    /// device shows them.
    ///
    /// # Errors
    ///
    /// [`ClockError`] when a clock cannot be computed at `now`, as for the device.
    pub(crate) fn of(
        state: &DeviceState,
        now: Timestamp,
        local: &TimeZone,
    ) -> Result<Self, ClockError> {
        let [one, two, three] = ShownClock::ALL;
        Ok(Self {
            clocks: [
                mfd::clock_display(state, one, now, local)?,
                mfd::clock_display(state, two, now, local)?,
                mfd::clock_display(state, three, now, local)?,
            ],
            date: mfd::date_text(state, now, local)?,
        })
    }

    /// The time of clock 1.
    pub(crate) const fn clock1(&self) -> &ClockDisplay {
        &self.clocks[0]
    }

    /// The time of clock 2 or 3.
    pub(crate) const fn zone_clock(&self, clock: OffsetClock) -> &ClockDisplay {
        match clock {
            OffsetClock::Clock2 => &self.clocks[1],
            OffsetClock::Clock3 => &self.clocks[2],
        }
    }
}

/// The clock of a profile a zone is picked for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ZoneTarget {
    /// The profile.
    pub(crate) profile: ProfileId,
    /// The clock.
    pub(crate) clock: OffsetClock,
}

/// The time zones clocks 2 and 3 left for a fixed offset this session, by profile and clock, so
/// switching back to a time zone brings that zone back in the same profile and clock, and
/// nowhere else.
#[derive(Debug, Default)]
pub(crate) struct ZoneMemory {
    zones: BTreeMap<ZoneTarget, IanaZone>,
}

impl ZoneMemory {
    /// Keeps `zone` for `target`: the clock leaves it, or it was just picked.
    pub(crate) fn remember(&mut self, target: ZoneTarget, zone: IanaZone) {
        self.zones.insert(target, zone);
    }

    /// The zone kept for `target`, if any.
    pub(crate) fn recall(&self, target: &ZoneTarget) -> Option<&IanaZone> {
        self.zones.get(target)
    }
}

/// One zone pick of the search dialog: the clock it is for, and the dialog's generation when it
/// started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZoneRequest {
    /// The clock the zone is for.
    pub(crate) target: ZoneTarget,
    generation: u64,
}

/// The zone picks of a search dialog. A pick looks the zone up off the main thread; by the time
/// it completes the dialog may be closed, another profile shown, or another zone picked, and the
/// pick is then dropped instead of being applied to whatever is shown.
#[derive(Debug, Default)]
pub(crate) struct ZoneRequests {
    generation: u64,
    open: Option<ZoneTarget>,
}

impl ZoneRequests {
    /// The dialog opens for `target`; any pick still running is dropped.
    pub(crate) fn open(&mut self, target: ZoneTarget) {
        self.generation += 1;
        self.open = Some(target);
    }

    /// The dialog closes; any pick still running is dropped.
    pub(crate) fn close(&mut self) {
        self.generation += 1;
        self.open = None;
    }

    /// The clock the dialog is open for.
    pub(crate) const fn target(&self) -> Option<&ZoneTarget> {
        self.open.as_ref()
    }

    /// A zone is picked: the request to check when its lookup completes. A pick before it is
    /// dropped. `None` while the dialog is closed.
    pub(crate) fn pick(&mut self) -> Option<ZoneRequest> {
        let target = self.open.clone()?;
        self.generation += 1;
        Some(ZoneRequest {
            target,
            generation: self.generation,
        })
    }

    /// Whether `request` is still the dialog's latest pick, for the clock it is open for.
    pub(crate) fn is_current(&self, request: &ZoneRequest) -> bool {
        request.generation == self.generation && self.open.as_ref() == Some(&request.target)
    }
}

#[cfg(test)]
mod tests;
