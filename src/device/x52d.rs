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

//! Detects a running libx52 `x52d`, which would fight GX52 over the LEDs, the MFD and the clocks
//! (docs/protocol.md, "libx52's x52d"; risk R11 of docs/rewrite-analysis.md).
//!
//! The daemon listens on a Unix command socket; a successful `connect` means it is running. A
//! socket file nobody listens on (left by a daemon that died) refuses the connection and is not a
//! running daemon.

use std::path::Path;
use std::time::Duration;

use rustix::io::Errno;
use rustix::net::{self, AddressFamily, SocketAddrUnix, SocketFlags, SocketType};

/// Where `x52d` creates its command socket by default: `$(prefix)/$(localstatedir)/run/x52d.cmd`
/// (`daemon/constants.h` and `include/meson.build` of nirenjan/libx52 at 2da88f4). A distribution
/// package (prefix `/usr`, local state in `/var`) puts it in `/run`, which `/var/run` links to; a
/// source build with Meson's default prefix puts it under `/usr/local/var/run`.
pub(crate) const SOCKET_PATHS: [&str; 3] = [
    "/run/x52d.cmd",
    "/var/run/x52d.cmd",
    "/usr/local/var/run/x52d.cmd",
];

/// How often the device watch checks for the daemon.
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// Whether `x52d` is running, at any of its default socket paths.
pub(crate) fn is_running() -> bool {
    SOCKET_PATHS
        .iter()
        .any(|path| is_listening(Path::new(path)))
}

/// Whether something accepts connections on the Unix socket at `path`.
///
/// The socket is nonblocking, so a daemon whose backlog is full (`EAGAIN`) never blocks the
/// caller; it is running all the same.
pub(crate) fn is_listening(path: &Path) -> bool {
    let Ok(address) = SocketAddrUnix::new(path) else {
        return false;
    };
    let Ok(socket) = net::socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
        None,
    ) else {
        return false;
    };
    match net::connect(&socket, &address) {
        Ok(()) | Err(Errno::AGAIN | Errno::INPROGRESS) => true,
        Err(_) => false,
    }
}

/// The daemon's state as last reported, so only changes are reported.
#[derive(Debug, Default)]
pub(crate) struct X52dWatch {
    running: Option<bool>,
}

impl X52dWatch {
    /// Records the result of a check; returns it when it differs from the last one (the first
    /// check always does).
    pub(crate) fn update(&mut self, running: bool) -> Option<bool> {
        (self.running != Some(running)).then(|| {
            self.running = Some(running);
            running
        })
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;

    use super::*;

    #[test]
    fn a_listening_socket_is_a_running_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x52d.cmd");
        let _listener = UnixListener::bind(&path).unwrap();

        assert!(is_listening(&path));
    }

    #[test]
    fn a_stale_socket_file_is_not_a_running_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x52d.cmd");
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists(), "the socket file stays behind");

        assert!(!is_listening(&path));
    }

    #[test]
    fn no_socket_is_not_a_running_daemon() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_listening(&dir.path().join("x52d.cmd")));
        let file = dir.path().join("plain");
        std::fs::write(&file, "").unwrap();
        assert!(!is_listening(&file));
    }

    #[test]
    fn only_changes_are_reported() {
        let mut watch = X52dWatch::default();
        assert_eq!(watch.update(false), Some(false), "the first check");
        assert_eq!(watch.update(false), None);
        assert_eq!(watch.update(true), Some(true));
        assert_eq!(watch.update(true), None);
        assert_eq!(watch.update(false), Some(false));
    }
}
