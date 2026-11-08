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

//! The profile store: every profile file of the profile directory, read under the validation and
//! recovery rules of docs/profiles.md.
//!
//! The rule above all the others: a file is never overwritten or deleted because it could not be
//! read. Loading writes nothing but a missing default profile file; a profile is rewritten only
//! when it is saved.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use x52::device::DeviceType;

use super::files::{self, Created, PROFILE_LIMIT, ReadError};
use super::migrate::Schema;
use super::name;
use super::schema::{self, FieldProblem, ParseError, Parsed, ProblemKind};
use super::{Profile, ProfileId};

/// Where a loaded profile came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Origin {
    /// Its file, in the current schema.
    File,
    /// Its file, of an older schema, migrated in memory.
    Migrated {
        /// The file's schema version.
        from: i64,
    },
    /// The built-in default profile, standing in for a missing or bad default file.
    Builtin(BuiltinReason),
}

/// Why the built-in default profile is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BuiltinReason {
    /// The file does not exist (and could not be written, or was not to be written yet).
    Missing,
    /// The file exists but could not be used; it is kept as it is.
    BadFile,
}

/// A profile that can be shown, edited, applied and mapped to a mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Loaded {
    /// The profile, with built-in values for the fields that had problems.
    pub(crate) profile: Profile,
    /// The problems of its file, until it is saved.
    pub(crate) problems: Vec<FieldProblem>,
    /// Where it came from.
    pub(crate) origin: Origin,
}

/// A file made by a newer GX52: listed, but never edited, applied or written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Newer {
    /// The device type the file is for.
    pub(crate) device_type: DeviceType,
    /// The name, when it could be read.
    pub(crate) name: Option<String>,
    /// The file's schema version.
    pub(crate) version: i64,
}

/// One profile of the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Entry {
    /// A usable profile.
    Loaded(Loaded),
    /// A read-only file of a newer schema.
    Newer(Newer),
}

impl Entry {
    /// The device type the profile is for.
    pub(crate) fn device_type(&self) -> DeviceType {
        match self {
            Self::Loaded(loaded) => loaded.profile.device_type,
            Self::Newer(newer) => newer.device_type,
        }
    }

    /// The name, when there is one.
    pub(crate) fn name(&self) -> Option<&str> {
        match self {
            Self::Loaded(loaded) => Some(&loaded.profile.name),
            Self::Newer(newer) => newer.name.as_deref(),
        }
    }
}

/// Whether loading writes the file of a missing default profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DefaultPersistence {
    /// Write it from the built-in values.
    Write,
    /// Keep the built-in values in memory only: the legacy import has not completed yet and may
    /// still write the default from GX52 0.7's values.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the legacy import loads with it until it completes"
        )
    )]
    MemoryOnly,
}

/// Why a profile file was not loaded.
#[derive(Debug, thiserror::Error)]
pub(crate) enum FileFailure {
    /// The file could not be read, or is too large.
    #[error(transparent)]
    Read(#[from] ReadError),
    /// The file is not a profile.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// A default profile's file is for the other device type.
    #[error("the default profile file is for {found}, not {expected}")]
    WrongDefaultType {
        /// The device type the id is reserved for.
        expected: DeviceType,
        /// The device type the file says.
        found: DeviceType,
    },
    /// A default profile's file is of a newer schema; the built-in default is used.
    #[error("the default profile file was made by a newer GX52 (schema version {0})")]
    NewerDefault(i64),
    /// A missing default profile file could not be written.
    #[error("the default profile file could not be written: {0}")]
    WriteDefault(io::Error),
}

/// A profile file that was not loaded, by file name.
#[derive(Debug)]
pub(crate) struct FailedFile {
    /// The file name inside the profile directory.
    pub(crate) file_name: String,
    /// Why.
    pub(crate) failure: FileFailure,
}

/// What loading found besides the profiles: logged and shown once.
#[derive(Debug, Default)]
pub(crate) struct LoadReport {
    /// Files that were not loaded; every one is kept as it is.
    pub(crate) failed: Vec<FailedFile>,
    /// `.toml` files whose name is not a profile id; ignored and kept.
    pub(crate) ignored: Vec<String>,
    /// The profile directory could not be listed.
    pub(crate) unlisted: Option<io::Error>,
    /// Default profile files written from the built-in values.
    pub(crate) defaults_written: Vec<ProfileId>,
}

impl LoadReport {
    /// Logs everything worth knowing, once.
    pub(crate) fn log(&self, store: &ProfileStore) {
        if let Some(error) = &self.unlisted {
            tracing::warn!("Cannot list the profile directory: {error}");
        }
        for failed in &self.failed {
            tracing::warn!(
                "Profile file {} not loaded: {}",
                failed.file_name,
                failed.failure
            );
        }
        for name in &self.ignored {
            tracing::warn!("Profile file {name} ignored: its name is not a profile id");
        }
        for id in &self.defaults_written {
            tracing::info!("Default profile {id} written");
        }
        for (id, entry) in store.entries() {
            match entry {
                Entry::Loaded(loaded) => {
                    for problem in &loaded.problems {
                        tracing::warn!("Profile {id}: {problem}");
                    }
                }
                Entry::Newer(newer) => tracing::warn!(
                    "Profile {id} is read-only: made by a newer GX52 (schema version {})",
                    newer.version
                ),
            }
        }
    }

