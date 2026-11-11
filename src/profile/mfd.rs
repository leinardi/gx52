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

//! The MFD as the MFD editor shows it: the simulated display, the line sources, the text budget of
//! a line and the text kept while a line shows another source (docs/ui.md, "MFD view").
//!
//! Plain Rust: the widgets only apply what is computed here, so the rules are tested here.

use std::collections::BTreeMap;

use gettextrs::gettext;
use jiff::Timestamp;
use jiff::tz::TimeZone;
use x52::charset::{self, LINE_BYTES};
use x52::clock::{self, Clock1Source, ClockError, DateFormat};
use x52::command::{Brightness, ClockTime};
use x52::device::DeviceType;
use x52::input::Mode;
use x52::state::DeviceState;

use super::{MFD_LINES, MFD_TEXT_MAX_CHARS, MfdLineSource, ProfileId, ProfileKind};

/// Minutes in a day.
const DAY_MINUTES: i32 = 1440;

/// The longest prefix of `inserted`, in whole characters, that keeps the line within the 16 bytes
/// of the MFD and the [`MFD_TEXT_MAX_CHARS`] characters of a line when typed between `before` and
/// `after`.
///
/// `before` and `after` are the text on each side of the insertion point once any selection is
/// gone (GTK deletes a selection before it inserts its replacement). The text already in the line
/// is never cut: when nothing fits, nothing is accepted. A character that composes with its
/// neighbours (a combining accent after `e`) is accepted when the composed line fits.
pub(crate) fn accepted_insert<'text>(
    before: &str,
    inserted: &'text str,
    after: &str,
) -> &'text str {
    // Only prefixes within the character limit are tried, so a paste of any length (or of
    // combining marks, which take no byte) costs at most that many conversions of a short line.
    let room = MFD_TEXT_MAX_CHARS.saturating_sub(before.chars().count() + after.chars().count());
    let ends = inserted
        .char_indices()
        .map(|(index, _)| index)
        .skip(1)
        .chain(std::iter::once(inserted.len()))
        .take(room);
    let mut accepted = 0;
    for end in ends {
        let prefix = &inserted[..end];
        if charset::encode(&format!("{before}{prefix}{after}")).len() <= LINE_BYTES {
            accepted = end;
        }
    }
    &inserted[..accepted]
}

/// What an MFD line shows, as the editor's combo row lists it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum LineSource {
    /// Text typed in the editor.
    CustomText,
    /// The active profile's name.
    ProfileName,
    /// The button being held.
    LastButton,
    /// Nothing.
    Empty,
}

impl LineSource {
    /// Every source, in the order the combo row lists them.
    pub(crate) const ALL: [Self; 4] = [
        Self::CustomText,
        Self::ProfileName,
        Self::LastButton,
        Self::Empty,
    ];

    /// The name shown, translated.
    pub(crate) fn label(self) -> String {
        match self {
            Self::CustomText => gettext("Custom text"),
            Self::ProfileName => gettext("Profile name"),
            Self::LastButton => gettext("Last button"),
            Self::Empty => gettext("Empty"),
        }
    }

    /// The source of a profile's line.
    pub(crate) const fn of(source: &MfdLineSource) -> Self {
        match source {
            MfdLineSource::Text(_) => Self::CustomText,
            MfdLineSource::Empty => Self::Empty,
            MfdLineSource::ProfileName => Self::ProfileName,
            MfdLineSource::LastButton => Self::LastButton,
        }
    }

    /// The profile's form of this source; `text` gives the text of a custom text line.
    pub(crate) fn to_line(self, text: impl FnOnce() -> String) -> MfdLineSource {
        match self {
            Self::CustomText => MfdLineSource::Text(text()),
            Self::ProfileName => MfdLineSource::ProfileName,
            Self::LastButton => MfdLineSource::LastButton,
            Self::Empty => MfdLineSource::Empty,
        }
    }
}

/// What the row of a custom text line shows about its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextInfo {
    /// The bytes the text takes on the MFD.
    pub(crate) count: usize,
    /// The characters the MFD has no glyph for and what it shows instead: "ő → o, € → ?"; empty
    /// when there are none.
    pub(crate) substitutions: String,
}

