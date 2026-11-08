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

//! The file operations of the profile store (docs/profiles.md, "Files" and "Persisting the
//! default"). Every function blocks: the app calls them through `gio::spawn_blocking`.
//!
//! No function here replaces a file it was not asked to replace: new files are created with
//! create-new semantics, backups never overwrite a backup, and only [`replace`] writes over an
//! existing file, atomically.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};

use gtk::gio::{self, prelude::*};
use jiff::Timestamp;

/// The largest profile or import file that is parsed: a valid profile is well under 2 KiB.
pub(crate) const PROFILE_LIMIT: u64 = 64 * 1024;

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadError {
    /// The file is larger than the limit; it is not parsed.
    #[error("the file is larger than {limit} bytes")]
    TooLarge {
        /// The limit, in bytes.
        limit: u64,
    },
    /// The file could not be opened or read.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The bytes of `path`, when it has at most `limit` of them.
///
/// # Errors
///
/// [`ReadError::TooLarge`] past `limit` (the rest is never read), [`ReadError::Io`] otherwise.
pub(crate) fn read_limited(path: &Path, limit: u64) -> Result<Vec<u8>, ReadError> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit.saturating_add(1))
        .read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).map_or(true, |len| len > limit) {
        return Err(ReadError::TooLarge { limit });
    }
    Ok(bytes)
}

/// Writes `contents` over `path` atomically: a crash leaves either the old file or the new one.
///
/// # Errors
///
/// The I/O error of GIO's temporary file, write or rename.
pub(crate) fn replace(path: &Path, contents: &[u8]) -> io::Result<()> {
    gio::File::for_path(path)
        .replace_contents(
            contents,
            None,
            false,
            gio::FileCreateFlags::NONE,
            gio::Cancellable::NONE,
        )
        .map(|_etag| ())
        .map_err(|error| io::Error::other(error.to_string()))
}

/// What [`create_new`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Created {
    /// The file did not exist and now holds the contents.
    Written,
    /// A file of that name already existed; it was left untouched.
    AlreadyExists,
}

/// Creates `path` with `contents`, complete or not at all, and never over an existing file.
///
/// The contents go to a temporary file in the same directory, which is synced and then
/// hard-linked to `path`; the link fails if `path` exists, so even a file created a moment before
/// by someone else is never replaced.
///
/// # Errors
///
/// The I/O error of the temporary file or the link.
pub(crate) fn create_new(path: &Path, contents: &[u8]) -> io::Result<Created> {
    let (temporary, mut file) = temporary_beside(path)?;
    let linked = file
        .write_all(contents)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::hard_link(&temporary, path));
    drop(file);
    let removed = fs::remove_file(&temporary);
    match linked {
        Ok(()) => {
            if let Err(error) = removed {
                tracing::warn!("Cannot remove {}: {error}", temporary.display());
            }
            Ok(Created::Written)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(Created::AlreadyExists),
        Err(error) => Err(error),
    }
}

/// A new, empty temporary file next to `path`, named so the store ignores it.
fn temporary_beside(path: &Path) -> io::Result<(PathBuf, File)> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    for attempt in 0_u32.. {
        let candidate = directory.join(format!(".{stem}.tmp-{}-{attempt}", std::process::id()));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("no free temporary file name"))
}

/// Copies `source` to `<source>.<suffix>`, or to `<source>.<suffix>-2`, `-3`, ... when that name is
/// taken, so a backup never overwrites a backup. Returns the path of the copy.
///
/// # Errors
///
/// The I/O error of reading `source` or creating the copy.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "saving, wired to the profile editor in M5, uses it"
    )
)]
pub(crate) fn backup(source: &Path, suffix: &str) -> io::Result<PathBuf> {
    let contents = fs::read(source)?;
    let base = format!("{}.{suffix}", source.display());
    for counter in 1_u32.. {
        let candidate = if counter == 1 {
            PathBuf::from(&base)
        } else {
            PathBuf::from(format!("{base}-{counter}"))
        };
        match create_new(&candidate, &contents)? {
            Created::Written => return Ok(candidate),
            Created::AlreadyExists => {}
        }
    }
    Err(io::Error::other("no free backup name"))
}

/// The suffix of the backup of a bad default profile file: `rejected-YYYYMMDDTHHMMSSZ`, in UTC.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "saving, wired to the profile editor in M5, uses it"
    )
)]
pub(crate) fn rejected_suffix(now: Timestamp) -> String {
    format!("rejected-{}", now.strftime("%Y%m%dT%H%M%SZ"))
}

/// The suffix of the backup of a file of an older schema: `schema-<version>.bak`.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "saving, wired to the profile editor in M5, uses it"
    )
)]
pub(crate) fn schema_suffix(version: i64) -> String {
    format!("schema-{version}.bak")
}

#[cfg(test)]
mod tests;
