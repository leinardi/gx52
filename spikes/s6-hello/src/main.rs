mod application;
mod config;
mod window;

use gettextrs::{LocaleCategory, gettext};
use gtk::{gio, glib, prelude::*};
use tracing_subscriber::EnvFilter;

use self::application::Application;

fn main() -> glib::ExitCode {
    // INFO for this crate and WARN for everything else; override with RUST_LOG.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(concat!(env!("CARGO_CRATE_NAME"), "=info,warn")));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    init_i18n();
    glib::set_application_name(&gettext("GX52"));

    let resources = load_resources();
    gio::resources_register(&resources);

    Application::new().run()
}

fn init_i18n() {
    // SAFETY: called before any other thread is spawned, so nothing can race the
    // process-global locale.
    #[allow(unsafe_code)]
    unsafe {
        gettextrs::setlocale(LocaleCategory::LcAll, "");
    }
    if let Err(err) = gettextrs::bindtextdomain(config::GETTEXT_PACKAGE, config::LOCALEDIR) {
        tracing::warn!("Unable to bind the text domain: {err}");
    }
    if let Err(err) = gettextrs::textdomain(config::GETTEXT_PACKAGE) {
        tracing::warn!("Unable to switch to the text domain: {err}");
    }
}

fn load_resources() -> gio::Resource {
    // Under `meson devenv` the uninstalled bundle lives in the build dir next to the binary's
    // sibling `data/` directory; otherwise it is installed in PKGDATADIR.
    let path = if std::env::var_os("MESON_DEVENV").is_some() {
        let mut path = std::env::current_exe().expect("Unable to get the executable path");
        path.pop();
        path.pop();
        path.push("data/resources/resources.gresource");
        path
    } else {
        std::path::Path::new(config::PKGDATADIR).join("resources.gresource")
    };
    gio::Resource::load(&path)
        .unwrap_or_else(|err| panic!("Could not load resources from {}: {err}", path.display()))
}
