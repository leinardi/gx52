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

//! The device check harness: `gx52 --device-check <scenario>`, Devel builds only.
//!
//! It runs the device service without a window, sets the states a scenario needs, detects the
//! physical events by itself and prints timestamped lines: `STEP` (something the user must do),
//! `CHECK` (something to confirm by looking), `PASS` and `FAIL` (what it verified itself),
//! `INFO`, and `SEND` for every command the writer sends. It ends with a summary.

use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use async_io::Timer;
use futures_lite::future;
use jiff::tz::TimeZone;
use x52::charset::MfdText;
use x52::clock::{Clock, DateFormat, FixedOffset, ZoneSource};
use x52::command::{Brightness, Command, CommandSlot, OffsetClock};
use x52::device::{BiColourLed, DeviceType, LedColour, SingleColourLed};
use x52::input::Mode;
use x52::state::DeviceState;

use super::access::{self, AccessState, UinputAccess};
use super::id::{DeviceId, Generation};
use super::monotonic::{Monotonic, SystemMonotonic};
use super::writer::WAKE_INTERVAL;
use super::{DeviceEvent, DeviceService};

/// The command-line option.
pub(crate) const OPTION: &str = "--device-check";

/// The udev rule the permission scenario moves away and back.
const RULE: &str = "/etc/udev/rules.d/70-gx52.rules";

/// Where the permission scenario parks the rule.
const RULE_DISABLED: &str = "/etc/udev/rules.d/70-gx52.rules.disabled";

/// How often the resume scenario compares the wall clock with monotonic time.
const TICK: Duration = Duration::from_secs(1);

/// A wall-clock jump larger than this, with no monotonic time passing, is a suspend.
const JUMP: Duration = Duration::from_secs(30);

/// How long the permission scenario lets udev settle before it looks at the node again.
const PERMISSION_SETTLE: Duration = Duration::from_secs(1);

/// How long the mode scenario keeps listening after the selector is back on 1.
const SETTLE: Duration = Duration::from_millis(1500);

/// A device check scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scenario {
    /// Selection, full replay, access, evdev node and initial mode, with no user action.
    Smoke,
    /// Unplug and plug back in: the full replay of a state set while unplugged.
    Replug,
    /// The mode selector turned 1-2-3-1.
    Mode,
    /// The udev rule removed (with a replug) and restored (without one).
    Permission,
    /// A suspend of more than a minute.
    Resume,
    /// Clocks 2 and 3 across midnight.
    Midnight,
    /// libx52's `x52d` detected, started and stopped.
    X52d,
    /// The uinput access state.
    Uinput,
}

impl Scenario {
    /// Every scenario, in the order of the plan.
    pub(crate) const ALL: [Self; 8] = [
        Self::Smoke,
        Self::Replug,
        Self::Mode,
        Self::Permission,
        Self::Resume,
        Self::Midnight,
        Self::X52d,
        Self::Uinput,
    ];

    /// The name on the command line.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Smoke => "smoke",
            Self::Replug => "replug",
            Self::Mode => "mode",
            Self::Permission => "permission",
            Self::Resume => "resume",
            Self::Midnight => "midnight",
            Self::X52d => "x52d",
            Self::Uinput => "uinput",
        }
    }
}

impl FromStr for Scenario {
    type Err = ArgsError;

    fn from_str(name: &str) -> Result<Self, ArgsError> {
        Self::ALL
            .into_iter()
            .find(|scenario| scenario.name() == name)
            .ok_or_else(|| ArgsError::UnknownScenario(name.to_owned()))
    }
}

/// Why the command line was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ArgsError {
    /// The option exists only in Devel builds.
    ReleaseBuild,
    /// No scenario after the option.
    MissingScenario,
    /// A scenario that does not exist.
    UnknownScenario(String),
}

impl fmt::Display for ArgsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let names: Vec<&str> = Scenario::ALL
            .iter()
            .map(|scenario| scenario.name())
            .collect();
        match self {
            Self::ReleaseBuild => write!(
                formatter,
                "{OPTION} is only available in development builds"
            ),
            Self::MissingScenario => {
                write!(formatter, "{OPTION} needs a scenario: {}", names.join(", "))
            }
            Self::UnknownScenario(name) => write!(
                formatter,
                "unknown scenario {name:?}; the scenarios are {}",
                names.join(", ")
            ),
        }
    }
}

