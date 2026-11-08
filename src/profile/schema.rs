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

//! The TOML schema of a profile file (docs/profiles.md, "TOML schema (version 1)").
//!
//! Reading takes two steps, so one bad field does not lose the whole profile: the text is parsed
//! into a [`Table`], `schema_version` and `device_type` are read first, then every field is
//! converted on its own. A field with a problem gets its built-in value and the problem is listed.
//! Writing produces the layout of docs/profiles.md, with every key the device type has.

use std::fmt::{self, Write as _};

use gettextrs::gettext;
use toml::{Table, Value};
use x52::charset::MfdText;
use x52::clock::{Clock1Source, DateFormat, FixedOffset};
use x52::command::Brightness;
use x52::device::{BiColourLed, DeviceType, LedColour, SingleColourLed};

use super::migrate::{Schema, Version};
use super::name::{self, NameProblem};
use super::{IanaZone, MFD_LINES, Profile, ZoneClock, ZoneSpec, device_type_from_id};

/// Why a document is not a profile at all.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ParseError {
    /// The bytes are not UTF-8.
    #[error("the file is not UTF-8 text")]
    NotUtf8,
    /// The text is not TOML.
    #[error("the file is not valid TOML: {0}")]
    Malformed(String),
    /// `schema_version` is missing.
    #[error("schema_version is missing")]
    MissingSchemaVersion,
    /// `schema_version` is not a version number (an integer from 1).
    #[error("schema_version {0} is not a schema version")]
    WrongSchemaVersion(String),
    /// `device_type` is missing.
    #[error("device_type is missing")]
    MissingDeviceType,
    /// `device_type` names no supported device type.
    #[error("device_type {0} is not a supported device type")]
    UnknownDeviceType(String),
}

/// A document that could be read, completely or in part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Parsed {
    /// A profile of the supported schema version, or migrated up to it.
    Readable {
        /// The profile, with built-in values for the fields that had problems.
        profile: Box<Profile>,
        /// What was wrong, field by field.
        problems: Vec<FieldProblem>,
        /// The file's version, when it was older and has been migrated.
        migrated_from: Option<i64>,
    },
    /// Made by a newer app: only listed, never edited, applied or written.
    Newer {
        /// The file's schema version.
        version: i64,
        /// The device type the file is for.
        device_type: DeviceType,
        /// The name, when it can be read.
        name: Option<String>,
    },
}

/// One field of a document that could not be used as it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FieldProblem {
    /// The key, with its table: `led_brightness`, `leds.a`, `clock_2.source`, `mfd.lines[1]`.
    pub(crate) key: String,
    /// What is wrong with it.
    pub(crate) kind: ProblemKind,
}

/// What is wrong with a field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProblemKind {
    /// The key is missing; the built-in value is used.
    Missing,
    /// The value has the wrong TOML type; the built-in value is used.
    WrongType {
        /// The type the key needs.
        expected: &'static str,
    },
    /// The value is outside its range; the built-in value is used.
    OutOfRange(String),
    /// The value is none of the allowed ones; the built-in value is used.
    UnknownValue(String),
    /// The name cannot be used; the built-in name is used.
    Name(NameProblem),
    /// The key is not in the schema; it is ignored.
    UnknownKey,
    /// The key exists, but not for this device type; it is ignored.
    NotForDeviceType,
    /// Another profile of the type has the same name; this one is shown under `to`.
    DuplicateName {
        /// The name the profile is shown under.
        to: String,
    },
}

impl fmt::Display for FieldProblem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = &self.key;
        match &self.kind {
            ProblemKind::Missing => write!(formatter, "{key} is missing"),
            ProblemKind::WrongType { expected } => write!(formatter, "{key} is not {expected}"),
            ProblemKind::OutOfRange(detail) => write!(formatter, "{key} is out of range: {detail}"),
            ProblemKind::UnknownValue(value) => {
                write!(formatter, "{key} has an unknown value {value}")
            }
            ProblemKind::Name(problem) => write!(formatter, "{key}: {problem}"),
            ProblemKind::UnknownKey => write!(formatter, "{key} is not a profile key"),
            ProblemKind::NotForDeviceType => {
                write!(formatter, "{key} does not apply to this device type")
            }
            ProblemKind::DuplicateName { to } => {
                write!(
                    formatter,
                    "{key} is used by another profile; shown as {to:?}"
                )
            }
        }
    }
}

