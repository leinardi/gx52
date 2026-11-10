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

//! Profiles: one full device state for one device type, stored as one TOML file each
//! (docs/profiles.md).
//!
//! Everything here is plain Rust. The functions that touch the disk block, so the app runs them
//! through `gio::spawn_blocking`; the tests call them directly on a temporary directory.

pub(crate) mod active;
pub(crate) mod editing;
pub(crate) mod editor;
pub(crate) mod exchange;
pub(crate) mod files;
pub(crate) mod leds;
pub(crate) mod legacy;
pub(crate) mod list;
pub(crate) mod mfd;
pub(crate) mod migrate;
pub(crate) mod name;
pub(crate) mod order;
pub(crate) mod removals;
pub(crate) mod reservations;
pub(crate) mod schema;
pub(crate) mod startup;
pub(crate) mod store;
pub(crate) mod writes;

use std::fmt;
use std::path::{Path, PathBuf};

use gettextrs::gettext;
use gtk::glib;
use jiff::tz::TimeZone;
use x52::charset::MfdText;
use x52::clock::{ClockError, FixedOffset, ZoneSource};
use x52::command::{Brightness, MfdLine};
use x52::device::DeviceType;
use x52::input::Mode;
use x52::state::{Clock1Settings, DeviceState, Leds, ZoneClockSettings};

use crate::config;

/// The number of MFD lines a profile holds.
pub(crate) const MFD_LINES: usize = MfdLine::ALL.len();

/// The most characters a custom text line keeps as typed (docs/profiles.md, "MFD lines").
///
/// Combining marks take no byte on the MFD, so the 16 converted bytes alone would let a line grow
/// without bound; this keeps a line, and the file holding it, small.
pub(crate) const MFD_TEXT_MAX_CHARS: usize = 64;

/// Why the text of a custom text line cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum LineTextError {
    /// More characters as typed than a line keeps.
    #[error("the text is {0} characters as typed, more than the {MFD_TEXT_MAX_CHARS} of a line")]
    TooManyCharacters(usize),
    /// More bytes after conversion than an MFD line holds.
    #[error(transparent)]
    TooLong(#[from] x52::charset::TextTooLong),
}

/// The MFD text of a custom text line: at most [`MFD_TEXT_MAX_CHARS`] characters as typed and 16
/// bytes after conversion.
///
/// # Errors
///
/// [`LineTextError`] when the text breaks either limit.
pub(crate) fn line_text(text: &str) -> Result<MfdText, LineTextError> {
    let chars = text.chars().count();
    if chars > MFD_TEXT_MAX_CHARS {
        return Err(LineTextError::TooManyCharacters(chars));
    }
    Ok(MfdText::new(text)?)
}

/// The id of a profile: the stem of its file name, stable for the profile's whole life.
///
/// Only lowercase ASCII letters, digits and `-`. The two default profiles have the reserved ids
/// `x52-pro-default` and `x52-default`; every other profile has a random lowercase UUID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ProfileId(String);

impl ProfileId {
    /// `text` as an id, or `None` when it has characters an id cannot have.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let valid = !text.is_empty()
            && text
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        valid.then(|| Self(text.to_owned()))
    }

    /// The reserved id of `device_type`'s default profile.
    pub(crate) fn default_for(device_type: DeviceType) -> Self {
        Self(format!("{}-default", device_type.id()))
    }

    /// A new random id, which can never be a reserved one.
    pub(crate) fn random() -> Self {
        Self(glib::uuid_string_random().to_lowercase())
    }

    /// The device type whose default profile this id names, if it is a reserved id.
    pub(crate) fn default_type(&self) -> Option<DeviceType> {
        DeviceType::ALL
            .into_iter()
            .find(|device_type| *self == Self::default_for(*device_type))
    }

    /// The id as text.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The name of this profile's file.
    pub(crate) fn file_name(&self) -> String {
        format!("{}.toml", self.0)
    }
}

impl fmt::Display for ProfileId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The device type written as `id` in a profile (`x52-pro` or `x52`).
pub(crate) fn device_type_from_id(id: &str) -> Option<DeviceType> {
    DeviceType::ALL
        .into_iter()
        .find(|device_type| device_type.id() == id)
}

/// What clock 2 or 3 of a profile shows: a named zone or a fixed offset from UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ZoneSpec {
    /// An IANA time zone the system's database knows.
    Iana(IanaZone),
    /// A fixed offset from UTC, with no daylight saving time.
    Fixed(FixedOffset),
}

/// An IANA time zone, looked up once in the system's database and kept with the name it was
/// written under, so the profile is written back as it was read and turning it into a device
/// state reads no file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IanaZone {
    name: String,
    zone: TimeZone,
}

