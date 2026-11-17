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

//! The screenshot mode of Devel builds (`--screenshots <dir>`): the app runs on fixture profiles
//! in a temporary directory, a fake X52 Pro, a fixed time and pinned looks, captures the
//! metainfo screenshots into `<dir>` and exits (docs/architecture.md, "Testing strategy").

mod backend;
pub(crate) mod capture;

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk::{gio, glib};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use tempfile::TempDir;
use x52::device::DeviceType;

pub(crate) use self::backend::ScreenshotBackend;
use crate::config;
use crate::profile::{self, ProfileId, active};
use crate::time::{self, TimeSources};

/// The command-line option.
const OPTION: &str = "--screenshots";

/// The instant every clock shows: 2026-06-15T10:09:00Z.
const FIXED_TIME: Timestamp = Timestamp::constant(1_781_518_140, 0);

/// The local time zone of the screenshots.
const LOCAL_ZONE: &str = "Europe/Berlin";

/// The profile active in the screenshots.
pub(crate) const ACTIVE_PROFILE: &str = "3f8e2c41-7b5a-4d9e-9c13-5a6b2e8f0d71";

/// The fixture profiles, by file name.
pub(crate) const PROFILES: [(&str, &str); 4] = [
    (
        "x52-pro-default.toml",
        include_str!("profiles/x52-pro-default.toml"),
    ),
    (
        "3f8e2c41-7b5a-4d9e-9c13-5a6b2e8f0d71.toml",
        include_str!("profiles/3f8e2c41-7b5a-4d9e-9c13-5a6b2e8f0d71.toml"),
    ),
    (
        "8c1d4f70-2e93-4b6a-a5d8-0f7c3e9b1a24.toml",
        include_str!("profiles/8c1d4f70-2e93-4b6a-a5d8-0f7c3e9b1a24.toml"),
    ),
    (
        "c2a7e95b-41d6-4f08-b3e1-6d9a0c5f7e38.toml",
        include_str!("profiles/c2a7e95b-41d6-4f08-b3e1-6d9a0c5f7e38.toml"),
    ),
];

/// The window size of the wide shots, in application pixels.
pub(crate) const WIDE: (i32, i32) = (1000, 700);

/// The window size of the narrow shot: below the window's 600sp breakpoint.
pub(crate) const NARROW: (i32, i32) = (420, 700);

/// The directory the shots are saved in; set once the mode is prepared.
static OUT_DIR: OnceLock<PathBuf> = OnceLock::new();

/// The local zone, loaded by [`prepare`] so [`local_zone`] never fails.
static ZONE: OnceLock<TimeZone> = OnceLock::new();

/// Set when the run failed, so the process exits with 1.
static FAILED: AtomicBool = AtomicBool::new(false);

/// A screenshot run asked for on the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    /// Where the shots go.
    pub(crate) out_dir: PathBuf,
    /// The other arguments, for the application.
    pub(crate) remaining: Vec<String>,
}

/// Why the command line asks for no valid screenshot run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArgsError {
    /// The option is only in Devel builds.
    ReleaseBuild,
    /// The option has no directory.
    MissingDirectory,
}

impl fmt::Display for ArgsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReleaseBuild => write!(
                formatter,
                "{OPTION} is only available in development builds"
            ),
            Self::MissingDirectory => write!(formatter, "{OPTION} needs a directory"),
        }
    }
}

/// The screenshot run asked for in `args` (without the program name), if any, with the option
/// removed from the arguments; `devel` is whether this is a Devel build.
///
/// # Errors
///
/// [`ArgsError`] for the option in a release build or without a directory.
pub(crate) fn parse_args(args: &[String], devel: bool) -> Result<Option<Request>, ArgsError> {
    let Some(position) = args
        .iter()
        .position(|arg| arg == OPTION || arg.starts_with(&format!("{OPTION}=")))
    else {
        return Ok(None);
    };
    if !devel {
        return Err(ArgsError::ReleaseBuild);
    }
    let (dir, taken) = match args[position].split_once('=') {
        Some((_, dir)) => (Some(dir), 1),
        None => (args.get(position + 1).map(String::as_str), 2),
    };
    let dir = match dir {
        Some(dir) if !dir.is_empty() && !dir.starts_with('-') => dir,
        Some(_) | None => return Err(ArgsError::MissingDirectory),
    };
    let mut remaining = args.to_vec();
    remaining.drain(position..position + taken);
    Ok(Some(Request {
        out_dir: PathBuf::from(dir),
        remaining,
    }))
}