/// The keys of the top-level table, in the order they are written.
const TOP_LEVEL_KEYS: [&str; 13] = [
    "schema_version",
    "device_type",
    "name",
    "led_brightness",
    "mfd_brightness",
    "shift",
    "blink",
    "date_format",
    "leds",
    "mfd",
    "clock_1",
    "clock_2",
    "clock_3",
];

/// Parses `bytes` as a profile of `schema`, migrating an older version in memory.
///
/// # Errors
///
/// [`ParseError`] when the bytes are not a profile at all: not UTF-8, not TOML, or without a valid
/// `schema_version` and `device_type`. Every other problem is a [`FieldProblem`].
pub(crate) fn parse(bytes: &[u8], schema: Schema) -> Result<Parsed, ParseError> {
    let text = std::str::from_utf8(bytes).map_err(|_| ParseError::NotUtf8)?;
    let mut document: Table = text
        .parse()
        .map_err(|error: toml::de::Error| ParseError::Malformed(error.message().to_owned()))?;
    let version = match document.get("schema_version") {
        None => return Err(ParseError::MissingSchemaVersion),
        Some(Value::Integer(version)) if *version >= 1 => *version,
        Some(other) => return Err(ParseError::WrongSchemaVersion(other.to_string())),
    };
    let device_type = match document.get("device_type") {
        None => return Err(ParseError::MissingDeviceType),
        Some(Value::String(id)) => {
            device_type_from_id(id).ok_or_else(|| ParseError::UnknownDeviceType(id.clone()))?
        }
        Some(other) => return Err(ParseError::UnknownDeviceType(other.to_string())),
    };
    match schema.migrate(&mut document, version) {
        Version::Newer(version) => Ok(Parsed::Newer {
            version,
            device_type,
            name: document
                .get("name")
                .and_then(Value::as_str)
                .and_then(|raw| name::normalise(raw).ok()),
        }),
        Version::Readable { migrated_from } => {
            let mut reader = Reader::default();
            let profile = reader.profile(&document, device_type);
            Ok(Parsed::Readable {
                profile: Box::new(profile),
                problems: reader.problems,
                migrated_from,
            })
        }
    }
}

/// The name a profile is shown under when its file has no usable one.
pub(crate) fn fallback_name() -> String {
    gettext("Unnamed profile")
}

/// Converts the fields of a document, collecting the problems.
#[derive(Debug, Default)]
struct Reader {
    problems: Vec<FieldProblem>,
}

impl Reader {
    fn profile(&mut self, document: &Table, device_type: DeviceType) -> Profile {
        let mut profile = Profile::builtin(device_type, fallback_name());
        self.unknown_keys(document, "", &TOP_LEVEL_KEYS);
        if let Some(raw) = self.string(document, "", "name") {
            match name::normalise(raw) {
                Ok(name) => profile.name = name,
                Err(problem) => self.problem("name", ProblemKind::Name(problem)),
            }
        }
        for (key, field) in [
            ("led_brightness", &mut profile.led_brightness),
            ("mfd_brightness", &mut profile.mfd_brightness),
        ] {
            if let Some(level) = self.brightness(document, key) {
                *field = level;
            }
        }
        for (key, supported, field) in [
            ("shift", device_type.has_shift(), &mut profile.shift),
            ("blink", device_type.has_blink(), &mut profile.blink),
        ] {
            if supported {
                if let Some(value) = self.boolean(document, "", key) {
                    *field = value;
                }
            } else if document.contains_key(key) {
                self.problem(key, ProblemKind::NotForDeviceType);
            }
        }
        if let Some(raw) = self.string(document, "", "date_format") {
            match date_format_from(raw) {
                Some(format) => profile.date_format = format,
                None => self.problem("date_format", ProblemKind::UnknownValue(quoted(raw))),
            }
        }
        if device_type.has_leds() {
            self.leds(document, &mut profile);
        } else if document.contains_key("leds") {
            self.problem("leds", ProblemKind::NotForDeviceType);
        }
        self.mfd(document, &mut profile);
        self.clock1(document, &mut profile);
        for (key, clock) in [
            ("clock_2", &mut profile.clock2),
            ("clock_3", &mut profile.clock3),
        ] {
            self.zone_clock(document, key, clock);
        }
        profile
    }

