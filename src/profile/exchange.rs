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
//! the disk, so the user can see the problems and cancel; an [`AddJob`] writes the confirmed
//! profile under a new id, and the caller adds it to the store with [`ProfileStore::added`]. Both
//! steps block (parsing reads the time zone database), so the app runs them through
//! `gio::spawn_blocking`. [`next_step`] decides between them, against the names in use.

use gettextrs::gettext;
use x52::device::DeviceType;

use super::migrate::Schema;
use super::schema::{self, FieldProblem, ParseError, Parsed};
#[cfg(doc)]
use super::store::{AddJob, ProfileStore};
use super::{Profile, name};

/// The contents of an exported profile: the schema without the id, which belongs to the store.
pub(crate) fn export(profile: &Profile) -> String {
    schema::write(profile)
}

/// A file read for import, checked and waiting to be written.
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

impl ImportPreview {
    /// Whether the user has to confirm the import first: the file had problems, or the profile
    /// gets another name than the file's. A clean file under its own name is imported at once.
    pub(crate) fn needs_confirmation(&self) -> bool {
        !self.problems.is_empty() || self.renamed_from.is_some()
    }
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
    let (profile, problems) = match schema::parse(bytes, Schema::CURRENT)? {
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
    let mut preview = ImportPreview {
        profile,
        problems,
        renamed_from: None,
    };
    revalidate_name(&mut preview, taken);
    Ok(preview)
}

/// Makes the name of `preview` unique against `taken`, starting again from the name in the file,
/// and returns whether it changed. The caller checks the name against the store again right before
/// the write, since profiles can be added while the user reads the preview, and asks for a
/// new confirmation when it changed.
pub(crate) fn revalidate_name(preview: &mut ImportPreview, taken: &[String]) -> bool {
    let original = preview
        .renamed_from
        .clone()
        .unwrap_or_else(|| preview.profile.name.clone());
    let unique = name::unique(&original, |candidate| {
        taken.iter().any(|name| name == candidate)
    });
    if unique == preview.profile.name {
        return false;
    }
    preview.renamed_from = (unique != original).then_some(original);
    preview.profile.name = unique;
    true
}

/// What an import does next, after [`next_step`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportStep {
    /// Show the preview and wait for the user's confirmation.
    Confirm,
    /// Reserve the name and write the file, with no `.await` in between.
    Write,
}

/// Checks the name of `preview` against `taken`, the names used or reserved now, and decides
/// what comes next: a name that changed always asks again, a file with problems or another name
/// asks until `confirmed`, and a clean file under its own name is written at once.
///
/// The import calls it after the file was read and after every confirmation, so a profile
/// created or renamed meanwhile is never overwritten by name; nothing is reserved until it
/// returns [`ImportStep::Write`], so the import never collides with itself.
pub(crate) fn next_step(
    preview: &mut ImportPreview,
    taken: &[String],
    confirmed: bool,
) -> ImportStep {
    let renamed = revalidate_name(preview, taken);
    if renamed || (!confirmed && preview.needs_confirmation()) {
        ImportStep::Confirm
    } else {
        ImportStep::Write
    }
}

#[cfg(test)]
mod tests;
