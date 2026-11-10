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
use x52::clock::{self, ClockError};
use x52::command::{Brightness, ClockTime};
use x52::device::DeviceType;
use x52::input::Mode;
use x52::state::DeviceState;

use super::{MFD_LINES, MfdLineSource, ProfileId, ProfileKind};

/// Minutes in a day.
const DAY_MINUTES: i32 = 1440;

/// How far past the converted text a prefix of an insertion is still tried: composition with the
/// text around the insertion point (a Hangul syllable built from three jamo) saves at most this
/// many bytes, so a longer prefix can never fit.
const COMPOSITION_SLACK: usize = 2;

/// The longest prefix of `inserted`, in whole characters, that keeps the line within the 16 bytes
/// of the MFD when typed between `before` and `after`.
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
    let ends = inserted
        .char_indices()
        .map(|(index, _)| index)
        .skip(1)
        .chain(std::iter::once(inserted.len()));
    let mut accepted = 0;
    for end in ends {
        let prefix = &inserted[..end];
        if charset::encode(&format!("{before}{prefix}{after}")).len() <= LINE_BYTES {
            accepted = end;
        } else if charset::encode(&format!("{before}{prefix}")).len()
            > LINE_BYTES + COMPOSITION_SLACK
        {
            break;
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
    /// The three lines, 16 characters each, as the MFD draws them.
    pub(crate) lines: [String; MFD_LINES],
    /// The time of the clock shown.
    pub(crate) clock: String,
    /// The number of the clock shown when it is not clock 1.
    pub(crate) clock_tag: Option<u8>,
    /// The date, with dots.
    pub(crate) date: String,
    /// Whether SHIFT is shown.
    pub(crate) shift: bool,
    /// How lit the display is.
    pub(crate) backlight: Backlight,
    /// The clock shown.
    shown_clock: ShownClock,
}

/// What the MFD of a `device_type` device shows for `state` at `now`, with `local` as the system
/// time zone and `shown_clock` in the clock area.
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
    shown_clock: ShownClock,
    now: Timestamp,
    local: &TimeZone,
) -> Result<MfdPreview, ClockError> {
    let clock1_source = state.clock1.source;
    let clock1 = clock::clock1_time(now, local, clock1_source)?;
    let (offset, h24) = match shown_clock {
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
    let date = clock::date_fields(now, local, clock1_source, state.date_format)?;
    Ok(MfdPreview {
        lines: state
            .mfd_lines
            .map(|line| format!("{:<width$}", line.preview(), width = LINE_BYTES)),
        clock: clock_text(clock1, offset, h24),
        clock_tag: (shown_clock != ShownClock::One).then_some(shown_clock.number()),
        date: format!(
            "{:02}.{:02}.{:02}",
            date.first(),
            date.second(),
            date.third()
        ),
        shift: device_type.has_shift() && state.shift,
        backlight: Backlight::of(state.mfd_brightness),
        shown_clock,
    })
}

/// The time `offset` minutes from `clock1`, as the MFD draws it: `13:05` in 24 h, `1:05 PM` in
/// 12 h.
fn clock_text(clock1: ClockTime, offset: i16, h24: bool) -> String {
    let minutes = (i32::from(clock1.hour()) * 60 + i32::from(clock1.minute()) + i32::from(offset))
        .rem_euclid(DAY_MINUTES);
    let (hour, minute) = (minutes / 60, minutes % 60);
    if h24 {
        format!("{hour:02}:{minute:02}")
    } else {
        let half = if hour < 12 { "AM" } else { "PM" };
        let hour = match hour % 12 {
            0 => 12,
            hour => hour,
        };
        format!("{hour}:{minute:02} {half}")
    }
}

/// The accessible label of the display: "Simulated MFD: GX52, Fire, blank; clock 1 12:45, date
/// 09.11.26".
pub(crate) fn accessible_label(preview: &MfdPreview) -> String {
    let lines: Vec<String> = preview
        .lines
        .iter()
        .map(|line| match line.trim() {
            "" => gettext("blank"),
            text => text.to_owned(),
        })
        .collect();
    // Translators: {lines} lists the three lines of the joystick's display, {number} is the clock
    // shown (1, 2 or 3), {time} its time and {date} the date.
    gettext("Simulated MFD: {lines}; clock {number} {time}, date {date}")
        .replace("{lines}", &lines.join(", "))
        .replace("{number}", &preview.shown_clock.number().to_string())
        .replace("{time}", &preview.clock)
        .replace("{date}", &preview.date)
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