    fn brightness(&mut self, table: &Table, key: &str) -> Option<Brightness> {
        let level = self.integer(table, "", key)?;
        let checked = u8::try_from(level)
            .ok()
            .and_then(|level| Brightness::new(level).ok());
        if checked.is_none() {
            self.problem(
                key,
                ProblemKind::OutOfRange(format!("{level} is not in 0–{}", Brightness::MAX)),
            );
        }
        checked
    }

    fn leds(&mut self, document: &Table, profile: &mut Profile) {
        let Some(leds) = self.table(document, "", "leds") else {
            return;
        };
        let known: Vec<&str> = SingleColourLed::ALL
            .iter()
            .map(|led| led.key())
            .chain(BiColourLed::ALL.iter().map(|led| led.key()))
            .collect();
        self.unknown_keys(leds, "leds", &known);
        for led in SingleColourLed::ALL {
            if let Some(raw) = self.string(leds, "leds", led.key()) {
                match switch_from(raw) {
                    Some(on) => profile.leds.set_on(led, on),
                    None => self.problem(
                        &join("leds", led.key()),
                        ProblemKind::UnknownValue(quoted(raw)),
                    ),
                }
            }
        }
        for led in BiColourLed::ALL {
            if let Some(raw) = self.string(leds, "leds", led.key()) {
                match colour_from(raw) {
                    Some(colour) => profile.leds.set_colour(led, colour),
                    None => self.problem(
                        &join("leds", led.key()),
                        ProblemKind::UnknownValue(quoted(raw)),
                    ),
                }
            }
        }
    }

    fn mfd(&mut self, document: &Table, profile: &mut Profile) {
        let Some(mfd) = self.table(document, "", "mfd") else {
            return;
        };
        self.unknown_keys(mfd, "mfd", &["lines"]);
        let Some(value) = self.value(mfd, "mfd", "lines") else {
            return;
        };
        let Some(lines) = value.as_array().filter(|lines| lines.len() == MFD_LINES) else {
            self.problem(
                "mfd.lines",
                ProblemKind::WrongType {
                    expected: "an array of 3 strings",
                },
            );
            return;
        };
        for (index, (line, field)) in lines.iter().zip(&mut profile.mfd_lines).enumerate() {
            let key = format!("mfd.lines[{index}]");
            let Some(text) = line.as_str() else {
                self.problem(
                    &key,
                    ProblemKind::WrongType {
                        expected: "a string",
                    },
                );
                continue;
            };
            match MfdText::new(text) {
                Ok(_) => text.clone_into(field),
                Err(error) => self.problem(&key, ProblemKind::OutOfRange(error.to_string())),
            }
        }
    }

    fn clock1(&mut self, document: &Table, profile: &mut Profile) {
        let Some(clock) = self.table(document, "", "clock_1") else {
            return;
        };
        self.unknown_keys(clock, "clock_1", &["source", "use_24h"]);
        if let Some(raw) = self.string(clock, "clock_1", "source") {
            match raw {
                "local" => profile.clock1.source = Clock1Source::Local,
                "utc" => profile.clock1.source = Clock1Source::Utc,
                _ => self.problem("clock_1.source", ProblemKind::UnknownValue(quoted(raw))),
            }
        }
        if let Some(h24) = self.boolean(clock, "clock_1", "use_24h") {
            profile.clock1.h24 = h24;
        }
    }