/// The scenario asked for in `args` (without the program name), if any; `devel` is whether this
/// is a Devel build.
///
/// # Errors
///
/// [`ArgsError`] for the option in a release build, without a scenario or with an unknown one.
pub(crate) fn parse_args(args: &[String], devel: bool) -> Result<Option<Scenario>, ArgsError> {
    let Some(position) = args
        .iter()
        .position(|arg| arg == OPTION || arg.starts_with(&format!("{OPTION}=")))
    else {
        return Ok(None);
    };
    if !devel {
        return Err(ArgsError::ReleaseBuild);
    }
    let name = match args[position].split_once('=') {
        Some((_, name)) => Some(name),
        None => args.get(position + 1).map(String::as_str),
    };
    match name {
        Some(name) if !name.is_empty() => name.parse().map(Some),
        Some(_) | None => Err(ArgsError::MissingScenario),
    }
}

/// How a check ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Every verification passed.
    Passed,
    /// A verification failed.
    Failed,
    /// The scenario could not be run as written (for the permission check, a node that stays
    /// writable): not a failure of the app.
    PreconditionFailed,
}

impl Outcome {
    /// The process exit code: 0, 1, or 3 for a precondition (2 is a command-line error).
    pub(crate) const fn exit_code(self) -> u8 {
        match self {
            Self::Passed => 0,
            Self::Failed => 1,
            Self::PreconditionFailed => 3,
        }
    }
}

/// What the harness asks of the system, so the tests can replace it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Env {
    /// Whether the user may write the USB node at this path (`access(W_OK)`).
    pub(crate) node_writable: fn(&Path) -> bool,
    /// The uinput access state.
    pub(crate) uinput: fn() -> UinputAccess,
    /// Whether libx52's `x52d` is installed.
    pub(crate) x52d_installed: fn() -> bool,
    /// The system time zone.
    pub(crate) local_zone: fn() -> TimeZone,
    /// How long to wait for the user to do a `STEP`.
    pub(crate) step_timeout: Duration,
    /// How long to wait for the service to do something by itself.
    pub(crate) auto_timeout: Duration,
}

impl Env {
    /// The real system.
    pub(crate) fn system() -> Self {
        Self {
            node_writable: |path| rustix::fs::access(path, rustix::fs::Access::WRITE_OK).is_ok(),
            uinput: access::check_uinput,
            x52d_installed: || {
                ["/usr/bin/x52d", "/usr/local/bin/x52d"]
                    .iter()
                    .any(|path| Path::new(path).exists())
            },
            local_zone: TimeZone::system,
            step_timeout: Duration::from_secs(300),
            auto_timeout: Duration::from_secs(10),
        }
    }
}

/// Runs `scenario` on the real device and returns the exit code.
pub(crate) fn run_on_device(scenario: Scenario, out: &mut dyn Write) -> u8 {
    let clock = x52::clock::SystemClock;
    let (service, events) = match DeviceService::start(clock) {
        Ok(started) => started,
        Err(error) => {
            let _ = writeln!(out, "cannot start the device service: {error}");
            return Outcome::Failed.exit_code();
        }
    };
    let monotonic = SystemMonotonic::new();
    let outcome = run(
        scenario,
        &service,
        events,
        &clock,
        &monotonic,
        &Env::system(),
        out,
    );
    service.shutdown();
    outcome.exit_code()
}

/// Runs `scenario` against `service`, reading its `events`, and reports to `out`.
pub(crate) fn run(
    scenario: Scenario,
    service: &DeviceService,
    events: async_channel::Receiver<DeviceEvent>,
    clock: &dyn Clock,
    monotonic: &dyn Monotonic,
    env: &Env,
    out: &mut dyn Write,
) -> Outcome {
    let mut session = Session {
        service,
        events,
        clock,
        monotonic,
        env,
        out,
        seen: Seen::default(),
        passed: 0,
        failed: 0,
        precondition_failed: false,
    };
    session.info(&format!("device check: {}", scenario.name()));
    match scenario {
        Scenario::Smoke => session.smoke(),
        Scenario::Replug => session.replug(),
        Scenario::Mode => session.mode(),
        Scenario::Permission => {
            session.print_restore("If this check is interrupted, restore the rule with:");
            session.permission();
            session.print_restore("Restore the rule, if it is not back yet, with:");
        }
        Scenario::Resume => session.resume(),
        Scenario::Midnight => session.midnight(),
        Scenario::X52d => session.x52d(),
        Scenario::Uinput => session.uinput(),
    }
    session.summary()
}

/// What the harness has seen of the service so far.
#[derive(Debug, Default)]
struct Seen {
    selected: Option<DeviceId>,
    gone: Vec<Generation>,
    access: Option<AccessState>,
    node: Option<PathBuf>,
    mode: Option<Mode>,
    modes: Vec<Mode>,
    /// Every command sent, with the monotonic time it was seen.
    sent: Vec<(Duration, Command)>,
    errors: Vec<String>,
    x52d: Option<bool>,
    closed: bool,
}

