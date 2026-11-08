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

//! What runs before the profiles are usable: the one-time legacy import when it is due, then the
//! store load (docs/architecture.md, "Persistence").
//!
//! It blocks, so the app runs it through `gio::spawn_blocking`; the settings are written back on
//! the main thread from the returned [`Startup`].

use std::path::Path;

use super::legacy::{self, DatabaseError, ImportReport, LegacySettings, ProfileWriter, WriteError};
use super::profiles_dir;
use super::store::{DefaultPersistence, LoadReport, ProfileStore};

/// What happened to the legacy import at this start.
#[derive(Debug)]
pub(crate) enum ImportOutcome {
    /// Not due: already done, or there is nothing to import.
    NotDue,
    /// Every row is imported; the import is done.
    Completed {
        /// What was imported.
        report: ImportReport,
        /// The settings to apply.
        settings: LegacySettings,
    },
    /// The database cannot be read; the import is done all the same, so a start never fails
    /// because of it.
    DatabaseFailed(DatabaseError),
    /// A file could not be written; the import is not done and resumes at the next start.
    WriteFailed(WriteError),
}

impl ImportOutcome {
    /// Whether the import is done after this start, so the flag is set.
    pub(crate) const fn is_done(&self) -> bool {
        match self {
            Self::Completed { .. } | Self::DatabaseFailed(_) => true,
            Self::NotDue | Self::WriteFailed(_) => false,
        }
    }
}

/// The profiles at the end of the start-up sequence.
#[derive(Debug)]
pub(crate) struct Startup {
    /// The loaded store.
    pub(crate) store: ProfileStore,
    /// What loading found.
    pub(crate) load_report: LoadReport,
    /// What the legacy import did.
    pub(crate) import: ImportOutcome,
}

/// Imports the GX52 0.7 database at `legacy_db` when the import is due, then loads the store of
/// `config_dir`.
///
/// The import is due while `import_done` is false and there is a database or a saved plan. A
/// missing default profile's file is written only once the import is no longer pending, so a
/// failed write of an imported default is retried instead of being replaced by the built-in one.
pub(crate) fn prepare_profiles(
    config_dir: &Path,
    legacy_db: &Path,
    import_done: bool,
    writer: &mut impl ProfileWriter,
) -> Startup {
    let dir = profiles_dir(config_dir);
    let saved = if import_done {
        None
    } else {
        legacy::saved_plan(config_dir)
    };
    let due = !import_done && (saved.is_some() || legacy_db.exists());
    let import = if due {
        import(config_dir, &dir, legacy_db, saved, writer)
    } else {
        ImportOutcome::NotDue
    };
    let persistence = if due && !import.is_done() {
        DefaultPersistence::MemoryOnly
    } else {
        DefaultPersistence::Write
    };
    let (store, load_report) = ProfileStore::load(&dir, persistence);
    Startup {
        store,
        load_report,
        import,
    }
}

fn import(
    config_dir: &Path,
    profiles_dir: &Path,
    legacy_db: &Path,
    saved: Option<legacy::Plan>,
    writer: &mut impl ProfileWriter,
) -> ImportOutcome {
    let plan_saved = saved.is_some();
    let plan = match saved {
        Some(plan) => plan,
        None => match legacy::read_database(legacy_db) {
            Ok(database) => {
                let (store, _) = ProfileStore::load(profiles_dir, DefaultPersistence::MemoryOnly);
                legacy::plan(&database, &store)
            }
            Err(error) => return ImportOutcome::DatabaseFailed(error),
        },
    };
    match legacy::execute(&plan, plan_saved, config_dir, profiles_dir, writer) {
        Ok(report) => ImportOutcome::Completed {
            report,
            settings: plan.settings,
        },
        Err(error) => ImportOutcome::WriteFailed(error),
    }
}

#[cfg(test)]
mod tests;