    fn zone_clock(&mut self, document: &Table, key: &str, clock: &mut ZoneClock) {
        let Some(table) = self.table(document, "", key) else {
            return;
        };
        self.unknown_keys(table, key, &["source", "use_24h"]);
        if let Some(value) = self.value(table, key, "source") {
            match zone_spec_from(value) {
                Ok(source) => clock.source = source,
                Err(kind) => self.problem(&join(key, "source"), kind),
            }
        }
        if let Some(h24) = self.boolean(table, key, "use_24h") {
            clock.h24 = h24;
        }
    }

    /// The value of `key`, or `None` with a "missing" problem.
    fn value<'doc>(&mut self, table: &'doc Table, prefix: &str, key: &str) -> Option<&'doc Value> {
        let value = table.get(key);
        if value.is_none() {
            self.problem(&join(prefix, key), ProblemKind::Missing);
        }
        value
    }

    fn typed<'doc, T>(
        &mut self,
        table: &'doc Table,
        prefix: &str,
        key: &str,
        expected: &'static str,
        convert: impl FnOnce(&'doc Value) -> Option<T>,
    ) -> Option<T> {
        let converted = convert(self.value(table, prefix, key)?);
        if converted.is_none() {
            self.problem(&join(prefix, key), ProblemKind::WrongType { expected });
        }
        converted
    }

    fn table<'doc>(&mut self, table: &'doc Table, prefix: &str, key: &str) -> Option<&'doc Table> {
        self.typed(table, prefix, key, "a table", Value::as_table)
    }

    fn string<'doc>(&mut self, table: &'doc Table, prefix: &str, key: &str) -> Option<&'doc str> {
        self.typed(table, prefix, key, "a string", Value::as_str)
    }

    fn integer(&mut self, table: &Table, prefix: &str, key: &str) -> Option<i64> {
        self.typed(table, prefix, key, "an integer", Value::as_integer)
    }

    fn boolean(&mut self, table: &Table, prefix: &str, key: &str) -> Option<bool> {
        self.typed(table, prefix, key, "a boolean", Value::as_bool)
    }

    /// Reports every key of `table` that is not in `known`.
    fn unknown_keys(&mut self, table: &Table, prefix: &str, known: &[&str]) {
        for key in table.keys() {
            if !known.contains(&key.as_str()) {
                self.problem(&join(prefix, key), ProblemKind::UnknownKey);
            }
        }
    }

    fn problem(&mut self, key: &str, kind: ProblemKind) {
        self.problems.push(FieldProblem {
            key: key.to_owned(),
            kind,
        });
    }
}

/// `key` inside `prefix`, as a dotted path.
fn join(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    }
}

/// A value as it appears in a problem report.
fn quoted(raw: &str) -> String {
    format!("{raw:?}")
}

/// A clock 2/3 source: `{ iana = "<zone>" }` or `{ fixed_offset_minutes = <n> }`.
fn zone_spec_from(value: &Value) -> Result<ZoneSpec, ProblemKind> {
    let unknown = || ProblemKind::UnknownValue(value.to_string());
    let table = value.as_table().ok_or_else(unknown)?;
    let mut entries = table.iter();
    let (Some((tag, inner)), None) = (entries.next(), entries.next()) else {
        return Err(unknown());
    };
    match (tag.as_str(), inner) {
        ("iana", Value::String(name)) => IanaZone::new(name)
            .map(ZoneSpec::Iana)
            .map_err(|_| ProblemKind::UnknownValue(quoted(name))),
        ("fixed_offset_minutes", Value::Integer(minutes)) => i16::try_from(*minutes)
            .ok()
            .and_then(|minutes| FixedOffset::new(minutes).ok())
            .map(ZoneSpec::Fixed)
            .ok_or_else(|| {
                ProblemKind::OutOfRange(format!(
                    "{minutes} is not in {}–{}",
                    FixedOffset::MIN_MINUTES,
                    FixedOffset::MAX_MINUTES
                ))
            }),
        _ => Err(unknown()),
    }
}