impl TextInfo {
    /// What the row shows about `text`.
    pub(crate) fn of(text: &str) -> Self {
        let substitutions: Vec<String> = charset::substitutions(text)
            .into_iter()
            .map(|(character, shown)| format!("{character} → {shown}"))
            .collect();
        Self {
            count: charset::encode(text).len(),
            substitutions: substitutions.join(", "),
        }
    }

    /// The counter: "3/16".
    pub(crate) fn counter(&self) -> String {
        format!("{}/{LINE_BYTES}", self.count)
    }

    /// The counter as a screen reader reads it: "3 of 16 characters".
    pub(crate) fn counter_label(&self) -> String {
        // Translators: {count} is the number of characters of an MFD line used, {max} the most
        // the line holds.
        gettext("{count} of {max} characters")
            .replace("{count}", &self.count.to_string())
            .replace("{max}", &LINE_BYTES.to_string())
    }
}

/// The clock the simulated display shows. The app cannot read which clock the MFD shows, so this
/// is the user's choice, clock 1 by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) enum ShownClock {
    /// Clock 1.
    #[default]
    One,
    /// Clock 2.
    Two,
    /// Clock 3.
    Three,
}

impl ShownClock {
    /// Every clock, in order.
    pub(crate) const ALL: [Self; 3] = [Self::One, Self::Two, Self::Three];

    /// The clock's number, which also names its toggle.
    pub(crate) const fn number(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

/// How lit the display is, by the MFD brightness: four steps, as the prototype draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Backlight {
    /// Brightness `0`: the text dims.
    Off,
    /// Brightness up to 40.
    Low,
    /// Brightness up to 90.
    Medium,
    /// Brightness above 90.
    High,
}

impl Backlight {
    /// Every step.
    pub(crate) const ALL: [Self; 4] = [Self::Off, Self::Low, Self::Medium, Self::High];

    /// The step of `brightness`.
    pub(crate) const fn of(brightness: Brightness) -> Self {
        match brightness.level() {
            0 => Self::Off,
            1..=40 => Self::Low,
            41..=90 => Self::Medium,
            _ => Self::High,
        }
    }

    /// The style class of the step.
    pub(crate) const fn css_class(self) -> &'static str {
        match self {
            Self::Off => "backlight-0",
            Self::Low => "backlight-1",
            Self::Medium => "backlight-2",
            Self::High => "backlight-3",
        }
    }
}

/// What the simulated display shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MfdPreview {
    /// The mode the MODE area lights: the selector's position, which the device shows by itself;
    /// `None` while it is unknown or no device is connected.
    pub(crate) mode: Option<Mode>,
    /// Whether SHIFT is lit.
    pub(crate) shift: bool,
    /// The three lines, 16 characters each, as the MFD draws them.
    pub(crate) lines: [String; MFD_LINES],
    /// The time of the clock shown: `13:05`, or `1:05` in 12 h.
    pub(crate) time: String,
    /// `AM` or `PM` in 12 h, as the MFD draws it next to the time.
    pub(crate) half: Option<&'static str>,
    /// The date, with dots.
    pub(crate) date: String,
    /// How lit the display is.
    pub(crate) backlight: Backlight,
    /// The clock shown.
    pub(crate) shown_clock: ShownClock,
}

impl MfdPreview {
    /// What the date area shows for a moment after another clock is picked, as the device does
    /// when Start/Stop cycles the clocks: the clock's number, where the date was.
    pub(crate) fn clock_flash(&self) -> String {
        format!(
            "{:<width$}",
            format!(" {}", self.shown_clock.number()),
            width = self.date.chars().count()
        )
    }
}