struct Session<'run> {
    service: &'run DeviceService,
    events: async_channel::Receiver<DeviceEvent>,
    clock: &'run dyn Clock,
    monotonic: &'run dyn Monotonic,
    env: &'run Env,
    out: &'run mut dyn Write,
    seen: Seen,
    passed: u32,
    failed: u32,
    precondition_failed: bool,
}

/// What a wait for the next event ended with.
enum Next {
    Event(DeviceEvent),
    Closed,
    Timeout,
}

impl Session<'_> {
    // Reporting.

    fn line(&mut self, kind: &str, text: &str) {
        let time = self
            .clock
            .now()
            .to_zoned((self.env.local_zone)())
            .strftime("%H:%M:%S%.3f")
            .to_string();
        // The report is the harness's output; a closed stdout leaves nothing to report to.
        let _ = writeln!(self.out, "{time} {kind:<5} {text}");
    }

    fn info(&mut self, text: &str) {
        self.line("INFO", text);
    }

    fn step(&mut self, text: &str) {
        self.line("STEP", text);
    }

    fn check(&mut self, text: &str) {
        self.line("CHECK", text);
    }

    fn pass(&mut self, text: &str) {
        self.passed += 1;
        self.line("PASS", text);
    }

    fn fail(&mut self, text: &str) {
        self.failed += 1;
        self.line("FAIL", text);
    }

    /// Passes or fails `text` by `ok`; returns `ok`.
    fn verify(&mut self, ok: bool, text: &str) -> bool {
        if ok {
            self.pass(text);
        } else {
            self.fail(text);
        }
        ok
    }

    /// Follows the events until `done` holds or `timeout` passes, then passes or fails `text`.
    fn verify_wait(
        &mut self,
        timeout: Duration,
        done: impl FnMut(&Seen) -> bool,
        text: &str,
    ) -> bool {
        let ok = self.wait_until(timeout, done);
        self.verify(ok, text)
    }

    fn print_restore(&mut self, heading: &str) {
        self.info(heading);
        self.info(&format!(
            "  sudo mv {RULE_DISABLED} {RULE} && sudo udevadm control --reload && sudo udevadm trigger"
        ));
    }

    fn summary(&mut self) -> Outcome {
        let outcome = if self.precondition_failed {
            Outcome::PreconditionFailed
        } else if self.failed > 0 {
            Outcome::Failed
        } else {
            Outcome::Passed
        };
        let text = format!(
            "{} passed, {} failed{}",
            self.passed,
            self.failed,
            if self.precondition_failed {
                ", precondition not met"
            } else {
                ""
            }
        );
        self.line("DONE", &text);
        outcome
    }

    // Following the service.

    /// Waits up to `timeout` for the next event and records it.
    fn next(&mut self, timeout: Duration) -> bool {
        let events = &self.events;
        let next = future::block_on(future::or(
            async {
                match events.recv().await {
                    Ok(event) => Next::Event(event),
                    Err(_) => Next::Closed,
                }
            },
            async {
                Timer::after(timeout).await;
                Next::Timeout
            },
        ));
        match next {
            Next::Event(event) => {
                self.observe(event);
                true
            }
            Next::Closed => {
                self.seen.closed = true;
                false
            }
            Next::Timeout => false,
        }
    }

    fn observe(&mut self, event: DeviceEvent) {
        match event {
            DeviceEvent::Selected(device) => self.seen.selected = Some(device),
            DeviceEvent::Gone(generation) => {
                self.seen.gone.push(generation);
                if self
                    .seen
                    .selected
                    .as_ref()
                    .is_some_and(|device| device.generation == generation)
                {
                    self.seen.selected = None;
                }
            }
            DeviceEvent::Access(access) => {
                self.seen.access = Some(access);
                self.info(&format!("access: {access:?}"));
            }
            DeviceEvent::InputNode(node) => {
                if node.is_none() {
                    self.seen.mode = None;
                }
                self.seen.node = node;
            }
            DeviceEvent::Mode(mode) => {
                self.seen.modes.push(mode);
                self.seen.mode = Some(mode);
            }
            DeviceEvent::X52dRunning(running) => self.seen.x52d = Some(running),
            DeviceEvent::Error(error) => {
                self.info(&format!("error: {error}"));
                self.seen.errors.push(error);
            }
            DeviceEvent::Sent(command) => {
                let transfers: Vec<String> = command
                    .encode()
                    .iter()
                    .map(|transfer| format!("{:#04x}={:#06x}", transfer.index(), transfer.value()))
                    .collect();
                self.line(
                    "SEND",
                    &format!("{}: {}", command.name(), transfers.join(" ")),
                );
                self.seen.sent.push((self.monotonic.elapsed(), command));
            }
        }
    }

    /// Follows the events until `done` holds or `timeout` passes; returns whether it held.
    fn wait_until(&mut self, timeout: Duration, mut done: impl FnMut(&Seen) -> bool) -> bool {
        let start = self.monotonic.elapsed();
        loop {
            if done(&self.seen) {
                return true;
            }
            let waited = self.monotonic.elapsed().saturating_sub(start);
            let Some(remaining) = timeout.checked_sub(waited).filter(|left| !left.is_zero()) else {
                return false;
            };
            if !self.next(remaining) && self.seen.closed {
                return false;
            }
        }
    }

    /// Records every event for `duration`.
    fn listen(&mut self, duration: Duration) {
        self.wait_until(duration, |_| false);
    }

    // Common steps.

    /// Waits for a selection, sets the state `state_for` gives its type and verifies the device
    /// opens and gets the full replay.
    fn connect(
        &mut self,
        state_for: impl Fn(DeviceType) -> DeviceState,
    ) -> Option<(DeviceId, DeviceState)> {
        let timeout = self.env.auto_timeout;
        if !self.wait_until(timeout, |seen| seen.selected.is_some()) {
            self.fail("no X52 joystick selected: is it plugged in?");
            return None;
        }
        let device = self.seen.selected.clone()?;
        self.pass(&format!(
            "selected the {} at {} (generation {})",
            device.device_type,
            device.sysfs_path.display(),
            device.generation
        ));
        let state = state_for(device.device_type);
        let mark = self.seen.sent.len();
        let errors = self.seen.errors.len();
        self.service.set_state(state.clone());
        if !self.wait_access(AccessState::Ready, timeout) {
            return None;
        }
        self.verify_replay(&device, &state, mark, errors);
        Some((device, state))
    }

    /// Waits for `access` and passes or fails.
    fn wait_access(&mut self, access: AccessState, timeout: Duration) -> bool {
        let reached = self.wait_until(timeout, |seen| seen.access == Some(access));
        let actual = self.seen.access;
        self.verify(
            reached,
            &format!("access state {access:?} (seen {actual:?})"),
        )
    }

    /// Verifies the commands sent since `mark` are the full replay of `state`, with no error
    /// since `errors`.
    fn verify_replay(
        &mut self,
        device: &DeviceId,
        state: &DeviceState,
        mark: usize,
        errors: usize,
    ) {
        let expected = match state.resolve(
            device.device_type,
            self.clock.now(),
            &(self.env.local_zone)(),
        ) {
            Ok(commands) => commands,
            Err(error) => {
                self.fail(&format!("cannot resolve the state: {error}"));
                return;
            }
        };
        let count = expected.len();
        let timeout = self.env.auto_timeout;
        self.wait_until(timeout, |seen| seen.sent.len() >= mark + count);
        let sent: Vec<Command> = self.seen.sent[mark..]
            .iter()
            .take(count)
            .map(|(_, command)| *command)
            .collect();
        let transfers: usize = expected.iter().map(|command| command.encode().len()).sum();
        match replay_matches(&sent, &expected) {
            Ok(()) => self.pass(&format!(
                "full replay: {count} commands, {transfers} transfers"
            )),
            Err(problem) => self.fail(&format!("full replay: {problem}")),
        }
        let new_errors = self.seen.errors.len() - errors;
        self.verify(
            new_errors == 0,
            &format!("every transfer succeeded ({new_errors} errors)"),
        );
    }

    // Scenarios.

    fn smoke(&mut self) {
        if self.connect(DeviceState::default_for).is_none() {
            return;
        }
        let timeout = self.env.auto_timeout;
        let found = self.wait_until(timeout, |seen| seen.node.is_some());
        let node = self.seen.node.clone();
        if !self.verify(
            found,
            &format!("evdev node found ({})", display(node.as_deref())),
        ) {
            return;
        }
        let read = self.wait_until(timeout, |seen| seen.mode.is_some());
        let mode = self.seen.mode;
        self.verify(read, &format!("initial mode read ({})", mode_text(mode)));
    }

    fn replug(&mut self) {
        let Some((device, _)) = self.connect(state_a) else {
            return;
        };
        self.step("Unplug the joystick.");
        let generation = device.generation;
        let step = self.env.step_timeout;
        if !self.verify_wait(
            step,
            |seen| seen.gone.contains(&generation),
            "the joystick is gone",
        ) {
            return;
        }
        let state = state_b(device.device_type);
        self.service.set_state(state.clone());
        self.info("state B set while unplugged");
        let mark = self.seen.sent.len();
        let errors = self.seen.errors.len();
        self.step("Plug the joystick back in.");
        let selected = self.wait_until(step, |seen| {
            seen.selected
                .as_ref()
                .is_some_and(|selected| selected.generation > generation)
        });
        if !self.verify(
            selected,
            "the joystick is selected again (a new generation)",
        ) {
            return;
        }
        let Some(device) = self.seen.selected.clone() else {
            return;
        };
        if !self.wait_access(AccessState::Ready, self.env.auto_timeout) {
            return;
        }
        self.verify_replay(&device, &state, mark, errors);
        self.check_state_b(&device, &state);
    }

    fn check_state_b(&mut self, device: &DeviceId, state: &DeviceState) {
        self.check("The MFD backlight is on at about half brightness.");
        if device.device_type.has_leds() {
            self.check("The throttle LEDs are lit.");
            self.check(
                "The LEDs: A green, B red, D amber, E off, T1/T2 green, T3/T4 red, T5/T6 amber, \
                 POV 2 green, i red, Fire lit.",
            );
        } else {
            self.check("The throttle is lit.");
        }
        self.check("The MFD reads \"STATE B\", \"LINE 2 OK\", \"LINE 3 OK\".");
        match state.resolve(
            device.device_type,
            self.clock.now(),
            &(self.env.local_zone)(),
        ) {
            Ok(commands) => {
                let shown = ClockDisplay::of(&commands);
                self.check(&format!(
                    "Clock 1 shows {} (24 h), clock 2 {}, clock 3 {}, the date {} (dd-mm-yy).",
                    shown.clock1, shown.clock2, shown.clock3, shown.date
                ));
            }
            Err(error) => self.fail(&format!("cannot resolve the clocks: {error}")),
        }
    }

    fn mode(&mut self) {
        if self.connect(DeviceState::default_for).is_none() {
            return;
        }
        let timeout = self.env.auto_timeout;
        if !self.verify_wait(timeout, |seen| seen.mode.is_some(), "initial mode read") {
            return;
        }
        let step = self.env.step_timeout;
        if self.seen.mode != Some(Mode::One) {
            self.step("Turn the mode selector to 1.");
            if !self.verify_wait(step, |seen| seen.mode == Some(Mode::One), "mode 1") {
                return;
            }
        }
        let mark = self.seen.modes.len();
        self.step(
            "Turn the mode selector to 2, then 3, then back to 1, a second at each position.",
        );
        self.wait_until(step, |seen| {
            let changes = &seen.modes[mark..];
            changes.len() >= 3 && changes.last() == Some(&Mode::One)
        });
        self.listen(SETTLE);
        let changes = self.seen.modes[mark..].to_vec();
        self.verify(
            changes == [Mode::Two, Mode::Three, Mode::One],
            &format!(
                "one change per position, none for the 2 passed through from 3 to 1 (seen {})",
                modes_text(&changes)
            ),
        );
    }

    fn permission(&mut self) {
        let Some((device, state)) = self.connect(DeviceState::default_for) else {
            return;
        };
        self.step(&format!(
            "Disable the udev rule: sudo mv {RULE} {RULE_DISABLED} && sudo udevadm control --reload \
             && sudo udevadm trigger"
        ));
        self.step("Then unplug the joystick and plug it back in.");
        let step = self.env.step_timeout;
        let generation = device.generation;
        let replugged = self.wait_until(step, |seen| {
            seen.selected
                .as_ref()
                .is_some_and(|selected| selected.generation > generation)
        });
        if !self.verify(replugged, "the joystick was unplugged and selected again") {
            return;
        }
        let Some(device) = self.seen.selected.clone() else {
            return;
        };
        let node = usb_node(&device);
        if self.node_still_writable(&node) {
            return;
        }
        let denied = self.wait_until(self.env.auto_timeout, |seen| {
            seen.access == Some(AccessState::NoPermission)
        });
        // The sandbox polls the device list and can see the device before udev has applied
        // the ACL; looked at again once the access state has settled, the node must still be
        // closed to the user.
        self.listen(PERMISSION_SETTLE);
        if self.node_still_writable(&node) {
            return;
        }
        self.pass(&format!("precondition: {} is not writable", node.display()));
        let access = self.seen.access;
        if !self.verify(
            denied,
            &format!("access state NoPermission (seen {access:?})"),
        ) {
            return;
        }
        let mark = self.seen.sent.len();
        let errors = self.seen.errors.len();
        self.step(&format!(
            "Restore the rule WITHOUT unplugging: sudo mv {RULE_DISABLED} {RULE} && sudo udevadm \
             control --reload && sudo udevadm trigger"
        ));
        if !self.wait_access(AccessState::Ready, step) {
            return;
        }
        let same = self
            .seen
            .selected
            .as_ref()
            .is_some_and(|selected| selected.generation == device.generation);
        self.verify(
            same,
            "ready again through the periodic retry, with no replug (same generation)",
        );
        self.verify_replay(&device, &state, mark, errors);
    }

    /// Whether the USB node at `node` is writable for the user, in which case the rule is still
    /// in effect: a precondition failure, reported as such.
    fn node_still_writable(&mut self, node: &Path) -> bool {
        if !(self.env.node_writable)(node) {
            return false;
        }
        self.precondition_failed = true;
        self.line(
            "FAIL",
            &format!(
                "precondition: {} is still writable, so the rule is still in effect; this is not \
                 a failure of GX52",
                node.display()
            ),
        );
        true
    }

    fn resume(&mut self) {
        if self.connect(DeviceState::default_for).is_none() {
            return;
        }
        self.step("Suspend the computer (systemctl suspend) and wake it after more than a minute.");
        let step = self.env.step_timeout;
        let start = self.monotonic.elapsed();
        let mut last = (self.clock.now(), self.monotonic.elapsed());
        let (resumed_after, jumped_to) = loop {
            if self.monotonic.elapsed().saturating_sub(start) > step {
                self.fail("no suspend detected");
                return;
            }
            self.listen(TICK);
            let now = (self.clock.now(), self.monotonic.elapsed());
            let wall: Duration = last.0.duration_until(now.0).try_into().unwrap_or_default();
            let monotonic = now.1.saturating_sub(last.1);
            if wall.saturating_sub(monotonic) > JUMP {
                break (last.1, now.0);
            }
            last = now;
        };
        self.pass("suspend detected: the wall clock jumped");
        let timeout = self.env.auto_timeout;
        // A clock 1 sent between the last tick and the suspend still shows the time before the
        // jump; the re-send after the resume shows the minute the jump was detected in, or the
        // one after it.
        let after_jump = jumped_to.to_zoned((self.env.local_zone)());
        let after_jump = i32::from(after_jump.hour()) * 60 + i32::from(after_jump.minute());
        let resent = |seen: &Seen| {
            seen.sent
                .iter()
                .find(|(at, command)| {
                    *at >= resumed_after
                        && clock1_minute(command)
                            .is_some_and(|minute| (minute - after_jump).rem_euclid(1440) <= 1)
                })
                .map(|(at, _)| at.saturating_sub(resumed_after))
        };
        self.wait_until(timeout, |seen| resent(seen).is_some());
        let limit = WAKE_INTERVAL + TICK;
        match resent(&self.seen) {
            Some(after) => {
                self.verify(
                    after <= limit,
                    &format!(
                        "clock 1 re-sent {:.1} s after the resume (at most {} s, the wake \
                         interval plus the detection tick)",
                        after.as_secs_f64(),
                        limit.as_secs()
                    ),
                );
            }
            None => self.fail("clock 1 was not re-sent after the resume"),
        }
        let now = self.clock.now().to_zoned((self.env.local_zone)());
        self.check(&format!(
            "The MFD clock 1 shows {} (the current time).",
            now.strftime("%H:%M")
        ));
    }

    fn midnight(&mut self) {
        let Some((device, mut state)) = self.connect(DeviceState::default_for) else {
            return;
        };
        let now = self.clock.now().to_zoned((self.env.local_zone)());
        let minute = i32::from(now.hour()) * 60 + i32::from(now.minute());
        let local_offset = now.offset().seconds() / 60;
        let plan = MidnightPlan::new(minute);
        for (clock, offset) in [
            (&mut state.clock2, plan.positive),
            (&mut state.clock3, plan.negative),
        ] {
            let Some(fixed) = fixed_offset_for(offset, local_offset) else {
                self.fail(&format!(
                    "no fixed offset gives {offset} minutes from clock 1"
                ));
                return;
            };
            clock.source = ZoneSource::Fixed(fixed);
        }
        let mark = self.seen.sent.len();
        self.service.set_state(state);
        let timeout = self.env.auto_timeout;
        let offsets = |seen: &Seen| -> Option<(i16, i16)> {
            let offset_of = |wanted: OffsetClock| {
                seen.sent[mark..].iter().rev().find_map(|(_, command)| {
                    if let Command::ClockOffset { clock, offset, .. } = command
                        && *clock == wanted
                    {
                        Some(offset.minutes())
                    } else {
                        None
                    }
                })
            };
            Some((
                offset_of(OffsetClock::Clock2)?,
                offset_of(OffsetClock::Clock3)?,
            ))
        };
        self.wait_until(timeout, |seen| offsets(seen).is_some());
        let sent = offsets(&self.seen);
        let want = (
            i16::try_from(plan.positive).unwrap_or_default(),
            i16::try_from(plan.negative).unwrap_or_default(),
        );
        self.verify(
            sent == Some(want),
            &format!(
                "clock offsets sent: {want:?} minutes from clock 1 (seen {sent:?}) on the {}",
                device.device_type
            ),
        );
        for (name, offset) in [("2", plan.positive), ("3", plan.negative)] {
            let (shown, crossed) = MidnightPlan::shown(minute, offset);
            self.check(&format!(
                "Clock {name} shows {shown} ({offset:+} minutes from clock 1{}).",
                if crossed {
                    ", across midnight"
                } else {
                    ", the same day"
                }
            ));
        }
        if !(plan.positive_crosses() && plan.negative_crosses()) {
            self.info(
                "Only one direction crosses midnight at this time of day; run the check again in \
                 the other half of the day for the other one.",
            );
        }
    }

    fn x52d(&mut self) {
        let timeout = self.env.auto_timeout;
        if !self.wait_until(timeout, |seen| seen.x52d.is_some()) {
            self.fail("the device watch did not report x52d");
            return;
        }
        let running = self.seen.x52d == Some(true);
        self.info(&format!("x52d running: {running}"));
        // A running daemon is installed wherever its binary is, also outside the paths the
        // harness can see (inside the Flatpak, /usr is the runtime's).
        if !running && !(self.env.x52d_installed)() {
            self.info("libx52's x52d is not installed: not checked with the daemon");
            return;
        }
        let step = self.env.step_timeout;
        for wanted in [!running, running] {
            let verb = if wanted { "start" } else { "stop" };
            self.step(&format!(
                "{} x52d: sudo systemctl {verb} x52d",
                capitalised(verb)
            ));
            self.verify_wait(
                step,
                |seen| seen.x52d == Some(wanted),
                &format!(
                    "x52d detected as {}",
                    if wanted { "running" } else { "stopped" }
                ),
            );
        }
    }

    fn uinput(&mut self) {
        let access = (self.env.uinput)();
        if access == UinputAccess::Available {
            self.pass("uinput: available");
        } else {
            self.info(&format!(
                "uinput: {access:?} (the virtual mouse needs the second udev rule line)"
            ));
        }
    }
}