/// The text of a date format in a profile.
pub(crate) const fn date_format_text(format: DateFormat) -> &'static str {
    match format {
        DateFormat::DdMmYy => "dd-mm-yy",
        DateFormat::MmDdYy => "mm-dd-yy",
        DateFormat::YyMmDd => "yy-mm-dd",
    }
}

fn date_format_from(raw: &str) -> Option<DateFormat> {
    [DateFormat::DdMmYy, DateFormat::MmDdYy, DateFormat::YyMmDd]
        .into_iter()
        .find(|format| date_format_text(*format) == raw)
}

const fn switch_text(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn switch_from(raw: &str) -> Option<bool> {
    match raw {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

const fn colour_text(colour: LedColour) -> &'static str {
    match colour {
        LedColour::Off => "off",
        LedColour::Green => "green",
        LedColour::Red => "red",
        LedColour::Amber => "amber",
    }
}

fn colour_from(raw: &str) -> Option<LedColour> {
    [
        LedColour::Off,
        LedColour::Green,
        LedColour::Red,
        LedColour::Amber,
    ]
    .into_iter()
    .find(|colour| colour_text(*colour) == raw)
}

/// `text` as a TOML string, escaped.
fn toml_string(text: &str) -> String {
    Value::String(text.to_owned()).to_string()
}

/// The file of `profile`, in the current schema, with every key its device type has.
pub(crate) fn write(profile: &Profile) -> String {
    let device_type = profile.device_type;
    let mut out = String::new();
    // Writing to a String cannot fail.
    let _ = write_into(&mut out, profile, device_type);
    out
}

fn write_into(out: &mut String, profile: &Profile, device_type: DeviceType) -> fmt::Result {
    writeln!(out, "schema_version = {}", Schema::CURRENT.supported())?;
    writeln!(out, "device_type = {}", toml_string(device_type.id()))?;
    writeln!(out, "name = {}", toml_string(&profile.name))?;
    writeln!(out)?;
    writeln!(out, "led_brightness = {}", profile.led_brightness.level())?;
    writeln!(out, "mfd_brightness = {}", profile.mfd_brightness.level())?;
    if device_type.has_shift() {
        writeln!(out, "shift = {}", profile.shift)?;
    }
    if device_type.has_blink() {
        writeln!(out, "blink = {}", profile.blink)?;
    }
    writeln!(
        out,
        "date_format = {}",
        toml_string(date_format_text(profile.date_format))
    )?;
    if device_type.has_leds() {
        writeln!(out, "\n[leds]")?;
        for led in SingleColourLed::ALL {
            let state = switch_text(profile.leds.is_on(led));
            writeln!(out, "{} = {}", led.key(), toml_string(state))?;
        }
        for led in BiColourLed::ALL {
            let colour = colour_text(profile.leds.colour(led));
            writeln!(out, "{} = {}", led.key(), toml_string(colour))?;
        }
    }
    let lines: Vec<String> = profile
        .mfd_lines
        .iter()
        .map(|line| toml_string(line))
        .collect();
    writeln!(out, "\n[mfd]\nlines = [{}]", lines.join(", "))?;
    let clock1 = match profile.clock1.source {
        Clock1Source::Local => "local",
        Clock1Source::Utc => "utc",
    };
    writeln!(out, "\n[clock_1]\nsource = {}", toml_string(clock1))?;
    writeln!(out, "use_24h = {}", profile.clock1.h24)?;
    for (key, clock) in [("clock_2", &profile.clock2), ("clock_3", &profile.clock3)] {
        let source = match &clock.source {
            ZoneSpec::Iana(zone) => format!("{{ iana = {} }}", toml_string(zone.name())),
            ZoneSpec::Fixed(offset) => {
                format!("{{ fixed_offset_minutes = {} }}", offset.minutes())
            }
        };
        writeln!(out, "\n[{key}]\nsource = {source}\nuse_24h = {}", clock.h24)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
