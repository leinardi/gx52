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
//! running daemon. A daemon running as root under the default umask leaves a socket other users
//! may not write to, so their `connect` fails with `EACCES`: then its pid file decides, and without
//! a readable pid file (inside the Flatpak) the protected socket counts as a running daemon.

use std::fs;
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

/// Where `x52d` writes its pid file, next to each socket path: `$(localstatedir)/run/x52d.pid`
/// (`daemon/constants.h`).
pub(crate) const PID_FILES: [&str; 3] = [
    "/run/x52d.pid",
    "/var/run/x52d.pid",
    "/usr/local/var/run/x52d.pid",
];

/// Where the kernel lists the processes.
const PROC: &str = "/proc";

/// How often the device watch checks for the daemon.
pub(crate) const CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// Whether `x52d` is running, at any of its default socket paths.
pub(crate) fn is_running() -> bool {
    SOCKET_PATHS
        .iter()
        .zip(PID_FILES)
        .any(|(socket, pid_file)| runs_at(Path::new(socket), Path::new(pid_file), Path::new(PROC)))
}

/// Whether a daemon runs behind the socket at `socket`, with `pid_file` and the process list at
/// `proc` to decide when the socket refuses this user.
pub(crate) fn runs_at(socket: &Path, pid_file: &Path, proc: &Path) -> bool {
    match probe(socket) {
        Socket::Listening => true,
        Socket::Absent => false,
        Socket::Protected => match fs::read_to_string(pid_file) {
            Ok(text) => match text.trim().parse::<u32>() {
                Ok(pid) => fs::read_to_string(proc.join(pid.to_string()).join("comm"))
                    .is_ok_and(|comm| comm.trim() == "x52d"),
                // An unreadable pid cannot rule the daemon out.
                Err(_) => true,
            },
            // No pid file to look at (inside the Flatpak): the protected socket decides.
            Err(_) => true,
        },
    }
}

/// What a `connect` to a Unix socket found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Socket {
    /// Something accepts connections.
    Listening,
    /// The socket exists, but this user may not connect to it (`EACCES`).
    Protected,
    /// No socket, or nobody listening on it (a stale file).
    Absent,
}

/// Connects to the Unix socket at `path`.
///
/// The socket is nonblocking, so a daemon whose backlog is full (`EAGAIN`) never blocks the
/// caller; it is listening all the same.
pub(crate) fn probe(path: &Path) -> Socket {
    let Ok(address) = SocketAddrUnix::new(path) else {
        return Socket::Absent;
    };
    let Ok(socket) = net::socket_with(
        AddressFamily::UNIX,
        SocketType::STREAM,
        SocketFlags::NONBLOCK | SocketFlags::CLOEXEC,
        None,
    ) else {
        return Socket::Absent;
    };
    match net::connect(&socket, &address) {
        Ok(()) | Err(Errno::AGAIN | Errno::INPROGRESS) => Socket::Listening,
        Err(Errno::ACCESS | Errno::PERM) => Socket::Protected,
        Err(_) => Socket::Absent,
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
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn a_listening_socket_is_a_running_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x52d.cmd");
        let _listener = UnixListener::bind(&path).unwrap();

        assert_eq!(probe(&path), Socket::Listening);
    }

    #[test]
    fn a_stale_socket_file_is_not_a_running_daemon() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x52d.cmd");
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists(), "the socket file stays behind");

        assert_eq!(probe(&path), Socket::Absent);
    }

    #[test]
    fn no_socket_is_not_a_running_daemon() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(probe(&dir.path().join("x52d.cmd")), Socket::Absent);
        let file = dir.path().join("plain");
        std::fs::write(&file, "").unwrap();
        assert_eq!(probe(&file), Socket::Absent);
    }

    /// A listening socket this user may not connect to, as root's daemon leaves it; `None` when
    /// the tests run as root, who connects anyway.
    fn protected_socket(dir: &Path) -> Option<(PathBuf, UnixListener)> {
        let path = dir.join("x52d.cmd");
        let listener = UnixListener::bind(&path).unwrap();
        fs::set_permissions(&path, Permissions::from_mode(0o555)).unwrap();
        (probe(&path) == Socket::Protected).then_some((path, listener))
    }

    /// A process list at `dir/proc` with `pid` running `comm`.
    fn proc_with(dir: &Path, pid: u32, comm: &str) -> PathBuf {
        let proc = dir.join("proc");
        fs::create_dir_all(proc.join(pid.to_string())).unwrap();
        fs::write(proc.join(pid.to_string()).join("comm"), format!("{comm}\n")).unwrap();
        proc
    }

    #[test]
    fn a_protected_socket_with_the_daemon_in_its_pid_file_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let Some((socket, _listener)) = protected_socket(dir.path()) else {
            return;
        };
        let pid_file = dir.path().join("x52d.pid");
        fs::write(&pid_file, "1234\n").unwrap();

        assert!(runs_at(
            &socket,
            &pid_file,
            &proc_with(dir.path(), 1234, "x52d")
        ));
    }

    #[test]
    fn a_protected_socket_whose_pid_is_another_process_is_stale() {
        let dir = tempfile::tempdir().unwrap();
        let Some((socket, _listener)) = protected_socket(dir.path()) else {
            return;
        };
        let pid_file = dir.path().join("x52d.pid");
        fs::write(&pid_file, "1234\n").unwrap();

        assert!(!runs_at(
            &socket,
            &pid_file,
            &proc_with(dir.path(), 1234, "bash")
        ));
        assert!(!runs_at(&socket, &pid_file, &dir.path().join("empty-proc")));
    }

    #[test]
    fn a_protected_socket_without_a_pid_file_is_running() {
        let dir = tempfile::tempdir().unwrap();
        let Some((socket, _listener)) = protected_socket(dir.path()) else {
            return;
        };

        assert!(runs_at(
            &socket,
            &dir.path().join("x52d.pid"),
            &dir.path().join("proc")
        ));
    }

    #[test]
    fn a_listening_or_absent_socket_needs_no_pid_file() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("x52d.cmd");
        let pid_file = dir.path().join("x52d.pid");
        let proc = dir.path().join("proc");
        assert!(!runs_at(&socket, &pid_file, &proc));
        let _listener = UnixListener::bind(&socket).unwrap();
        assert!(runs_at(&socket, &pid_file, &proc));
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