    /// The names of the files that could not be used, for the one toast per start.
    pub(crate) fn failed_files(&self) -> Vec<&str> {
        self.failed
            .iter()
            .map(|failed| failed.file_name.as_str())
            .chain(self.ignored.iter().map(String::as_str))
            .collect()
    }
}

/// Why a store operation failed.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "returned by the editing operations the M5 editor calls"
    )
)]
#[derive(Debug, thiserror::Error)]
pub(crate) enum StoreError {
    /// No profile has that id.
    #[error("no profile {0}")]
    Unknown(ProfileId),
    /// The profile was made by a newer GX52.
    #[error("profile {0} was made by a newer GX52 and is read-only")]
    ReadOnly(ProfileId),
    /// The default profile's file is bad; replacing it needs the user's confirmation.
    #[error("the file of profile {0} could not be read; replacing it needs confirmation")]
    NeedsConfirmation(ProfileId),
    /// A file appeared for a profile that had none when the store was loaded.
    #[error("a file for profile {0} appeared after loading")]
    FileAppeared(ProfileId),
    /// A default profile cannot be removed.
    #[error("the default profile {0} cannot be removed")]
    DefaultNotRemovable(ProfileId),
    /// A profile cannot move to another device type.
    #[error("profile {0} cannot change its device type")]
    TypeChange(ProfileId),
    /// A file operation failed.
    #[error("{context}: {source}")]
    Io {
        /// What was being done.
        context: String,
        /// The cause.
        source: io::Error,
    },
}

/// Every profile of the profile directory, by id.
#[derive(Debug, Clone)]
pub(crate) struct ProfileStore {
    dir: PathBuf,
    entries: BTreeMap<ProfileId, Entry>,
}

impl ProfileStore {
    /// Loads every profile file of `dir` (creating `dir` when needed), with the default profiles
    /// always present.
    ///
    /// Writes nothing but a missing default profile's file, and that only with
    /// [`DefaultPersistence::Write`]. Every problem is in the report; none stops the load.
    pub(crate) fn load(dir: &Path, persistence: DefaultPersistence) -> (Self, LoadReport) {
        Self::load_with(dir, persistence, Schema::CURRENT)
    }

    /// [`Self::load`] with the given schema, so the tests can cover the migration path.
    pub(crate) fn load_with(
        dir: &Path,
        persistence: DefaultPersistence,
        schema: Schema,
    ) -> (Self, LoadReport) {
        let mut store = Self {
            dir: dir.to_owned(),
            entries: BTreeMap::new(),
        };
        let mut report = LoadReport::default();
        let mut bad_defaults = BTreeSet::new();
        if let Err(error) = fs::create_dir_all(dir) {
            tracing::debug!("Cannot create {}: {error}", dir.display());
        }
        match profile_files(dir) {
            Ok(Listing { files, ignored }) => {
                report.ignored = ignored;
                for (id, path) in files {
                    store.load_file(id, &path, schema, &mut report, &mut bad_defaults);
                }
            }
            Err(error) => report.unlisted = Some(error),
        }
        for device_type in DeviceType::ALL {
            store.ensure_default(device_type, persistence, &bad_defaults, &mut report);
        }
        store.rename_duplicates();
        (store, report)
    }