/// Compares a replay with the resolved state, command by command; the clocks and the date may
/// differ in value, since time can pass while the replay is compared.
pub(crate) fn replay_matches(sent: &[Command], expected: &[Command]) -> Result<(), String> {
    if sent.len() != expected.len() {
        return Err(format!(
            "{} commands sent, {} expected",
            sent.len(),
            expected.len()
        ));
    }
    for (position, (sent, expected)) in sent.iter().zip(expected).enumerate() {
        let timed = matches!(
            expected.slot(),
            CommandSlot::Clock1 | CommandSlot::ClockOffset(_) | CommandSlot::Date
        );
        if sent.slot() != expected.slot() || (!timed && sent != expected) {
            return Err(format!(
                "command {position}: sent {sent:?}, expected {expected:?}"
            ));
        }
    }
    Ok(())
}

/// The minute of the day a clock 1 command shows.
fn clock1_minute(command: &Command) -> Option<i32> {
    if let Command::Clock1 { time, .. } = command {
        Some(i32::from(time.hour()) * 60 + i32::from(time.minute()))
    } else {
        None
    }
}

/// The USB node of `device`, `/dev/bus/usb/BBB/DDD`.
fn usb_node(device: &DeviceId) -> PathBuf {
    PathBuf::from(format!(
        "/dev/bus/usb/{:03}/{:03}",
        device.attachment.bus, device.attachment.address
    ))
}

