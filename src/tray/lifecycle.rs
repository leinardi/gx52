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

//! Which tray is the current one, and whether a tray host shows it (spike S5).
//!
//! Starting a tray is asynchronous: its handle exists only once the start completes, and the
//! tray can be turned off and on again meanwhile. Every start gets a [`TrayGeneration`]; only the
//! latest one is ever installed, an obsolete start's handle is handed back to be shut down, and
//! the reports of an obsolete tray are ignored.

use std::fmt;

/// The number of one start of the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct TrayGeneration(u64);

impl fmt::Display for TrayGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Whether a `StatusNotifierWatcher` (a tray host) shows the tray.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TrayAvailability {
    /// Nothing reported yet.
    #[default]
    Unknown,
    /// A tray host shows the icon.
    Available,
    /// No tray host, or it went away.
    Unavailable,
}

/// What became of a watcher report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WatcherReport {
    /// The report is for a tray that is no longer the current one.
    Obsolete,
    /// The availability was updated.
    Applied,
    /// The tray host that showed the icon went away.
    Lost,
}

/// The tray's state, generic over the handle so the tests use a fake one.
#[derive(Debug)]
enum State<H> {
    /// No tray.
    Off,
    /// A start is running.
    Starting(TrayGeneration),
    /// The tray runs.
    Running(TrayGeneration, H),
}

/// The current tray and its availability.
#[derive(Debug)]
pub(crate) struct TrayLifecycle<H> {
    /// The generation the next start gets.
    next: u64,
    state: State<H>,
    availability: TrayAvailability,
}

impl<H> Default for TrayLifecycle<H> {
    fn default() -> Self {
        Self {
            next: 0,
            state: State::Off,
            availability: TrayAvailability::Unknown,
        }
    }
}

impl<H> TrayLifecycle<H> {
    /// Asks for a tray: the generation to start one with, or `None` when one is already starting
    /// or running.
    pub(crate) fn enable(&mut self) -> Option<TrayGeneration> {
        if !matches!(self.state, State::Off) {
            return None;
        }
        let generation = TrayGeneration(self.next);
        self.next += 1;
        self.state = State::Starting(generation);
        // An earlier tray's reports say nothing about this one.
        self.availability = TrayAvailability::Unknown;
        Some(generation)
    }

    /// Drops the tray: the running one's handle, to be shut down. A start still running becomes
    /// obsolete, and its handle comes back through [`Self::spawned`].
    pub(crate) fn disable(&mut self) -> Option<H> {
        let state = std::mem::replace(&mut self.state, State::Off);
        self.availability = TrayAvailability::Unknown;
        match state {
            State::Running(_, handle) => Some(handle),
            State::Off | State::Starting(_) => None,
        }
    }

    /// The start of `generation` completed with `handle`: installed when it is the current start,
    /// otherwise handed back to be shut down at once.
    pub(crate) fn spawned(&mut self, generation: TrayGeneration, handle: H) -> Option<H> {
        match self.state {
            State::Starting(current) if current == generation => {
                self.state = State::Running(generation, handle);
                None
            }
            State::Off | State::Starting(_) | State::Running(..) => Some(handle),
        }
    }

    /// The start of `generation` failed: no tray until the next [`Self::enable`]. True when it was
    /// the current start.
    pub(crate) fn spawn_failed(&mut self, generation: TrayGeneration) -> bool {
        if !matches!(self.state, State::Starting(current) if current == generation) {
            return false;
        }
        self.state = State::Off;
        self.availability = TrayAvailability::Unavailable;
        true
    }

    /// Whether no tray is starting or running.
    pub(crate) fn is_off(&self) -> bool {
        matches!(self.state, State::Off)
    }

    /// Whether `generation` is the tray starting or running now, so its reports count.
    pub(crate) fn is_current(&self, generation: TrayGeneration) -> bool {
        match self.state {
            State::Starting(current) | State::Running(current, _) => current == generation,
            State::Off => false,
        }
    }