    fn load_file(
        &mut self,
        id: ProfileId,
        path: &Path,
        schema: Schema,
        report: &mut LoadReport,
        bad_defaults: &mut BTreeSet<ProfileId>,
    ) {
        let file_name = id.file_name();
        let parsed = files::read_limited(path, PROFILE_LIMIT)
            .map_err(FileFailure::from)
            .and_then(|bytes| schema::parse(&bytes, schema).map_err(FileFailure::from));
        let reserved = id.default_type();
        let outcome = match (parsed, reserved) {
            (Err(failure), _) => Err(failure),
            (Ok(Parsed::Newer { version, .. }), Some(_)) => Err(FileFailure::NewerDefault(version)),
            (
                Ok(Parsed::Newer {
                    version,
                    device_type,
                    name,
                }),
                None,
            ) => Ok(Entry::Newer(Newer {
                device_type,
                name,
                version,
            })),
            (Ok(Parsed::Readable { profile, .. }), Some(expected))
                if profile.device_type != expected =>
            {
                Err(FileFailure::WrongDefaultType {
                    expected,
                    found: profile.device_type,
                })
            }
            (
                Ok(Parsed::Readable {
                    profile,
                    problems,
                    migrated_from,
                }),
                _,
            ) => Ok(Entry::Loaded(Loaded {
                profile: *profile,
                problems,
                origin: migrated_from.map_or(Origin::File, |from| Origin::Migrated { from }),
            })),
        };
        match outcome {
            Ok(entry) => {
                self.entries.insert(id, entry);
            }
            Err(failure) => {
                if reserved.is_some() {
                    bad_defaults.insert(id);
                }
                report.failed.push(FailedFile { file_name, failure });
            }
        }
    }

    /// Puts `device_type`'s default profile in the store when its file did not provide it.
    fn ensure_default(
        &mut self,
        device_type: DeviceType,
        persistence: DefaultPersistence,
        bad_defaults: &BTreeSet<ProfileId>,
        report: &mut LoadReport,
    ) {
        let id = ProfileId::default_for(device_type);
        if self.entries.contains_key(&id) {
            return;
        }
        let profile = Profile::builtin_default(device_type);
        let mut origin = Origin::Builtin(BuiltinReason::Missing);
        if bad_defaults.contains(&id) {
            origin = Origin::Builtin(BuiltinReason::BadFile);
        } else if persistence == DefaultPersistence::Write {
            let path = self.path(&id);
            match files::create_new(&path, schema::write(&profile).as_bytes()) {
                Ok(Created::Written) => {
                    origin = Origin::File;
                    report.defaults_written.push(id.clone());
                }
                // Appeared since the directory was listed: never replaced, and handled like a
                // bad file until the next load reads it.
                Ok(Created::AlreadyExists) => origin = Origin::Builtin(BuiltinReason::BadFile),
                Err(error) => report.failed.push(FailedFile {
                    file_name: id.file_name(),
                    failure: FileFailure::WriteDefault(error),
                }),
            }
        }
        self.entries.insert(
            id,
            Entry::Loaded(Loaded {
                profile,
                problems: Vec::new(),
                origin,
            }),
        );
    }

    /// Gives every profile whose name is already used within its device type a `(2)`, `(3)`, ...
    /// suffix in memory, defaults first, then in id order.
    fn rename_duplicates(&mut self) {
        let mut order: Vec<ProfileId> = DeviceType::ALL
            .into_iter()
            .map(ProfileId::default_for)
            .collect();
        order.extend(
            self.entries
                .keys()
                .filter(|id| id.default_type().is_none())
                .cloned(),
        );
        let mut seen: BTreeSet<(DeviceType, String)> = BTreeSet::new();
        for id in order {
            let Some(entry) = self.entries.get_mut(&id) else {
                continue;
            };
            let device_type = entry.device_type();
            let Some(current) = entry.name().map(str::to_owned) else {
                continue;
            };
            let unique = name::unique(&current, |candidate| {
                seen.contains(&(device_type, candidate.to_owned()))
            });
            if unique != current {
                match entry {
                    Entry::Loaded(loaded) => {
                        loaded.profile.name.clone_from(&unique);
                        loaded.problems.push(FieldProblem {
                            key: "name".to_owned(),
                            kind: ProblemKind::DuplicateName { to: unique.clone() },
                        });
                    }
                    Entry::Newer(newer) => newer.name = Some(unique.clone()),
                }
            }
            seen.insert((device_type, unique));
        }
    }