/// What the MFD of a `device_type` device shows for `state` at `now`, with `local` as the system
/// time zone, `shown_clock` in the clock area and the mode selector at `selector`.
///
/// Built from the desired state the device gets, so the display shows exactly what is sent: the
/// converted lines, and the time of clock 2 or 3 as clock 1's plus the offset sent.
///
/// # Errors
///
/// [`ClockError`] when a clock value cannot be computed at `now`, as for the device.
pub(crate) fn preview(
    state: &DeviceState,
    device_type: DeviceType,
    selector: Option<Mode>,
    shown_clock: ShownClock,
    now: Timestamp,
    local: &TimeZone,
) -> Result<MfdPreview, ClockError> {
    let ClockDisplay { time, half } = clock_display(state, shown_clock, now, local)?;
    let date = date_text(state, now, local)?;
    Ok(MfdPreview {
        mode: selector,
        shift: device_type.has_shift() && state.shift,
        lines: state
            .mfd_lines
            .map(|line| format!("{:<width$}", line.preview(), width = LINE_BYTES)),
        time,
        half,
        date,
        backlight: Backlight::of(state.mfd_brightness),
        shown_clock,
    })
}

/// The time of one clock as the MFD draws it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClockDisplay {
    /// `13:05`, or `1:05` in 12 h.
    pub(crate) time: String,
    /// `AM` or `PM` in 12 h, as the MFD draws it next to the time.
    pub(crate) half: Option<&'static str>,
}

impl ClockDisplay {
    /// The time with its half of the day: `13:05`, `1:05 PM`.
    pub(crate) fn text(&self) -> String {
        match self.half {
            Some(half) => format!("{} {half}", self.time),
            None => self.time.clone(),
        }
    }
}

/// The time `clock` of `state` shows at `now`, with `local` as the system time zone: for clock 2
/// or 3, clock 1's time plus the offset sent, exactly as the device shows it.
///
/// # Errors
///
/// [`ClockError`] when the clock cannot be computed at `now`, as for the device.
pub(crate) fn clock_display(
    state: &DeviceState,
    clock: ShownClock,
    now: Timestamp,
    local: &TimeZone,
) -> Result<ClockDisplay, ClockError> {
    let clock1_source = state.clock1.source;
    let clock1 = clock::clock1_time(now, local, clock1_source)?;
    let (offset, h24) = match clock {
        ShownClock::One => (0, state.clock1.h24),
        ShownClock::Two => (
            clock::clock_offset(now, local, clock1_source, &state.clock2.source)?.minutes(),
            state.clock2.h24,
        ),
        ShownClock::Three => (
            clock::clock_offset(now, local, clock1_source, &state.clock3.source)?.minutes(),
            state.clock3.h24,
        ),
    };
    let (time, half) = clock_text(clock1, offset, h24);
    Ok(ClockDisplay { time, half })
}

/// The date `state` shows at `now` with `local` as the system time zone, with dots, as the MFD
/// draws it: `09.11.26`.
///
/// # Errors
///
/// [`ClockError`] when the date cannot be computed at `now`, as for the device.
pub(crate) fn date_text(
    state: &DeviceState,
    now: Timestamp,
    local: &TimeZone,
) -> Result<String, ClockError> {
    format_date(state.date_format, state.clock1.source, now, local)
}

/// The date in `format` at `now` in clock 1's zone (`clock1_source`, `local` being the system
/// time zone), with dots: `09.11.26`.
///
/// # Errors
///
/// [`ClockError`] when the date cannot be computed at `now`, as for the device.
pub(crate) fn format_date(
    format: DateFormat,
    clock1_source: Clock1Source,
    now: Timestamp,
    local: &TimeZone,
) -> Result<String, ClockError> {
    let date = clock::date_fields(now, local, clock1_source, format)?;
    Ok(format!(
        "{:02}.{:02}.{:02}",
        date.first(),
        date.second(),
        date.third()
    ))
}

/// The time `offset` minutes from `clock1`, as the MFD draws it: `13:05` in 24 h, `1:05` and
/// `PM` in 12 h.
fn clock_text(clock1: ClockTime, offset: i16, h24: bool) -> (String, Option<&'static str>) {
    let minutes = (i32::from(clock1.hour()) * 60 + i32::from(clock1.minute()) + i32::from(offset))
        .rem_euclid(DAY_MINUTES);
    let (hour, minute) = (minutes / 60, minutes % 60);
    if h24 {
        (format!("{hour:02}:{minute:02}"), None)
    } else {
        let half = if hour < 12 { "AM" } else { "PM" };
        let hour = match hour % 12 {
            0 => 12,
            hour => hour,
        };
        (format!("{hour}:{minute:02}"), Some(half))
    }
}

