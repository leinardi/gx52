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
// Adapted from gtk-rs-skills/skills/gtk-rs-new-app/assets/template/src/main.rs at 42eb5e9,
// originally MIT-licensed, Copyright (c) 2026 Roberto Leinardi

//! GX52: configure the Logitech X52 and X52 Pro joysticks.

mod application;
mod config;
mod device;
mod model;
mod preferences;
mod profile;
mod tray;
mod widgets;
mod window;
mod zones;

use std::{env, io, io::Write, path::PathBuf};

use gtk::{gio, glib, prelude::*};
use tracing_subscriber::EnvFilter;

use self::application::Application;

fn main() -> glib::ExitCode {
    // INFO for this crate and WARN for everything else; override with RUST_LOG.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(concat!(env!("CARGO_CRATE_NAME"), "=info,warn")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        // Logs go to stderr, so stdout carries only the device check's report.
        .with_writer(io::stderr)
        .init();

    // The device check harness runs without GTK (Devel builds only).
    let args: Vec<String> = env::args().skip(1).collect();
    match device::check::parse_args(&args, config::is_devel()) {
        Ok(Some(scenario)) => {
            return glib::ExitCode::from(device::check::run_on_device(scenario, &mut io::stdout()));
        }
        Ok(None) => {}
        Err(error) => {
            let _ = writeln!(io::stderr(), "gx52: {error}");
            return glib::ExitCode::from(2);
        }
    }

    init_i18n();

    let path = match resources_path() {
        Ok(path) => path,
        Err(err) => {
            tracing::error!("Unable to locate the executable: {err}");
            return glib::ExitCode::FAILURE;
        }
    };
    match gio::Resource::load(&path) {
        Ok(resources) => gio::resources_register(&resources),
        Err(err) => {
            tracing::error!(
                "Unable to load the resources from {}: {err}",
                path.display()
            );
            return glib::ExitCode::FAILURE;
        }
    }

    Application::new().run()
}

/// Selects the translation domain.
///
/// The locale itself is set by GTK: `gtk_init`, which runs in the application's `startup`, calls
/// `setlocale(LC_ALL, "")` before any string is translated.
fn init_i18n() {
    if let Err(err) = gettextrs::bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR) {
        tracing::warn!("Unable to bind the text domain: {err}");
    }
    if let Err(err) = gettextrs::textdomain(config::GETTEXT_PACKAGE) {
        tracing::warn!("Unable to switch to the text domain: {err}");
    }
}

/// The path of the compiled `GResource` bundle.
///
/// Under `meson devenv` the uninstalled bundle lives in the build directory, in the `data/`
/// directory next to the binary's `src/`; otherwise it is installed in `PKGDATADIR`.
fn resources_path() -> io::Result<PathBuf> {
    if env::var_os("MESON_DEVENV").is_some() {
        let mut path = env::current_exe()?;
        path.pop();
        path.pop();
        path.push("data/resources/resources.gresource");
        Ok(path)
    } else {
        Ok(PathBuf::from(config::PKGDATADIR).join("resources.gresource"))
    }
}