    /// Every profile, in id order.
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&ProfileId, &Entry)> {
        self.entries.iter()
    }

    /// The profile `id`, usable or read-only.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the mode mapping looks the mapped profiles up")
    )]
    pub(crate) fn entry(&self, id: &ProfileId) -> Option<&Entry> {
        self.entries.get(id)
    }

    /// The usable profile `id`.
    pub(crate) fn loaded(&self, id: &ProfileId) -> Option<&Loaded> {
        match self.entries.get(id)? {
            Entry::Loaded(loaded) => Some(loaded),
            Entry::Newer(_) => None,
        }
    }

    /// Whether a profile of `device_type` other than `except` is named `name`.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "the legacy import and the file import name their profiles"
        )
    )]
    pub(crate) fn name_taken(
        &self,
        device_type: DeviceType,
        name: &str,
        except: Option<&ProfileId>,
    ) -> bool {
        self.entries.iter().any(|(id, entry)| {
            Some(id) != except && entry.device_type() == device_type && entry.name() == Some(name)
        })
    }

    /// The path of `id`'s file.
    pub(crate) fn path(&self, id: &ProfileId) -> PathBuf {
        self.dir.join(id.file_name())
    }
}

/// Editing, saving and removing: wired to the profile editor in M5 (docs/roadmap.md).
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the profile editor of M5 saves and removes profiles; the tests cover it now"
    )
)]
impl ProfileStore {
    /// Replaces the in-memory values of `id`; nothing is written until [`Self::save`].
    ///
    /// # Errors
    ///
    /// [`StoreError::Unknown`], [`StoreError::ReadOnly`], or [`StoreError::TypeChange`] when
    /// `profile` is for another device type.
    pub(crate) fn update(&mut self, id: &ProfileId, profile: Profile) -> Result<(), StoreError> {
        match self.entries.get_mut(id) {
            None => Err(StoreError::Unknown(id.clone())),
            Some(Entry::Newer(_)) => Err(StoreError::ReadOnly(id.clone())),
            Some(Entry::Loaded(loaded)) if loaded.profile.device_type != profile.device_type => {
                Err(StoreError::TypeChange(id.clone()))
            }
            Some(Entry::Loaded(loaded)) => {
                loaded.profile = profile;
                Ok(())
            }
        }
    }

    /// Writes `id`'s file from its in-memory values.
    ///
    /// A file of an older schema is first copied to `<id>.toml.schema-<version>.bak`.
    ///
    /// # Errors
    ///
    /// [`StoreError::NeedsConfirmation`] for a default profile whose file is bad (see
    /// [`Self::save_replacing_bad_default`]), [`StoreError::ReadOnly`] for a newer file,
    /// [`StoreError::FileAppeared`] when a missing file appeared meanwhile, and
    /// [`StoreError::Io`].
    pub(crate) fn save(&mut self, id: &ProfileId) -> Result<(), StoreError> {
        let path = self.path(id);
        let loaded = self.loaded_mut(id)?;
        let contents = schema::write(&loaded.profile);
        match loaded.origin {
            Origin::Builtin(BuiltinReason::BadFile) => {
                return Err(StoreError::NeedsConfirmation(id.clone()));
            }
            Origin::Builtin(BuiltinReason::Missing) => {
                match files::create_new(&path, contents.as_bytes()).map_err(io_error(id))? {
                    Created::Written => {}
                    Created::AlreadyExists => return Err(StoreError::FileAppeared(id.clone())),
                }
            }
            Origin::Migrated { from } => {
                files::backup(&path, &files::schema_suffix(from)).map_err(io_error(id))?;
                files::replace(&path, contents.as_bytes()).map_err(io_error(id))?;
            }
            Origin::File => files::replace(&path, contents.as_bytes()).map_err(io_error(id))?,
        }
        loaded.origin = Origin::File;
        loaded.problems.clear();
        Ok(())
    }

