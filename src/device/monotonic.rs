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

//! Monotonic time for the workers' schedules: it never jumps with the wall clock and, like the
//! timeouts of `std::sync::mpsc`, does not advance while the system is suspended.

use std::time::{Duration, Instant};

/// A source of monotonic time.
pub(crate) trait Monotonic {
    /// The time elapsed since an origin fixed for the life of the source.
    fn elapsed(&self) -> Duration;
}

/// The system's monotonic clock.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SystemMonotonic {
    origin: Instant,
}

impl SystemMonotonic {
    /// A clock whose origin is now.
    #[expect(
        clippy::disallowed_methods,
        reason = "the one monotonic read: the workers' schedules come from an injected Monotonic"
    )]
    pub(crate) fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Monotonic for SystemMonotonic {
    fn elapsed(&self) -> Duration {
        self.origin.elapsed()
    }
}

#[cfg(test)]
pub(crate) use fake::FakeMonotonic;

#[cfg(test)]
mod fake {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::Monotonic;

    /// A monotonic clock moved by hand; clones share the time.
    #[derive(Debug, Clone, Default)]
    pub(crate) struct FakeMonotonic {
        elapsed: Arc<Mutex<Duration>>,
    }

    impl FakeMonotonic {
        /// Moves the time forward by `by`.
        pub(crate) fn advance(&self, by: Duration) {
            *self.elapsed.lock().expect("lock poisoned") += by;
        }

        /// Moves the time forward to `at`.
        ///
        /// # Panics
        ///
        /// If `at` is earlier than the current time: monotonic time never goes back.
        pub(crate) fn set(&self, at: Duration) {
            let mut elapsed = self.elapsed.lock().expect("lock poisoned");
            assert!(
                at >= *elapsed,
                "monotonic time going back: {elapsed:?} to {at:?}"
            );
            *elapsed = at;
        }
    }

    impl Monotonic for FakeMonotonic {
        fn elapsed(&self) -> Duration {
            *self.elapsed.lock().expect("lock poisoned")
        }
    }
}
