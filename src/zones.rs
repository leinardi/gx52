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

//! The time zones the zone search dialog lists, read from the tz database's `zone.tab` and
//! `iso3166.tab`, which the GNOME runtime ships (docs/ui.md, "Clocks view").
//!
//! Plain Rust. [`load`] reads files, so the app runs it through `gio::spawn_blocking`.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

use gettextrs::gettext;
use jiff::Timestamp;
use jiff::tz::TimeZone;

/// The directory of the system's tz database.
pub(crate) const SYSTEM_ZONEINFO: &str = "/usr/share/zoneinfo";

/// The IANA name of UTC, listed in addition to the zones of `zone.tab`.
const UTC: &str = "UTC";

/// The largest table file read, so a broken file cannot use up the memory; the real ones are
/// under 20 KiB.
const MAX_TABLE_BYTES: u64 = 1024 * 1024;

/// One zone of the search dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZoneEntry {
    /// The place that names the zone: `Tokyo`, `Buenos Aires`, or `Coordinated Universal Time`.
    pub(crate) city: String,
    /// The country, in English as the tz database names it; empty when unknown.
    pub(crate) country: String,
    /// The IANA name: `Asia/Tokyo`.
    pub(crate) iana: String,
}

impl ZoneEntry {
    /// The entry of a zone known only by its IANA name.
    fn from_name(iana: &str, country: &str) -> Self {
        Self {
            city: city_of(iana),
            country: country.to_owned(),
            iana: iana.to_owned(),
        }
    }

    /// Whether the entry matches what was typed in the search: a case-insensitive part of its
    /// city, country or IANA name. An empty search matches every entry.
    pub(crate) fn matches(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty()
            || [&self.city, &self.country, &self.iana]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
            || self.iana.replace('_', " ").to_lowercase().contains(&query)
    }
}

/// The place an IANA name names, for display: its last part, with spaces for underscores
/// (`America/Argentina/Buenos_Aires` is `Buenos Aires`).
pub(crate) fn city_of(iana: &str) -> String {
    if iana == UTC {
        return gettext("Coordinated Universal Time");
    }
    iana.rsplit('/').next().unwrap_or(iana).replace('_', " ")
}

/// The time in `entry`'s zone at `now`, `HH:MM`, and its offset from UTC in minutes; `None` for a
/// zone the database does not know.
///
/// Reads the database, so it runs off the main thread.
pub(crate) fn zone_time(entry: &ZoneEntry, now: Timestamp) -> Option<ZoneTime> {
    let zone = TimeZone::get(&entry.iana).ok()?;
    let zoned = now.to_zoned(zone);
    Some(ZoneTime {
        time: zoned.strftime("%H:%M").to_string(),
        offset_minutes: zoned.offset().seconds().div_euclid(60),
    })
}

/// The time of a zone at one moment, as the search dialog lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZoneTime {
    /// `HH:MM`, 24 h.
    pub(crate) time: String,
    /// The offset from UTC in minutes.
    pub(crate) offset_minutes: i32,
}

/// The country names of `iso3166.tab`, by two-letter code. Comments and lines without a code and
/// a name are skipped.
pub(crate) fn parse_iso3166(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (code, name) = line.split_once('\t')?;
            let (code, name) = (code.trim(), name.trim());
            (code.len() == 2 && !name.is_empty()).then(|| (code.to_owned(), name.to_owned()))
        })
        .collect()
}

/// The zones of `zone.tab`, with the country names of `countries` (empty for a code it lacks),
/// UTC added, sorted by city then country. Comments and lines without a code, coordinates and a
/// zone name are skipped; a zone listed twice is kept once.
pub(crate) fn parse_zone_tab(text: &str, countries: &HashMap<String, String>) -> Vec<ZoneEntry> {
    let mut entries: Vec<ZoneEntry> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let code = fields.next()?.trim();
            let _coordinates = fields.next()?;
            let iana = fields.next()?.trim();
            (code.len() == 2 && valid_name(iana)).then(|| {
                let country = countries.get(code).map_or("", String::as_str);
                ZoneEntry::from_name(iana, country)
            })
        })
        .collect();
    entries.push(ZoneEntry::from_name(UTC, ""));
    finish(entries)
}

/// The entries of `names` when `zone.tab` cannot be read: every IANA name the database knows,
/// with no country, sorted by city.
pub(crate) fn from_names(names: impl IntoIterator<Item = String>) -> Vec<ZoneEntry> {
    let entries = names
        .into_iter()
        .filter(|name| valid_name(name))
        .map(|name| ZoneEntry::from_name(&name, ""))
        .collect();
    finish(entries)
}

/// Whether `name` looks like an IANA zone name: no space, no empty part.
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(char::is_whitespace)
        && name.split('/').all(|part| !part.is_empty())
}

/// `entries` sorted by city, then country, then name, each name kept once.
fn finish(mut entries: Vec<ZoneEntry>) -> Vec<ZoneEntry> {
    entries.sort_by(|left, right| {
        (&left.city, &left.country, &left.iana).cmp(&(&right.city, &right.country, &right.iana))
    });
    let mut seen = std::collections::HashSet::new();
    entries.retain(|entry| seen.insert(entry.iana.clone()));
    entries
}

/// Reads a table file of at most [`MAX_TABLE_BYTES`].
fn read_table(path: &Path) -> io::Result<String> {
    let size = fs::metadata(path)?.len();
    if size > MAX_TABLE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{size} bytes is over the {MAX_TABLE_BYTES}-byte limit"),
        ));
    }
    fs::read_to_string(path)
}

/// The zones of the tz database in `dir`, from its `zone.tab` and `iso3166.tab`.
///
/// Without `iso3166.tab` the zones have no country. Without `zone.tab`, `fallback` gives the
/// zone names (jiff's database in the app) and no zone has a country.
///
/// Blocks on the file reads.
pub(crate) fn load_from(dir: &Path, fallback: impl FnOnce() -> Vec<String>) -> Vec<ZoneEntry> {
    let zone_tab = dir.join("zone.tab");
    let text = match read_table(&zone_tab) {
        Ok(text) => text,
        Err(error) => {
            tracing::warn!(
                "Cannot read {}: {error}; listing the zone names only",
                zone_tab.display()
            );
            return from_names(fallback());
        }
    };
    let iso3166 = dir.join("iso3166.tab");
    let countries = read_table(&iso3166).map_or_else(
        |error| {
            tracing::warn!(
                "Cannot read {}: {error}; listing zones without countries",
                iso3166.display()
            );
            HashMap::new()
        },
        |text| parse_iso3166(&text),
    );
    parse_zone_tab(&text, &countries)
}

/// The zones of the system's tz database, with jiff's list of names as the fallback.
///
/// Blocks on the file reads.
pub(crate) fn load() -> Vec<ZoneEntry> {
    load_from(Path::new(SYSTEM_ZONEINFO), || {
        jiff::tz::db()
            .available()
            .map(|name| name.as_str().to_owned())
            .collect()
    })
}

#[cfg(test)]
mod tests;