impl IanaZone {
    /// The zone `name`, for example `Europe/Rome`, from the system's database.
    ///
    /// Reads the database, so it runs off the main thread (while parsing).
    ///
    /// # Errors
    ///
    /// [`ClockError::UnknownZone`] when the database has no zone of that name.
    pub(crate) fn new(name: &str) -> Result<Self, ClockError> {
        TimeZone::get(name)
            .map(|zone| Self {
                name: name.to_owned(),
                zone,
            })
            .map_err(|source| ClockError::UnknownZone {
                name: name.to_owned(),
                source,
            })
    }

    /// The name the zone was looked up by.
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// Clock 2 or clock 3 of a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZoneClock {
    /// What the clock shows.
    pub(crate) source: ZoneSpec,
    /// 24 h when true, 12 h otherwise.
    pub(crate) h24: bool,
}

/// What one MFD line shows (docs/profiles.md, "MFD lines").
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum MfdLineSource {
    /// Text as typed; it fits a line after conversion.
    Text(String),
    /// Nothing.
    #[default]
    Empty,
    /// The active profile's name.
    ProfileName,
    /// The most recently pressed button that is still held.
    LastButton,
}

/// What a profile may set differently in each mode: everything but the clocks and the date.
///
/// Fields a device type lacks (the LEDs, shift and blink of the X52) are kept at their built-in
/// values and never written to that type's file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModeSettings {
    /// The LED brightness.
    pub(crate) led_brightness: Brightness,
    /// The MFD backlight.
    pub(crate) mfd_brightness: Brightness,
    /// The SHIFT label; X52 Pro only.
    pub(crate) shift: bool,
    /// The i and POV 1 LEDs blink; X52 Pro only.
    pub(crate) blink: bool,
    /// The LEDs; X52 Pro only.
    pub(crate) leds: Leds,
    /// What each MFD line shows.
    pub(crate) mfd_lines: [MfdLineSource; MFD_LINES],
}

impl ModeSettings {
    /// The built-in mode settings of `device_type` (docs/profiles.md, "Defaults").
    pub(crate) fn builtin(device_type: DeviceType) -> Self {
        let state = DeviceState::default_for(device_type);
        Self {
            led_brightness: state.led_brightness,
            mfd_brightness: state.mfd_brightness,
            shift: state.shift,
            blink: state.blink,
            leds: state.leds,
            mfd_lines: Default::default(),
        }
    }
}

/// Whether a profile has one set of mode settings or one per mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProfileKind {
    /// One set; the mode selector changes nothing.
    Single,
    /// One set per position of the mode selector.
    PerMode,
}

/// The mode settings of a profile, one set or three.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ModeSets {
    /// One set for every mode.
    Single(ModeSettings),
    /// The sets of modes 1, 2 and 3, in that order.
    PerMode(Box<[ModeSettings; 3]>),
}

impl ModeSets {
    /// The kind these sets make.
    pub(crate) const fn kind(&self) -> ProfileKind {
        match self {
            Self::Single(_) => ProfileKind::Single,
            Self::PerMode(_) => ProfileKind::PerMode,
        }
    }

    /// The set the device shows in `mode`.
    pub(crate) fn for_mode(&self, mode: Mode) -> &ModeSettings {
        match self {
            Self::Single(settings) => settings,
            Self::PerMode(sets) => &sets[mode_index(mode)],
        }
    }

    /// The set the device shows in `mode`, to edit.
    pub(crate) fn for_mode_mut(&mut self, mode: Mode) -> &mut ModeSettings {
        match self {
            Self::Single(settings) => settings,
            Self::PerMode(sets) => &mut sets[mode_index(mode)],
        }
    }

    /// Per-mode sets: a single set is copied into all three modes.
    pub(crate) fn into_per_mode(self) -> Self {
        match self {
            Self::Single(settings) => {
                Self::PerMode(Box::new([settings.clone(), settings.clone(), settings]))
            }
            per_mode @ Self::PerMode(_) => per_mode,
        }
    }

    /// A single set: per-mode sets keep mode 1 and drop modes 2 and 3.
    pub(crate) fn into_single(self) -> Self {
        match self {
            single @ Self::Single(_) => single,
            Self::PerMode(sets) => {
                let [first, _, _] = *sets;
                Self::Single(first)
            }
        }
    }
}

/// The index of `mode` in a per-mode array.
pub(crate) const fn mode_index(mode: Mode) -> usize {
    match mode {
        Mode::One => 0,
        Mode::Two => 1,
        Mode::Three => 2,
    }
}

/// The values the dynamic MFD lines show, as the app knows them right now.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LiveValues {
    /// The name of the most recently pressed button still held, if any.
    pub(crate) last_button: Option<String>,
}

/// The name of `mode`, translated: "Mode 2".
pub(crate) fn mode_text(mode: Mode) -> String {
    match mode {
        Mode::One => gettext("Mode 1"),
        Mode::Two => gettext("Mode 2"),
        Mode::Three => gettext("Mode 3"),
    }
}