/// The state set before the replug: everything red, a dim backlight.
fn state_a(device_type: DeviceType) -> DeviceState {
    let mut state = DeviceState::default_for(device_type);
    state.mfd_brightness = brightness(16);
    if device_type.has_leds() {
        state.leds = x52::state::Leds::uniform(LedColour::Red, true);
    }
    state.mfd_lines = [text("STATE A"), MfdText::EMPTY, MfdText::EMPTY];
    state
}

/// The state set while unplugged, distinct in every field the user checks.
fn state_b(device_type: DeviceType) -> DeviceState {
    let mut state = DeviceState::default_for(device_type);
    state.mfd_brightness = brightness(64);
    state.led_brightness = brightness(Brightness::MAX);
    if device_type.has_leds() {
        for (led, colour) in [
            (BiColourLed::ButtonA, LedColour::Green),
            (BiColourLed::ButtonB, LedColour::Red),
            (BiColourLed::ButtonD, LedColour::Amber),
            (BiColourLed::ButtonE, LedColour::Off),
            (BiColourLed::T1T2, LedColour::Green),
            (BiColourLed::T3T4, LedColour::Red),
            (BiColourLed::T5T6, LedColour::Amber),
            (BiColourLed::Pov2, LedColour::Green),
            (BiColourLed::ButtonI, LedColour::Red),
        ] {
            state.leds.set_colour(led, colour);
        }
        state.leds.set_on(SingleColourLed::Fire, true);
        state.leds.set_on(SingleColourLed::Throttle, true);
    }
    state.mfd_lines = [text("STATE B"), text("LINE 2 OK"), text("LINE 3 OK")];
    state.date_format = DateFormat::DdMmYy;
    state
}

