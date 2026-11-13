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

//! Ends the launch a tray host starts for a click that shows no window.
//!
//! GNOME's `AppIndicator` extension starts a launch for every tray click and hands its activation
//! token over first; mutter shows the busy cursor until that launch ends. A window presented with
//! the token ends it. Without a window only `gtk_shell1.set_startup_id` does, and GTK sends that
//! only when the compositor has no xdg-activation, which mutter has:
//! `gdk_display_notify_startup_complete` does nothing there. So this sends it itself, on a short
//! Wayland connection of its own to GTK's compositor. On a compositor without `gtk_shell1` (KDE
//! Plasma ends its launches itself) it does nothing; an X11 display gets GTK's
//! `notify_startup_complete`.

use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use wayland_client::globals::{BindError, GlobalError, GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry::WlRegistry;
use wayland_client::{ConnectError, Connection, Dispatch, DispatchError, QueueHandle};

use self::protocol::gtk_shell1::{self, GtkShell1};

/// The client side of GTK's private `gtk_shell1` protocol (`gtk-shell.xml`, from GTK), laid out
/// as wayland-scanner documents it.
#[expect(
    clippy::single_component_path_imports,
    reason = "the generated code names the crate itself unqualified"
)]
// Clippy leaves wildcard imports alone in a test build, so the expectation holds only outside it.
#[cfg_attr(
    not(test),
    expect(
        clippy::wildcard_imports,
        reason = "the generated code names the core protocol's items unqualified, so these are \
                  the imports wayland-scanner documents for it"
    )
)]
mod protocol {
    use wayland_client;
    use wayland_client::protocol::*;

    pub(super) mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!("src/tray/gtk-shell.xml");
    }
    use self::__interfaces::*;

    wayland_scanner::generate_client_code!("src/tray/gtk-shell.xml");
}

/// Why the launch could not be ended over Wayland.
#[derive(Debug, thiserror::Error)]
enum EndError {
    #[error("connecting to {}: {source}", .socket.display())]
    Connect { socket: PathBuf, source: io::Error },
    #[error("opening the Wayland connection: {0}")]
    Backend(#[from] ConnectError),
    #[error("reading the Wayland globals: {0}")]
    Globals(#[from] GlobalError),
    #[error("no gtk_shell1: {0}")]
    Bind(#[from] BindError),
    #[error("sending the startup id: {0}")]
    Dispatch(#[from] DispatchError),
}

/// The GDK type of a Wayland display.
const WAYLAND_DISPLAY_TYPE: &str = "GdkWaylandDisplay";

/// Ends the launch the tray host started with `token`, for a click that shows no window.
///
/// The Wayland round trip runs on a blocking-pool thread, not on the main thread.
pub(crate) fn end(token: &str) {
    let Some(display) = gdk::Display::default() else {
        return;
    };
    if display.type_().name() != WAYLAND_DISPLAY_TYPE {
        #[expect(
            deprecated,
            reason = "the X11 path; its replacement, gdk_toplevel_set_startup_id, needs a window, \
                      which these clicks do not show"
        )]
        display.notify_startup_complete(token);
        return;
    }
    // GTK's own socket: WAYLAND_DISPLAY may be unset, GTK then uses its default name.
    let socket = socket_path(display.name().as_str(), &glib::user_runtime_dir());
    let token = token.to_owned();
    drop(gio::spawn_blocking(move || {
        if let Err(error) = end_over_wayland(&socket, token) {
            tracing::debug!("Could not end the tray click's launch: {error}");
        }
    }));
}

/// The socket of the Wayland display named `name`: a relative name lives in `runtime_dir`.
fn socket_path(name: &str, runtime_dir: &Path) -> PathBuf {
    let name = Path::new(name);
    if name.is_absolute() {
        name.to_owned()
    } else {
        runtime_dir.join(name)
    }
}

fn end_over_wayland(socket: &Path, token: String) -> Result<(), EndError> {
    let stream = UnixStream::connect(socket).map_err(|source| EndError::Connect {
        socket: socket.to_owned(),
        source,
    })?;
    let connection = Connection::from_socket(stream)?;
    let (globals, mut queue) = registry_queue_init::<Launch>(&connection)?;
    let shell: GtkShell1 = globals.bind(&queue.handle(), 1..=1, ())?;
    shell.set_startup_id(Some(token));
    // The round trip makes sure the request reached the compositor before the connection closes.
    queue.roundtrip(&mut Launch)?;
    Ok(())
}

/// The state of the short-lived connection: none, every event is ignored.
struct Launch;

impl Dispatch<WlRegistry, GlobalListContents> for Launch {
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: <WlRegistry as wayland_client::Proxy>::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<GtkShell1, ()> for Launch {
    fn event(
        _: &mut Self,
        _: &GtkShell1,
        _: gtk_shell1::Event,
        (): &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_display_name_is_a_socket_in_the_runtime_dir() {
        let runtime = Path::new("/run/user/1000");
        assert_eq!(
            socket_path("wayland-0", runtime),
            Path::new("/run/user/1000/wayland-0")
        );
    }

    #[test]
    fn an_absolute_display_name_is_the_socket() {
        let runtime = Path::new("/run/user/1000");
        assert_eq!(
            socket_path("/tmp/wl-test", runtime),
            Path::new("/tmp/wl-test")
        );
    }
}