/// One profile: everything the device shows, for one device type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Profile {
    /// The device type the profile is for.
    pub(crate) device_type: DeviceType,
    /// The name shown in the UI: 1–64 characters, trimmed, unique within the device type.
    pub(crate) name: String,
    /// The field order of the date.
    pub(crate) date_format: x52::clock::DateFormat,
    /// Clock 1.
    pub(crate) clock1: Clock1Settings,
    /// Clock 2.
    pub(crate) clock2: ZoneClock,
    /// Clock 3.
    pub(crate) clock3: ZoneClock,
    /// The mode settings, one set or one per mode.
    pub(crate) settings: ModeSets,
}

impl Profile {
    /// The built-in values of `device_type`'s default profile (docs/profiles.md, "Defaults"),
    /// named "Default".
    pub(crate) fn builtin_default(device_type: DeviceType) -> Self {
        Self::builtin(device_type, gettext("Default"))
    }

    /// The built-in values for `device_type` under `name`, as a single profile: the value a
    /// field falls back to when a file has a problem with it.
    pub(crate) fn builtin(device_type: DeviceType, name: String) -> Self {
        let state = DeviceState::default_for(device_type);
        Self {
            device_type,
            name,
            date_format: state.date_format,
            clock1: state.clock1,
            clock2: ZoneClock::from_settings(&state.clock2),
            clock3: ZoneClock::from_settings(&state.clock3),
            settings: ModeSets::Single(ModeSettings::builtin(device_type)),
        }
    }

    /// Whether the profile has one set of mode settings or three.
    pub(crate) const fn kind(&self) -> ProfileKind {
        self.settings.kind()
    }

    /// The mode settings the device shows in `mode`.
    pub(crate) fn settings_for(&self, mode: Mode) -> &ModeSettings {
        self.settings.for_mode(mode)
    }

    /// The desired device state of this profile in `mode`, with the dynamic MFD lines filled in
    /// from `live`.
    pub(crate) fn to_device_state(&self, mode: Mode, live: &LiveValues) -> DeviceState {
        let builtin = DeviceState::default_for(self.device_type);
        let settings = self.settings_for(mode);
        let mfd_lines = settings.mfd_lines.each_ref().map(|line| match line {
            MfdLineSource::Text(text) => line_text(text).unwrap_or_else(|error| {
                tracing::warn!(
                    "Profile {:?}: MFD line {text:?} not shown: {error}",
                    self.name
                );
                MfdText::EMPTY
            }),
            MfdLineSource::Empty => MfdText::EMPTY,
            MfdLineSource::ProfileName => MfdText::fit(&self.name),
            MfdLineSource::LastButton => live
                .last_button
                .as_deref()
                .map_or(MfdText::EMPTY, MfdText::fit),
        });
        DeviceState {
            mfd_brightness: settings.mfd_brightness,
            led_brightness: settings.led_brightness,
            leds: if self.device_type.has_leds() {
                settings.leds
            } else {
                builtin.leds
            },
            mfd_lines,
            clock1: self.clock1,
            clock2: self.clock2.to_settings(),
            clock3: self.clock3.to_settings(),
            date_format: self.date_format,
            shift: self.device_type.has_shift() && settings.shift,
            blink: self.device_type.has_blink() && settings.blink,
        }
    }
}

impl ZoneClock {
    /// The profile form of a device-state clock.
    fn from_settings(settings: &ZoneClockSettings) -> Self {
        let source = match &settings.source {
            ZoneSource::Fixed(offset) => ZoneSpec::Fixed(*offset),
            ZoneSource::Zone(zone) => ZoneSpec::Iana(IanaZone {
                name: zone.iana_name().unwrap_or("UTC").to_owned(),
                zone: zone.clone(),
            }),
        };
        Self {
            source,
            h24: settings.h24,
        }
    }

    /// The device-state form.
    fn to_settings(&self) -> ZoneClockSettings {
        let source = match &self.source {
            ZoneSpec::Fixed(offset) => ZoneSource::Fixed(*offset),
            ZoneSpec::Iana(iana) => ZoneSource::Zone(iana.zone.clone()),
        };
        ZoneClockSettings {
            source,
            h24: self.h24,
        }
    }
}

/// The directory of the app's own files: `gx52`, or `gx52-devel` for a Devel build so a host
/// Devel build never shares a production build's profiles (docs/profiles.md, "Files").
pub(crate) fn config_dir() -> PathBuf {
    let name = if config::is_devel() {
        "gx52-devel"
    } else {
        "gx52"
    };
    glib::user_config_dir().join(name)
}

/// The profile directory inside `config_dir`.
pub(crate) fn profiles_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("profiles")
}

#[cfg(test)]
mod tests;