fn brightness(level: u8) -> Brightness {
    Brightness::new(level).unwrap_or(Brightness::FULL)
}

fn text(line: &str) -> MfdText {
    MfdText::new(line).unwrap_or(MfdText::EMPTY)
}

/// What the MFD clocks show for resolved commands.
#[derive(Debug, PartialEq, Eq)]
struct ClockDisplay {
    clock1: String,
    clock2: String,
    clock3: String,
    date: String,
}

impl ClockDisplay {
    fn of(commands: &[Command]) -> Self {
        let mut shown = Self {
            clock1: String::from("?"),
            clock2: String::from("?"),
            clock3: String::from("?"),
            date: String::from("?"),
        };
        let mut base = None;
        for command in commands {
            if let Command::Clock1 { time, .. } = command {
                base = Some(i32::from(time.hour()) * 60 + i32::from(time.minute()));
                shown.clock1 = format!("{:02}:{:02}", time.hour(), time.minute());
            }
        }
        for command in commands {
            if let Command::ClockOffset { clock, offset, .. } = command
                && let Some(base) = base
            {
                let (time, _) = MidnightPlan::shown(base, i32::from(offset.minutes()));
                match clock {
                    OffsetClock::Clock2 => shown.clock2 = time,
                    OffsetClock::Clock3 => shown.clock3 = time,
                }
            }
            if let Command::Date(fields) = command {
                shown.date = format!(
                    "{:02}-{:02}-{:02}",
                    fields.first(),
                    fields.second(),
                    fields.third()
                );
            }
        }
        shown
    }
}

