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
//
// Adapted from gtk-rs-skills/skills/gtk-rs-new-app/assets/template/src/config.rs at 42eb5e9,
// originally MIT-licensed, Copyright (c) 2026 Roberto Leinardi

//! Build-time constants.
//!
//! Meson sets them through the environment of its cargo invocations (`src/meson.build`); the
//! defaults are the Devel profile's, so a plain `cargo build` (the pre-commit hooks) still works.

/// The application ID: `com.leinardi.gx52`, or `com.leinardi.gx52.Devel` for development builds.
pub const APP_ID: &str = default_env(option_env!("APP_ID"), DEFAULT_APP_ID);
/// `"Devel"` for development builds, empty for release builds.
pub const PROFILE: &str = default_env(option_env!("PROFILE"), "Devel");
/// The version shown in the About dialog: the Meson project version plus the Devel suffix.
pub const VERSION: &str = default_env(option_env!("VERSION"), env!("CARGO_PKG_VERSION"));
/// The gettext translation domain.
pub const GETTEXT_PACKAGE: &str =
    default_env(option_env!("GETTEXT_PACKAGE"), env!("CARGO_PKG_NAME"));
/// Where the compiled translations are installed.
pub const LOCALEDIR: &str = default_env(option_env!("LOCALEDIR"), "/usr/share/locale");
/// Where the `GResource` bundle is installed.
pub const PKGDATADIR: &str = default_env(
    option_env!("PKGDATADIR"),
    concat!("/usr/share/", env!("CARGO_PKG_NAME")),
);
// Kept out of the expressions above so the application ID cannot push them past rustfmt's width.
const DEFAULT_APP_ID: &str = "com.leinardi.gx52.Devel";
/// Base path of the `GResource` bundle, without the `.Devel` suffix.
pub const RESOURCE_BASE_PATH: &str = "/com/leinardi/gx52";

/// Whether this is a development build.
pub const fn is_devel() -> bool {
    matches!(PROFILE.as_bytes(), b"Devel")
}

const fn default_env(value: Option<&'static str>, default: &'static str) -> &'static str {
    match value {
        Some(value) => value,
        None => default,
    }
}
