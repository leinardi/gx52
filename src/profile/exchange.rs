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

//! Importing and exporting single profile files (docs/profiles.md, "Import and export").
//!
//! An import takes two steps: [`prepare_import`] validates the file without touching the store or
//! the disk, so the user can see the problems and cancel; [`write_import`] writes the confirmed
//! profile under a new id, and the caller adds it to the store with
//! [`ProfileStore::insert`](super::store::ProfileStore::insert). Both steps block (parsing reads
//! the time zone database), so the app runs them through `gio::spawn_blocking`.

use std::io;
use std::path::Path;

use gettextrs::gettext;
use x52::device::DeviceType;

use super::files::{self, Created};
use super::migrate::Schema;
use super::schema::{self, FieldProblem, ParseError, Parsed};
use super::{Profile, ProfileId, name};

/// The contents of an exported profile: the schema without the id, which belongs to the store.
pub(crate) fn export(profile: &Profile) -> String {
    schema::write(profile)
}

/// A file read for import and waiting for the user's confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportPreview {
    /// The profile as it will be imported: problems fixed to the type defaults, the name made
    /// unique within the type.
    pub(crate) profile: Profile,
    /// What was wrong in the file, field by field, shown before confirming.
    pub(crate) problems: Vec<FieldProblem>,
    /// The name in the file, when the imported name differs from it.
    pub(crate) renamed_from: Option<String>,
}

/// Why a file cannot be imported.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ImportRefusal {
    /// The file is too large or cannot be read.
    #[error("the file cannot be read: {0}")]
    Read(#[from] super::files::ReadError),
    /// The file is not a profile.
    #[error(transparent)]
    NotAProfile(#[from] ParseError),
    /// The file is a profile of another device type.
    #[error("the file is a profile for the {found}, not the {expected}")]
    OtherType {
        /// The type whose list the import was started from.
        expected: DeviceType,
        /// The type the file is for.
        found: DeviceType,
    },
    /// The file was made by a newer GX52.
    #[error("the file was made by a newer GX52 (schema version {0})")]
    Newer(i64),
}

impl ImportRefusal {
    /// The message shown to the user.
    pub(crate) fn message(&self) -> String {
        match self {
            Self::Read(_) => gettext("The file cannot be read"),
            Self::NotAProfile(ParseError::UnknownDeviceType(found)) => {
                gettext("This is a profile for an unknown device type ({type})")
                    .replace("{type}", found)
            }
            Self::NotAProfile(_) => gettext("The file is not a GX52 profile"),
            Self::OtherType { found, .. } => match found {
                DeviceType::X52Pro => gettext("This is a profile for the X52 Pro"),
                DeviceType::X52 => gettext("This is a profile for the X52"),
            },
            Self::Newer(_) => gettext("The profile was made by a newer version of GX52"),
        }
    }
}

/// Reads `bytes` as a profile for `device_type`'s list, with the store's validation: the
/// problems are fixed to the type defaults and listed, and the name is made unique against
/// `taken`, the names of `device_type`'s profiles.
///
/// Writes nothing.
///
/// # Errors
///
/// [`ImportRefusal`] when the file is not a profile, is for another or an unknown device type, or
/// was made by a newer GX52.
pub(crate) fn prepare_import(
    bytes: &[u8],
    device_type: DeviceType,
    taken: &[String],
) -> Result<ImportPreview, ImportRefusal> {
    let (mut profile, problems) = match schema::parse(bytes, Schema::CURRENT)? {
        Parsed::Newer { version, .. } => return Err(ImportRefusal::Newer(version)),
        Parsed::Readable {
            profile, problems, ..
        } => (*profile, problems),
    };
    if profile.device_type != device_type {
        return Err(ImportRefusal::OtherType {
            expected: device_type,
            found: profile.device_type,
        });
    }
    let unique = name::unique(&profile.name, |candidate| {
        taken.iter().any(|name| name == candidate)
    });
    let renamed_from =
        (unique != profile.name).then(|| std::mem::replace(&mut profile.name, unique));
    Ok(ImportPreview {
        profile,
        problems,
        renamed_from,
    })
}

/// Writes the confirmed import into `profiles_dir` under a new random id, never a default one,
/// and returns the id; the caller then adds it to the store.
///
/// # Errors
///
/// The I/O error of the write; nothing is left behind.
pub(crate) fn write_import(preview: &ImportPreview, profiles_dir: &Path) -> io::Result<ProfileId> {
    let contents = schema::write(&preview.profile);
    loop {
        let id = ProfileId::random();
        match files::create_new(&profiles_dir.join(id.file_name()), contents.as_bytes())? {
            Created::Written => return Ok(id),
            Created::AlreadyExists => {}
        }
    }
}

#[cfg(test)]
mod tests;