/// The accessible label of the display: "Simulated MFD: mode 2, SHIFT on; GX52, Fire, blank;
/// clock 1 12:45, date 09.11.26".
pub(crate) fn accessible_label(preview: &MfdPreview) -> String {
    let lines: Vec<String> = preview
        .lines
        .iter()
        .map(|line| match line.trim() {
            "" => gettext("blank"),
            text => text.to_owned(),
        })
        .collect();
    let time = ClockDisplay {
        time: preview.time.clone(),
        half: preview.half,
    }
    .text();
    let mut indicators = Vec::new();
    if let Some(mode) = preview.mode {
        // Translators: the mode lit on the joystick's display; {number} is 1, 2 or 3.
        indicators.push(gettext("mode {number}").replace("{number}", &mode_number(mode)));
    }
    if preview.shift {
        indicators.push(gettext("SHIFT on"));
    }
    let template = if indicators.is_empty() {
        // Translators: {lines} lists the three lines of the joystick's display, {number} is the
        // clock shown (1, 2 or 3), {time} its time and {date} the date.
        gettext("Simulated MFD: {lines}; clock {number} {time}, date {date}")
    } else {
        // Translators: {indicators} lists what the joystick's display lights ("mode 2, SHIFT
        // on"), {lines} its three lines, {number} the clock shown (1, 2 or 3), {time} its time
        // and {date} the date.
        gettext("Simulated MFD: {indicators}; {lines}; clock {number} {time}, date {date}")
    };
    fill(
        &template,
        &[
            ("indicators", &indicators.join(", ")),
            ("lines", &lines.join(", ")),
            ("number", &preview.shown_clock.number().to_string()),
            ("time", &time),
            ("date", &preview.date),
        ],
    )
}

/// `template` with each `{key}` of `values` replaced by its value, in one pass: a value is never
/// searched for placeholders, so text the user typed into a line stays as it is.
fn fill(template: &str, values: &[(&str, &str)]) -> String {
    let mut filled = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        filled.push_str(&rest[..start]);
        let placeholder = &rest[start + 1..];
        let found = values.iter().find(|(key, _)| {
            placeholder
                .strip_prefix(key)
                .is_some_and(|after| after.starts_with('}'))
        });
        if let Some((key, value)) = found {
            filled.push_str(value);
            rest = &placeholder[key.len() + 1..];
        } else {
            filled.push('{');
            rest = placeholder;
        }
    }
    filled.push_str(rest);
    filled
}

/// The number of `mode`, as the MODE area draws it.
pub(crate) fn mode_number(mode: Mode) -> String {
    (super::mode_index(mode) + 1).to_string()
}

/// Which set of mode settings of a profile is edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum SettingsSet {
    /// The one set of a single profile.
    Single,
    /// The set of a mode of a per-mode profile.
    Mode(Mode),
}

impl SettingsSet {
    /// The set a profile of `kind` shows in `mode`.
    pub(crate) const fn of(kind: ProfileKind, mode: Mode) -> Self {
        match kind {
            ProfileKind::Single => Self::Single,
            ProfileKind::PerMode => Self::Mode(mode),
        }
    }
}

/// The custom text line a draft is kept for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct DraftKey {
    /// The profile.
    pub(crate) profile: ProfileId,
    /// The set of mode settings.
    pub(crate) set: SettingsSet,
    /// The line, from `0`.
    pub(crate) line: usize,
}

/// The text of the custom text lines switched to another source this session, so switching back
/// brings the text back in the same profile, set and line, and nowhere else.
#[derive(Debug, Default)]
pub(crate) struct TextDrafts {
    texts: BTreeMap<DraftKey, String>,
}

impl TextDrafts {
    /// Keeps `text`: the line at `key` switches away from custom text.
    pub(crate) fn leave(&mut self, key: DraftKey, text: String) {
        if text.is_empty() {
            self.texts.remove(&key);
        } else {
            self.texts.insert(key, text);
        }
    }

    /// The text the line at `key` gets back when it switches to custom text: the text it had,
    /// or nothing.
    pub(crate) fn resume(&mut self, key: &DraftKey) -> String {
        self.texts.remove(key).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests;