/// Why the screenshot mode cannot start.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PrepareError {
    /// The settings would be written to the user's real backend.
    #[error("GSETTINGS_BACKEND must be \"memory\", so the user's settings are never changed")]
    NotMemoryBackend,
    /// The fixed local zone is not in the time zone database.
    #[error("the time zone {LOCAL_ZONE} is not available")]
    Zone(#[source] jiff::Error),
    /// A directory or fixture could not be written.
    #[error("cannot write {path}")]
    Io {
        /// The path.
        path: PathBuf,
        /// The cause.
        #[source]
        source: io::Error,
    },
    /// A setting could not be written.
    #[error("cannot write the settings")]
    Settings(#[source] glib::BoolError),
    /// Something read the config root or the clock before the mode could replace them.
    #[error("the {0} was read before the screenshot mode set it")]
    AlreadyRead(&'static str),
}

/// What [`prepare`] made, kept until the process ends: the temporary config root is removed with
/// it.
#[derive(Debug)]
pub(crate) struct Session {
    root: TempDir,
}

impl Session {
    /// An empty directory to bind the translations to, so the strings stay untranslated.
    pub(crate) fn locale_dir(&self) -> PathBuf {
        self.root.path().join("locale")
    }
}

/// Whether `backend`, the value of `GSETTINGS_BACKEND`, keeps the settings in memory.
fn is_memory_backend(backend: Option<&OsStr>) -> bool {
    backend == Some(OsStr::new("memory"))
}

/// Prepares the run of `request`, `backend` being the value of `GSETTINGS_BACKEND`: the config
/// root, the fixture profiles, the fixed clock and zone, and the settings. Call it before the
/// application starts.
///
/// # Errors
///
/// [`PrepareError`] when the settings are not in memory, before anything is changed; or when a
/// step fails.
pub(crate) fn prepare(request: &Request, backend: Option<&OsStr>) -> Result<Session, PrepareError> {
    if !is_memory_backend(backend) {
        return Err(PrepareError::NotMemoryBackend);
    }
    let zone = TimeZone::get(LOCAL_ZONE).map_err(PrepareError::Zone)?;
    let _ = ZONE.set(zone);
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| PrepareError::Io { path, source }
    };
    fs::create_dir_all(&request.out_dir).map_err(io(&request.out_dir))?;
    let root = tempfile::Builder::new()
        .prefix("gx52-screenshots-")
        .tempdir()
        .map_err(io(&std::env::temp_dir()))?;
    let session = Session { root };
    fs::create_dir(session.locale_dir()).map_err(io(&session.locale_dir()))?;
    write_profiles(&profile::profiles_dir(session.root.path()))?;

    profile::set_config_dir(session.root.path().to_path_buf())
        .map_err(|_| PrepareError::AlreadyRead("config directory"))?;
    time::set(TimeSources {
        clock: Box::new(fixed_now),
        local_zone,
    })
    .map_err(|_| PrepareError::AlreadyRead("clock"))?;
    set_up_settings()?;
    let _ = OUT_DIR.set(request.out_dir.clone());
    Ok(session)
}

/// Writes the fixture profiles into `dir`.
fn write_profiles(dir: &Path) -> Result<(), PrepareError> {
    fs::create_dir_all(dir).map_err(|source| PrepareError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for (name, text) in PROFILES {
        let path = dir.join(name);
        fs::write(&path, text).map_err(|source| PrepareError::Io { path, source })?;
    }
    Ok(())
}

/// The keys the screenshots need, in the in-memory settings.
fn set_up_settings() -> Result<(), PrepareError> {
    let backend = gio::SettingsBackend::default();
    if backend.type_().name() != "GMemorySettingsBackend" {
        return Err(PrepareError::NotMemoryBackend);
    }
    let settings = gio::Settings::new(config::APP_ID);
    let pro = DeviceType::X52Pro;
    let results = [
        settings.set_string("last-device-type", pro.id()),
        settings.set_string(active::settings_key(pro), ACTIVE_PROFILE),
        settings.set_boolean("legacy-import-done", true),
        settings.set_boolean("run-in-background", false),
        settings.set_boolean("autostart", false),
        settings.set_boolean("virtual-mouse", true),
        settings.set_int("window-width", WIDE.0),
        settings.set_int("window-height", WIDE.1),
        settings.set_boolean("is-maximized", false),
        settings.set_string("last-view", "leds"),
    ];
    results
        .into_iter()
        .collect::<Result<(), glib::BoolError>>()
        .map_err(PrepareError::Settings)
}

/// The fixed instant.
fn fixed_now() -> Timestamp {
    FIXED_TIME
}

/// The fixed local zone, which [`prepare`] loads before it installs this function.
fn local_zone() -> TimeZone {
    ZONE.get().cloned().unwrap_or_else(|| {
        tracing::error!("Screenshots: the local zone was read before it was loaded");
        TimeZone::UTC
    })
}

/// Whether the app runs in screenshot mode.
pub(crate) fn active() -> bool {
    OUT_DIR.get().is_some()
}

/// The id of the active fixture profile; `None` only if [`ACTIVE_PROFILE`] were no id.
pub(crate) fn active_profile() -> Option<ProfileId> {
    ProfileId::parse(ACTIVE_PROFILE)
}

/// Pins every look the host could change: animations, fonts and their rendering, and the light
/// style. Call it in `startup`, after GTK is initialized; the values are set at application
/// priority, over the desktop's.
pub(crate) fn pin_look() {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
        settings.set_gtk_font_name(Some("Adwaita Sans 11"));
        settings.set_gtk_xft_antialias(1);
        settings.set_gtk_xft_hinting(1);
        settings.set_gtk_xft_hintstyle(Some("hintslight"));
        settings.set_gtk_xft_rgba(Some("none"));
        settings.set_gtk_xft_dpi(96 * 1024);
        settings.set_gtk_hint_font_metrics(true);
    }
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceLight);
}

/// Ends the run with a failure: the process exits with 1.
pub(crate) fn fail(app: &impl IsA<gio::Application>, message: &str) {
    tracing::error!("Screenshots: {message}");
    FAILED.store(true, Ordering::SeqCst);
    app.quit();
}

/// The process exit code after a run that ended with `code`.
pub(crate) fn exit_code(code: glib::ExitCode) -> glib::ExitCode {
    if FAILED.load(Ordering::SeqCst) {
        glib::ExitCode::FAILURE
    } else {
        code
    }
}

/// The directory the shots are saved in, once prepared.
fn out_dir() -> Option<&'static Path> {
    OUT_DIR.get().map(PathBuf::as_path)
}

#[cfg(test)]
mod tests;
