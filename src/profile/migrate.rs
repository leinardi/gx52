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

//! Schema versions and the migration steps between them (docs/profiles.md, "Migration policy").
//!
//! Each step turns a parsed version N document into version N + 1, in memory. Nothing ever
//! migrates down: a file of a newer version is listed read-only.

use toml::Table;

/// One migration step: a version N document becomes version N + 1.
pub(crate) type Step = fn(&mut Table);

/// A schema version the app can write, and the steps that bring every older version up to it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Schema {
    /// The version the app reads natively and writes.
    supported: i64,
    /// `steps[i]` migrates version `i + 1` to `i + 2`; there are `supported - 1` of them.
    steps: &'static [Step],
}

/// What a document's version means for this app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Version {
    /// The supported version, or an older one that has been migrated up to it.
    Readable {
        /// The version the file has, when older than the supported one.
        migrated_from: Option<i64>,
    },
    /// Made by a newer app.
    Newer(i64),
}

impl Schema {
    /// The schema this app reads and writes.
    pub(crate) const CURRENT: Self = Self {
        supported: 1,
        steps: &[],
    };

    /// A schema with its migration steps.
    ///
    /// Only the tests build other schemas, to cover the migration path before schema 2 exists.
    #[cfg(test)]
    pub(crate) const fn new(supported: i64, steps: &'static [Step]) -> Self {
        Self { supported, steps }
    }

    /// The version the app writes.
    pub(crate) const fn supported(self) -> i64 {
        self.supported
    }

    /// Brings `document`, of `version` (at least 1), up to the supported version.
    pub(crate) fn migrate(self, document: &mut Table, version: i64) -> Version {
        if version > self.supported {
            return Version::Newer(version);
        }
        let first = usize::try_from(version.saturating_sub(1)).unwrap_or(0);
        for step in self.steps.iter().skip(first) {
            step(document);
        }
        Version::Readable {
            migrated_from: (version < self.supported).then_some(version),
        }
    }
}

#[cfg(test)]
mod tests;
