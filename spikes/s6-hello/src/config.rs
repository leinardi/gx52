//! Build-time constants. Meson exports them as environment variables to cargo
//! (see src/meson.build); the defaults keep plain `cargo check`/`clippy` working.

pub const APP_ID: &str = default_env(option_env!("APP_ID"), DEFAULT_APP_ID);
/// `"Devel"` for development builds, empty for release builds.
pub const PROFILE: &str = default_env(option_env!("PROFILE"), "Devel");
pub const VERSION: &str = default_env(option_env!("VERSION"), env!("CARGO_PKG_VERSION"));
pub const GETTEXT_PACKAGE: &str =
    default_env(option_env!("GETTEXT_PACKAGE"), env!("CARGO_PKG_NAME"));
pub const LOCALEDIR: &str = default_env(option_env!("LOCALEDIR"), "/usr/share/locale");
pub const PKGDATADIR: &str = default_env(
    option_env!("PKGDATADIR"),
    concat!("/usr/share/", env!("CARGO_PKG_NAME")),
);
// Kept out of the expressions above so a long application id cannot push them past rustfmt's width.
const DEFAULT_APP_ID: &str = "com.leinardi.gx52.Devel";
/// Base path of the `GResource` bundle, without the `.Devel` suffix.
pub const RESOURCE_BASE_PATH: &str = "/com/leinardi/gx52";

pub const fn is_devel() -> bool {
    matches!(PROFILE.as_bytes(), b"Devel")
}

const fn default_env(value: Option<&'static str>, default: &'static str) -> &'static str {
    match value {
        Some(value) => value,
        None => default,
    }
}