    /// The running tray's handle.
    pub(crate) fn handle(&self) -> Option<&H> {
        match &self.state {
            State::Running(_, handle) => Some(handle),
            State::Off | State::Starting(_) => None,
        }
    }

    /// Whether a tray host shows the current tray.
    #[cfg(test)]
    pub(crate) fn availability(&self) -> TrayAvailability {
        self.availability
    }

    /// `generation`'s tray registered with the watcher. `ksni` reports a missing watcher during
    /// the start, before the start returns, and never reports a present one for the first
    /// registration (S5): this comes after any such report, so only an unknown availability is
    /// filled.
    pub(crate) fn registered(&mut self, generation: TrayGeneration) -> WatcherReport {
        if !self.is_current(generation) {
            return WatcherReport::Obsolete;
        }
        if self.availability == TrayAvailability::Unknown {
            self.availability = TrayAvailability::Available;
        }
        WatcherReport::Applied
    }

    /// A tray host showed up for `generation`'s tray.
    pub(crate) fn watcher_online(&mut self, generation: TrayGeneration) -> WatcherReport {
        if !self.is_current(generation) {
            return WatcherReport::Obsolete;
        }
        self.availability = TrayAvailability::Available;
        WatcherReport::Applied
    }

    /// The tray host of `generation`'s tray went away, or there is none.
    pub(crate) fn watcher_offline(&mut self, generation: TrayGeneration) -> WatcherReport {
        if !self.is_current(generation) {
            return WatcherReport::Obsolete;
        }
        let before = std::mem::replace(&mut self.availability, TrayAvailability::Unavailable);
        if before == TrayAvailability::Available {
            WatcherReport::Lost
        } else {
            WatcherReport::Applied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A handle that says which start it came from.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct FakeHandle(u32);

    #[test]
    fn a_start_installs_its_handle_and_is_available() {
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        assert_eq!(tray.handle(), None);
        assert_eq!(tray.spawned(generation, FakeHandle(1)), None);
        assert_eq!(tray.handle(), Some(&FakeHandle(1)));
        assert_eq!(
            tray.availability(),
            TrayAvailability::Unknown,
            "a start proves nothing about a tray host"
        );
        assert_eq!(tray.registered(generation), WatcherReport::Applied);
        assert_eq!(tray.availability(), TrayAvailability::Available);
    }

    #[test]
    fn enabling_twice_starts_one_tray() {
        let mut tray = TrayLifecycle::<FakeHandle>::default();
        assert!(tray.enable().is_some());
        assert_eq!(tray.enable(), None, "starting");
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        tray.spawned(generation, FakeHandle(1));
        assert_eq!(tray.enable(), None, "running");
    }

    #[test]
    fn disabled_before_the_start_completes_shuts_the_handle_down() {
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        assert_eq!(tray.disable(), None, "no handle yet");
        assert_eq!(
            tray.spawned(generation, FakeHandle(1)),
            Some(FakeHandle(1)),
            "the late handle goes back to be shut down"
        );
        assert_eq!(tray.handle(), None);
    }

    #[test]
    fn an_obsolete_start_completing_last_leaves_the_new_tray() {
        let mut tray = TrayLifecycle::default();
        let first = tray.enable().expect("the tray is off");
        tray.disable();
        let second = tray.enable().expect("the tray is off again");
        assert_eq!(tray.spawned(second, FakeHandle(2)), None);
        assert_eq!(tray.spawned(first, FakeHandle(1)), Some(FakeHandle(1)));
        assert_eq!(tray.handle(), Some(&FakeHandle(2)));
    }

    #[test]
    fn an_obsolete_start_completing_first_is_not_installed() {
        let mut tray = TrayLifecycle::default();
        let first = tray.enable().expect("the tray is off");
        tray.disable();
        let second = tray.enable().expect("the tray is off again");
        assert_eq!(tray.spawned(first, FakeHandle(1)), Some(FakeHandle(1)));
        assert_eq!(tray.handle(), None, "the second start still runs");
        assert_eq!(tray.spawned(second, FakeHandle(2)), None);
        assert_eq!(tray.handle(), Some(&FakeHandle(2)));
    }

    #[test]
    fn disabling_a_running_tray_returns_its_handle() {
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        tray.spawned(generation, FakeHandle(1));
        assert!(!tray.is_off());
        assert_eq!(tray.disable(), Some(FakeHandle(1)));
        assert_eq!(tray.handle(), None);
        assert!(!tray.is_current(generation));
        assert!(tray.is_off());
    }

    #[test]
    fn an_offline_report_before_the_start_returns_is_kept() {
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        assert_eq!(tray.watcher_offline(generation), WatcherReport::Applied);
        tray.spawned(generation, FakeHandle(1));
        tray.registered(generation);
        assert_eq!(tray.availability(), TrayAvailability::Unavailable);
    }

    #[test]
    fn an_offline_report_applied_after_the_handle_is_no_loss() {
        // The handle and the reports reach the main thread by different paths: the start's
        // offline report can be applied after the handle is installed, never after `registered`.
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        tray.spawned(generation, FakeHandle(1));
        assert_eq!(tray.watcher_offline(generation), WatcherReport::Applied);
        tray.registered(generation);
        assert_eq!(tray.availability(), TrayAvailability::Unavailable);
    }

    #[test]
    fn only_a_shown_tray_losing_its_host_is_a_loss() {
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        tray.spawned(generation, FakeHandle(1));
        tray.registered(generation);
        assert_eq!(tray.watcher_offline(generation), WatcherReport::Lost);
        assert_eq!(tray.watcher_offline(generation), WatcherReport::Applied);
        assert_eq!(tray.watcher_online(generation), WatcherReport::Applied);
        assert_eq!(tray.availability(), TrayAvailability::Available);
        assert_eq!(tray.watcher_offline(generation), WatcherReport::Lost);
    }

    #[test]
    fn an_obsolete_trays_reports_are_ignored() {
        let mut tray = TrayLifecycle::default();
        let first = tray.enable().expect("the tray is off");
        tray.spawned(first, FakeHandle(1));
        tray.disable();
        let second = tray.enable().expect("the tray is off again");
        tray.spawned(second, FakeHandle(2));
        tray.registered(second);
        assert_eq!(tray.registered(first), WatcherReport::Obsolete);
        assert_eq!(tray.watcher_offline(first), WatcherReport::Obsolete);
        assert_eq!(tray.watcher_online(first), WatcherReport::Obsolete);
        assert_eq!(tray.availability(), TrayAvailability::Available);
        assert!(!tray.is_current(first));
        assert!(tray.is_current(second));
    }

    #[test]
    fn a_new_start_forgets_the_earlier_availability() {
        let mut tray = TrayLifecycle::default();
        let first = tray.enable().expect("the tray is off");
        tray.watcher_offline(first);
        tray.disable();
        let second = tray.enable().expect("the tray is off again");
        assert_eq!(tray.availability(), TrayAvailability::Unknown);
        tray.spawned(second, FakeHandle(2));
        tray.registered(second);
        assert_eq!(tray.availability(), TrayAvailability::Available);
    }

    #[test]
    fn a_failed_start_leaves_no_tray_until_enabled_again() {
        let mut tray = TrayLifecycle::<FakeHandle>::default();
        let first = tray.enable().expect("the tray is off");
        assert!(tray.spawn_failed(first));
        assert_eq!(tray.availability(), TrayAvailability::Unavailable);
        assert!(!tray.is_current(first));
        let second = tray.enable().expect("a failed start leaves the tray off");
        assert!(!tray.spawn_failed(first), "an old failure changes nothing");
        assert!(tray.is_current(second));
    }

    #[test]
    fn shutdown_while_starting_shuts_the_late_handle_down() {
        let mut tray = TrayLifecycle::default();
        let generation = tray.enable().expect("the tray is off");
        // The application's shutdown disables the tray with the start still running.
        assert_eq!(tray.disable(), None);
        assert_eq!(tray.spawned(generation, FakeHandle(1)), Some(FakeHandle(1)));
        assert!(!tray.is_current(generation));
    }
}
