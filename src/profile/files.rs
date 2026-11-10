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
use std::io::{self, Read};
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
/// by someone else is never replaced. See [`create_new_from`] for file systems without hard links.
///
/// # Errors
///
/// The I/O error of the temporary file or the link.
pub(crate) fn create_new(path: &Path, contents: &[u8]) -> io::Result<Created> {
    create_new_from(path, &mut &contents[..])
}

/// [`create_new`] with the contents streamed from `source`.
///
/// On a file system that has no hard links (vfat, exFAT, some FUSE mounts) the link fails with
/// an error other than "exists"; the file is then created directly with create-new semantics and
/// written in place. That is still never over an existing file, but a crash can leave it
/// incomplete: the store reports such a file as unreadable and keeps it, and the legacy import
/// reports it as a conflict instead of counting it as imported.
///
/// # Errors
///
/// The I/O error of reading `source`, the temporary file, the link or the direct write.
pub(crate) fn create_new_from(path: &Path, source: &mut impl Read) -> io::Result<Created> {
    let (temporary, mut file) = temporary_beside(path)?;
    let written = io::copy(source, &mut file).and_then(|_| file.sync_all());
    drop(file);
    let linked = written.and_then(|()| fs::hard_link(&temporary, path));
    let outcome = match linked {
        Ok(()) => Ok(Created::Written),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(Created::AlreadyExists),
        Err(error) if is_link_unsupported(&error) => {
            tracing::debug!("No hard links for {}: {error}", path.display());
            create_in_place(path, &temporary)
        }
        Err(error) => Err(error),
    };
    if let Err(error) = fs::remove_file(&temporary) {
        tracing::warn!("Cannot remove {}: {error}", temporary.display());
    }
    outcome
}

/// Whether a failed `link(2)` means the file system has no hard links.
fn is_link_unsupported(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::Unsupported
        || error.kind() == io::ErrorKind::PermissionDenied
        || error.raw_os_error() == Some(rustix::io::Errno::OPNOTSUPP.raw_os_error())
}

/// Creates `path` with create-new semantics and copies the synced `temporary` into it.
fn create_in_place(path: &Path, temporary: &Path) -> io::Result<Created> {
    let mut file = match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            return Ok(Created::AlreadyExists);
        }
        Err(error) => return Err(error),
    };
    io::copy(&mut File::open(temporary)?, &mut file)?;
    file.sync_all()?;
    Ok(Created::Written)
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
/// The copy is streamed, so a bad file of any size is backed up without reading it into memory.
///
/// # Errors
///
/// The I/O error of reading `source` or creating the copy.
pub(crate) fn backup(source: &Path, suffix: &str) -> io::Result<PathBuf> {
    let base = format!("{}.{suffix}", source.display());
    for counter in 1_u32.. {
        let candidate = if counter == 1 {
            PathBuf::from(&base)
        } else {
            PathBuf::from(format!("{base}-{counter}"))
        };
        match create_new_from(&candidate, &mut File::open(source)?)? {
            Created::Written => return Ok(candidate),
            Created::AlreadyExists => {}
        }
    }
    Err(io::Error::other("no free backup name"))
}

/// The suffix of the backup of a bad default profile file: `rejected-YYYYMMDDTHHMMSSZ`, in UTC.
pub(crate) fn rejected_suffix(now: Timestamp) -> String {
    format!("rejected-{}", now.strftime("%Y%m%dT%H%M%SZ"))
}

/// The suffix of the backup of a file of an older schema: `schema-<version>.bak`.
pub(crate) fn schema_suffix(version: i64) -> String {
    format!("schema-{version}.bak")
}

#[cfg(test)]
mod tests;