/// The offsets from clock 1 the midnight scenario sets, chosen from clock 1's minute of the day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MidnightPlan {
    minute: i32,
    /// Clock 2's offset: past midnight forward when the time of day allows it.
    positive: i32,
    /// Clock 3's offset: before midnight backward when the time of day allows it.
    negative: i32,
}

impl MidnightPlan {
    /// How far past midnight a crossing offset lands, in minutes.
    const MARGIN: i32 = 60;

    pub(crate) fn new(minute: i32) -> Self {
        Self {
            minute,
            positive: (1440 - minute + Self::MARGIN).min(720),
            negative: -(minute + Self::MARGIN).min(719),
        }
    }

    pub(crate) const fn positive_crosses(&self) -> bool {
        self.minute + self.positive >= 1440
    }

    pub(crate) const fn negative_crosses(&self) -> bool {
        self.minute + self.negative < 0
    }

    /// The time a clock `offset` minutes from clock 1 at `minute` shows, and whether it is on
    /// another day.
    pub(crate) fn shown(minute: i32, offset: i32) -> (String, bool) {
        let total = minute + offset;
        let within = total.rem_euclid(1440);
        (
            format!("{:02}:{:02}", within / 60, within % 60),
            !(0..1440).contains(&total),
        )
    }
}

/// A fixed offset from UTC that puts a clock `offset` minutes from clock 1, whose offset from
/// UTC is `clock1` minutes: a whole day added or removed as needed to stay in range.
pub(crate) fn fixed_offset_for(offset: i32, clock1: i32) -> Option<FixedOffset> {
    [0, 1440, -1440].iter().find_map(|day| {
        let minutes = i16::try_from(offset + clock1 + day).ok()?;
        FixedOffset::new(minutes).ok()
    })
}

fn display(path: Option<&Path>) -> String {
    path.map_or_else(|| String::from("none"), |path| path.display().to_string())
}

fn mode_text(mode: Option<Mode>) -> String {
    mode.map_or_else(|| String::from("none"), |mode| modes_text(&[mode]))
}

fn modes_text(modes: &[Mode]) -> String {
    let numbers: Vec<&str> = modes
        .iter()
        .map(|mode| match mode {
            Mode::One => "1",
            Mode::Two => "2",
            Mode::Three => "3",
        })
        .collect();
    if numbers.is_empty() {
        String::from("none")
    } else {
        numbers.join(", ")
    }
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