    /// Writes a default profile over its bad file, after the user confirmed it: the bad file is
    /// first copied to `<id>.toml.rejected-<timestamp>`. Returns the backup's path, if there was
    /// still a file to back up.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] when the backup or the write fails (nothing is written without the
    /// backup), [`StoreError::Unknown`] and [`StoreError::ReadOnly`].
    pub(crate) fn save_replacing_bad_default(
        &mut self,
        id: &ProfileId,
        now: Timestamp,
    ) -> Result<Option<PathBuf>, StoreError> {
        let path = self.path(id);
        let loaded = self.loaded_mut(id)?;
        if loaded.origin != Origin::Builtin(BuiltinReason::BadFile) {
            return self.save(id).map(|()| None);
        }
        let backup = match files::backup(&path, &files::rejected_suffix(now)) {
            Ok(backup) => Some(backup),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(io_error(id)(error)),
        };
        files::replace(&path, schema::write(&loaded.profile).as_bytes()).map_err(io_error(id))?;
        loaded.origin = Origin::File;
        loaded.problems.clear();
        Ok(backup)
    }

    /// Adds `profile` under a new random id and writes its file. Returns the id.
    ///
    /// # Errors
    ///
    /// [`StoreError::Io`] when the file cannot be written; the store is then unchanged.
    pub(crate) fn add(&mut self, profile: Profile) -> Result<ProfileId, StoreError> {
        let contents = schema::write(&profile);
        loop {
            let id = ProfileId::random();
            match files::create_new(&self.path(&id), contents.as_bytes()).map_err(io_error(&id))? {
                Created::Written => {
                    self.entries.insert(
                        id.clone(),
                        Entry::Loaded(Loaded {
                            profile,
                            problems: Vec::new(),
                            origin: Origin::File,
                        }),
                    );
                    return Ok(id);
                }
                Created::AlreadyExists => {}
            }
        }
    }

    /// Removes `id` and deletes its file. The caller resets the mode-mapping entries that named
    /// it, after the user confirmed the removal.
    ///
    /// # Errors
    ///
    /// [`StoreError::DefaultNotRemovable`], [`StoreError::Unknown`], or [`StoreError::Io`] when
    /// the file cannot be deleted (the profile then stays).
    pub(crate) fn remove(&mut self, id: &ProfileId) -> Result<(), StoreError> {
        if id.default_type().is_some() {
            return Err(StoreError::DefaultNotRemovable(id.clone()));
        }
        if !self.entries.contains_key(id) {
            return Err(StoreError::Unknown(id.clone()));
        }
        match fs::remove_file(self.path(id)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error(id)(error)),
        }
        self.entries.remove(id);
        Ok(())
    }

    fn loaded_mut(&mut self, id: &ProfileId) -> Result<&mut Loaded, StoreError> {
        match self.entries.get_mut(id) {
            None => Err(StoreError::Unknown(id.clone())),
            Some(Entry::Newer(_)) => Err(StoreError::ReadOnly(id.clone())),
            Some(Entry::Loaded(loaded)) => Ok(loaded),
        }
    }
}

/// Wraps an I/O error about `id`'s file.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "used by the editing operations the M5 editor calls"
    )
)]
fn io_error(id: &ProfileId) -> impl Fn(io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        context: format!("profile {id}"),
        source,
    }
}

/// The files of the profile directory.
#[derive(Debug, Default)]
struct Listing {
    /// The profile files by id, in id order.
    files: Vec<(ProfileId, PathBuf)>,
    /// The `.toml` names that are not ids.
    ignored: Vec<String>,
}

/// The profile files of `dir`. Names that do not end in `.toml` (backups, temporary files, editor
/// leftovers) and entries that are not files are skipped silently.
fn profile_files(dir: &Path) -> io::Result<Listing> {
    let mut files = Vec::new();
    let mut ignored = Vec::new();
    for entry in fs::read_dir(dir)? {
        // One entry that cannot be read must not hide every other profile.
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::debug!("Skipping an entry of {}: {error}", dir.display());
                continue;
            }
        };
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(stem) = file_name.strip_suffix(".toml") else {
            continue;
        };
        // Follows symbolic links, so a linked profile file is read like any other.
        if !fs::metadata(entry.path()).is_ok_and(|metadata| metadata.is_file()) {
            continue;
        }
        match ProfileId::parse(stem) {
            Some(id) => files.push((id, entry.path())),
            None => ignored.push(file_name.to_owned()),
        }
    }
    files.sort();
    ignored.sort();
    Ok(Listing { files, ignored })
}

#[cfg(test)]
mod tests;
