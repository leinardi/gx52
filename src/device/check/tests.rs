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

//! The scenarios run against the real device service over a fake backend: a scripted watch that
//! plays the user's `STEP`s, an opener whose permission the script switches, and an evdev source
//! whose node replays scripted key events in real time.

use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use jiff::{SignedDuration, Timestamp};
use x52::clock::FakeClock;
use x52::transport::RecordingTransport;

use super::*;
use crate::device::Backend;
use crate::device::id::Attachment;
use crate::device::reader::{EvdevSource, InputEvent, InputNode, NodeEntry};
use crate::device::usb::{OpenError, Opener};
use crate::device::watch::{SelectionChange, ShutdownSignal, WatchEvent};

const USB: &str = "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-4";
const MODE_1: u16 = 715;
const MODE_2: u16 = 716;
const MODE_3: u16 = 717;

fn device(generation: u64, address: u8) -> DeviceId {
    DeviceId {
        device_type: DeviceType::X52Pro,
        sysfs_path: PathBuf::from(USB),
        attachment: Attachment { bus: 1, address },
        generation: Generation(generation),
    }
}

fn ms(milliseconds: u64) -> Duration {
    Duration::from_millis(milliseconds)
}

/// What the scripted watch does, after a delay.
enum Action {
    Emit(WatchEvent),
    /// The udev rule is removed (`false`) or restored (`true`).
    Permission(bool),
    /// The wall clock jumps, as across a suspend.
    Jump(SignedDuration),
}

fn selected(generation: u64, address: u8) -> Action {
    Action::Emit(WatchEvent::Selection(SelectionChange::Selected(device(
        generation, address,
    ))))
}

fn gone(generation: u64) -> Action {
    Action::Emit(WatchEvent::Selection(SelectionChange::Gone(Generation(
        generation,
    ))))
}

struct FakeBackend {
    script: Vec<(Duration, Action)>,
    allowed: Arc<AtomicBool>,
    clock: Arc<FakeClock>,
    /// Key events the evdev node plays, by time since it is opened.
    keys: Vec<(Duration, u16, bool)>,
    /// What the backend tweaks: see [`Setup`].
    setup: Setup,
}

/// What a test changes in the fake backend.
#[derive(Clone, Copy)]
struct Setup {
    /// The time zone the writer follows.
    zone: fn() -> TimeZone,
    /// The handle of the second open fails at this transfer.
    second_open_fails_at: Option<usize>,
    /// Whether `x52d` runs when the watch starts.
    x52d_running: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            zone: || TimeZone::UTC,
            second_open_fails_at: None,
            x52d_running: false,
        }
    }
}

struct FakeOpener {
    allowed: Arc<AtomicBool>,
    opens: usize,
    second_open_fails_at: Option<usize>,
}

impl Opener for FakeOpener {
    type Transport = RecordingTransport;

    fn open(&mut self, _: &DeviceId) -> Result<RecordingTransport, OpenError> {
        if self.allowed.load(Ordering::SeqCst) {
            self.opens += 1;
            Ok(match self.second_open_fails_at {
                Some(call) if self.opens == 2 => RecordingTransport::failing_at(call),
                Some(_) | None => RecordingTransport::new(),
            })
        } else {
            Err(OpenError::PermissionDenied)
        }
    }
}

struct FakeEvdev {
    keys: Vec<(Duration, u16, bool)>,
}

impl EvdevSource for FakeEvdev {
    type Node = FakeNode;

    fn nodes(&mut self) -> Vec<NodeEntry> {
        vec![NodeEntry {
            node: PathBuf::from("/dev/input/event20"),
            device: PathBuf::from(format!("{USB}/1-4:1.0/input/input20")),
        }]
    }

    fn canonical(&self, path: &Path) -> PathBuf {
        path.to_path_buf()
    }

    fn open(&mut self, _: &Path) -> io::Result<FakeNode> {
        Ok(FakeNode {
            keys: self.keys.clone().into(),
            opened: SystemMonotonic::new(),
            due: Vec::new(),
        })
    }
}

/// Mode 1 held; then the scripted keys, each returned once its time since the open has come.
struct FakeNode {
    keys: std::collections::VecDeque<(Duration, u16, bool)>,
    opened: SystemMonotonic,
    due: Vec<InputEvent>,
}

impl InputNode for FakeNode {
    fn held_keys(&self) -> io::Result<Vec<u16>> {
        Ok(vec![MODE_1])
    }

    fn wait(&mut self, timeout: Duration) -> io::Result<bool> {
        let now = self.opened.elapsed();
        let until = self
            .keys
            .front()
            .map_or(timeout, |(at, _, _)| at.saturating_sub(now).min(timeout));
        thread::sleep(until);
        let now = self.opened.elapsed();
        while let Some((at, code, pressed)) = self.keys.front().copied() {
            if at > now {
                break;
            }
            self.keys.pop_front();
            self.due.push(InputEvent {
                event_type: 1,
                code,
                value: i32::from(pressed),
                at,
            });
        }
        Ok(!self.due.is_empty())
    }

    fn read(&mut self) -> io::Result<Vec<InputEvent>> {
        Ok(std::mem::take(&mut self.due))
    }
}

impl Backend for FakeBackend {
    type Opener = FakeOpener;
    type Evdev = FakeEvdev;

    fn opener(&mut self) -> FakeOpener {
        FakeOpener {
            allowed: Arc::clone(&self.allowed),
            opens: 0,
            second_open_fails_at: self.setup.second_open_fails_at,
        }
    }

    fn evdev(&mut self) -> FakeEvdev {
        FakeEvdev {
            keys: self.keys.clone(),
        }
    }

    fn local_zone(&self) -> fn() -> TimeZone {
        self.setup.zone
    }

    fn watch(self, shutdown: &ShutdownSignal, mut notify: impl FnMut(WatchEvent)) {
        notify(WatchEvent::X52dRunning(self.setup.x52d_running));
        for (delay, action) in self.script {
            if shutdown.wait_timeout(delay) {
                return;
            }
            match action {
                Action::Emit(event) => notify(event),
                Action::Permission(allowed) => self.allowed.store(allowed, Ordering::SeqCst),
                Action::Jump(by) => self.clock.advance(by),
            }
        }
        while !shutdown.wait_timeout(Duration::from_secs(1)) {}
    }
}

/// The fake clock, shared by the writer and the harness.
#[derive(Clone)]
struct SharedClock(Arc<FakeClock>);

impl Clock for SharedClock {
    fn now(&self) -> Timestamp {
        self.0.now()
    }
}

fn test_env() -> Env {
    Env {
        node_writable: |_| false,
        uinput: || UinputAccess::Available,
        x52d_installed: || false,
        local_zone: || TimeZone::UTC,
        step_timeout: Duration::from_secs(5),
        auto_timeout: Duration::from_secs(3),
    }
}

/// Runs `scenario` over a fake backend; returns the outcome and the report.
fn check(
    scenario: Scenario,
    script: Vec<(Duration, Action)>,
    keys: Vec<(Duration, u16, bool)>,
    now: &str,
    env: &Env,
) -> (Outcome, String) {
    check_with(scenario, script, keys, now, env, Setup::default())
}

/// [`check`] with the backend changed by `setup`.
fn check_with(
    scenario: Scenario,
    script: Vec<(Duration, Action)>,
    keys: Vec<(Duration, u16, bool)>,
    now: &str,
    env: &Env,
    setup: Setup,
) -> (Outcome, String) {
    let clock = Arc::new(FakeClock::new(now.parse().unwrap()));
    let backend = FakeBackend {
        script,
        allowed: Arc::new(AtomicBool::new(true)),
        clock: Arc::clone(&clock),
        keys,
        setup,
    };
    let shared = SharedClock(clock);
    let (service, events) = DeviceService::start_with(shared.clone(), backend).unwrap();
    let mut report = Vec::new();
    let outcome = run(
        scenario,
        &service,
        events,
        &shared,
        &SystemMonotonic::new(),
        env,
        &mut report,
    );
    service.shutdown();
    (outcome, String::from_utf8(report).unwrap())
}

const NOON: &str = "2026-11-03T12:00:30Z";

fn count(report: &str, kind: &str) -> usize {
    report
        .lines()
        .filter(|line| line.split_whitespace().nth(1) == Some(kind))
        .count()
}

#[test]
fn smoke_passes_with_a_device() {
    let (outcome, report) = check(
        Scenario::Smoke,
        vec![(ms(0), selected(1, 5))],
        Vec::new(),
        NOON,
        &test_env(),
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert_eq!(count(&report, "FAIL"), 0, "{report}");
    assert!(report.contains("full replay: 22 commands"), "{report}");
    assert!(
        report.contains("evdev node found (/dev/input/event20)"),
        "{report}"
    );
    assert!(report.contains("initial mode read (1)"), "{report}");
    assert_eq!(
        count(&report, "SEND"),
        22,
        "every transfer is printed: {report}"
    );
}

#[test]
fn smoke_fails_without_a_device() {
    let env = Env {
        auto_timeout: ms(300),
        ..test_env()
    };
    let (outcome, report) = check(Scenario::Smoke, Vec::new(), Vec::new(), NOON, &env);
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(report.contains("no X52 joystick selected"), "{report}");
}

#[test]
fn replug_verifies_the_full_replay_of_the_state_set_while_unplugged() {
    let (outcome, report) = check(
        Scenario::Replug,
        vec![
            (ms(0), selected(1, 5)),
            (ms(800), gone(1)),
            (ms(800), selected(2, 6)),
        ],
        Vec::new(),
        NOON,
        &test_env(),
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert_eq!(count(&report, "FAIL"), 0, "{report}");
    assert!(report.contains("STATE B"), "{report}");
    assert!(
        report.contains("Clock 1 shows 12:00 (24 h), clock 2 08:00, clock 3 20:00"),
        "{report}"
    );
}

#[test]
fn mode_passes_for_one_change_per_position() {
    // 1 to 2, 2 to 3, then 3 to 1 through 2 for 50 ms.
    let keys = vec![
        (ms(600), MODE_1, false),
        (ms(600), MODE_2, true),
        (ms(1100), MODE_2, false),
        (ms(1100), MODE_3, true),
        (ms(1600), MODE_3, false),
        (ms(1600), MODE_2, true),
        (ms(1650), MODE_2, false),
        (ms(1650), MODE_1, true),
    ];
    let (outcome, report) = check(
        Scenario::Mode,
        vec![(ms(0), selected(1, 5))],
        keys,
        NOON,
        &test_env(),
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(report.contains("(seen 2, 3, 1)"), "{report}");
}

#[test]
fn mode_fails_when_the_passed_through_2_is_applied() {
    // The 2 between 3 and 1 is held for 300 ms, longer than the debounce.
    let keys = vec![
        (ms(600), MODE_1, false),
        (ms(600), MODE_2, true),
        (ms(1100), MODE_2, false),
        (ms(1100), MODE_3, true),
        (ms(1600), MODE_3, false),
        (ms(1600), MODE_2, true),
        (ms(1900), MODE_2, false),
        (ms(1900), MODE_1, true),
    ];
    let (outcome, report) = check(
        Scenario::Mode,
        vec![(ms(0), selected(1, 5))],
        keys,
        NOON,
        &test_env(),
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(report.contains("(seen 2, 3, 2, 1)"), "{report}");
}

fn permission_script() -> Vec<(Duration, Action)> {
    vec![
        (ms(0), selected(1, 5)),
        // The rule removed, reloaded and triggered, then a replug.
        (ms(800), Action::Permission(false)),
        (ms(0), gone(1)),
        (ms(300), selected(2, 6)),
        // The rule restored, with no replug: no event at all.
        (ms(1500), Action::Permission(true)),
    ]
}

#[test]
fn permission_passes_through_the_periodic_retry() {
    let (outcome, report) = check(
        Scenario::Permission,
        permission_script(),
        Vec::new(),
        NOON,
        &test_env(),
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(report.contains("access state NoPermission"), "{report}");
    assert!(
        report.contains("with no replug (same generation)"),
        "{report}"
    );
    let restore = report
        .lines()
        .filter(|line| line.split_whitespace().nth(1) == Some("INFO"))
        .filter(|line| line.contains("sudo mv /etc/udev/rules.d/70-gx52.rules.disabled"))
        .count();
    assert_eq!(restore, 2, "printed at the start and at the end: {report}");
}

#[test]
fn permission_with_a_writable_node_is_a_precondition_failure() {
    let env = Env {
        node_writable: |_| true,
        ..test_env()
    };
    let (outcome, report) = check(
        Scenario::Permission,
        permission_script(),
        Vec::new(),
        NOON,
        &env,
    );
    assert_eq!(outcome, Outcome::PreconditionFailed, "{report}");
    assert!(
        report.contains("/dev/bus/usb/001/006 is still writable"),
        "{report}"
    );
    assert!(report.contains("precondition not met"), "{report}");
    assert!(
        report.contains("sudo mv /etc/udev/rules.d/70-gx52.rules.disabled"),
        "{report}"
    );
}

#[test]
fn resume_verifies_the_clocks_are_resent_after_a_wall_clock_jump() {
    let env = Env {
        auto_timeout: Duration::from_secs(8),
        ..test_env()
    };
    let (outcome, report) = check(
        Scenario::Resume,
        vec![
            (ms(0), selected(1, 5)),
            (ms(1500), Action::Jump(SignedDuration::from_mins(2))),
        ],
        Vec::new(),
        NOON,
        &env,
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(report.contains("suspend detected"), "{report}");
    assert!(report.contains("clock 1 re-sent"), "{report}");
    assert!(report.contains("The MFD clock 1 shows 12:02"), "{report}");
}

#[test]
fn midnight_sends_a_crossing_offset() {
    let (outcome, report) = check(
        Scenario::Midnight,
        vec![(ms(0), selected(1, 5))],
        Vec::new(),
        "2026-11-03T23:30:00Z",
        &test_env(),
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(
        report.contains("(90, -719) minutes from clock 1"),
        "{report}"
    );
    assert!(
        report.contains("Clock 2 shows 01:00 (+90 minutes from clock 1, across midnight)"),
        "{report}"
    );
    assert!(report.contains("Clock 3 shows 11:31"), "{report}");
}

#[test]
fn x52d_without_the_daemon_installed_is_reported() {
    let (outcome, report) = check(Scenario::X52d, Vec::new(), Vec::new(), NOON, &test_env());
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(report.contains("not installed"), "{report}");
}

#[test]
fn uinput_is_reported() {
    let (outcome, report) = check(Scenario::Uinput, Vec::new(), Vec::new(), NOON, &test_env());
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(report.contains("uinput: available"), "{report}");
    let env = Env {
        uinput: || UinputAccess::NoPermission,
        ..test_env()
    };
    let (outcome, report) = check(Scenario::Uinput, Vec::new(), Vec::new(), NOON, &env);
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(report.contains("uinput: NoPermission"), "{report}");
}

#[test]
fn the_option_is_refused_in_a_release_build() {
    let args =
        |list: &[&str]| -> Vec<String> { list.iter().map(|arg| (*arg).to_owned()).collect() };
    assert_eq!(
        parse_args(&args(&["--device-check", "smoke"]), false),
        Err(ArgsError::ReleaseBuild)
    );
    assert_eq!(parse_args(&args(&[]), false), Ok(None));
    assert_eq!(
        parse_args(&args(&["--device-check", "smoke"]), true),
        Ok(Some(Scenario::Smoke))
    );
    assert_eq!(
        parse_args(&args(&["--device-check=replug"]), true),
        Ok(Some(Scenario::Replug))
    );
    assert_eq!(
        parse_args(&args(&["--device-check"]), true),
        Err(ArgsError::MissingScenario)
    );
    assert_eq!(
        parse_args(&args(&["--device-check", "dance"]), true),
        Err(ArgsError::UnknownScenario("dance".into()))
    );
    for scenario in Scenario::ALL {
        assert_eq!(scenario.name().parse(), Ok(scenario));
    }
}

#[test]
fn a_replay_matches_except_for_the_time() {
    let state = DeviceState::default_for(DeviceType::X52Pro);
    let at = |instant: &str| {
        state
            .resolve(DeviceType::X52Pro, instant.parse().unwrap(), &TimeZone::UTC)
            .unwrap()
    };
    let noon = at("2026-11-03T12:00:00Z");
    assert_eq!(replay_matches(&noon, &noon), Ok(()));
    assert_eq!(replay_matches(&at("2026-11-03T12:01:00Z"), &noon), Ok(()));
    assert!(replay_matches(&noon[1..], &noon).is_err());
    let mut other = noon.clone();
    other[0] = Command::MfdBrightness(Brightness::new(1).unwrap());
    assert!(replay_matches(&other, &noon).is_err());
    other.swap(0, 1);
    assert!(replay_matches(&other, &noon).is_err());
}

#[test]
fn the_midnight_plan_crosses_in_the_direction_the_time_allows() {
    let evening = MidnightPlan::new(23 * 60 + 30);
    assert!(evening.positive_crosses());
    assert!(!evening.negative_crosses());
    let morning = MidnightPlan::new(30);
    assert!(!morning.positive_crosses());
    assert!(morning.negative_crosses());
    assert_eq!(MidnightPlan::shown(30, -90), (String::from("23:00"), true));
    assert_eq!(fixed_offset_for(-719, 60).unwrap().minutes(), -659);
    // +720 from clock 1 at UTC+14:00 is beyond +840 and goes round by a day.
    assert_eq!(fixed_offset_for(720, 840).unwrap().minutes(), 120);
}

#[test]
fn exit_codes() {
    assert_eq!(Outcome::Passed.exit_code(), 0);
    assert_eq!(Outcome::Failed.exit_code(), 1);
    assert_eq!(Outcome::PreconditionFailed.exit_code(), 3);
}

// Every automatic verification also fails when what it checks does not happen.

/// The report's `FAIL` lines.
fn failures(report: &str) -> Vec<&str> {
    report
        .lines()
        .filter(|line| line.split_whitespace().nth(1) == Some("FAIL"))
        .collect()
}

fn short_steps() -> Env {
    Env {
        step_timeout: Duration::from_millis(1500),
        ..test_env()
    }
}

#[test]
fn replug_fails_without_a_new_selection() {
    let (outcome, report) = check(
        Scenario::Replug,
        vec![(ms(0), selected(1, 5)), (ms(800), gone(1))],
        Vec::new(),
        NOON,
        &short_steps(),
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(
        failures(&report)[0].contains("the joystick is selected again"),
        "{report}"
    );
}

#[test]
fn replug_fails_when_the_replay_breaks() {
    let setup = Setup {
        second_open_fails_at: Some(3),
        ..Setup::default()
    };
    let (outcome, report) = check_with(
        Scenario::Replug,
        vec![
            (ms(0), selected(1, 5)),
            (ms(800), gone(1)),
            (ms(800), selected(2, 6)),
        ],
        Vec::new(),
        NOON,
        &test_env(),
        setup,
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    let failed = failures(&report);
    assert!(
        failed.iter().any(|line| line.contains("full replay:")),
        "{report}"
    );
    assert!(
        failed
            .iter()
            .any(|line| line.contains("every transfer succeeded (1 errors)")),
        "{report}"
    );
}

#[test]
fn permission_fails_when_access_is_never_denied() {
    let (outcome, report) = check(
        Scenario::Permission,
        vec![
            (ms(0), selected(1, 5)),
            (ms(800), gone(1)),
            (ms(300), selected(2, 6)),
        ],
        Vec::new(),
        NOON,
        &test_env(),
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(
        failures(&report)[0].contains("access state NoPermission (seen Some(Ready))"),
        "{report}"
    );
}

#[test]
fn permission_fails_when_access_never_comes_back() {
    let (outcome, report) = check(
        Scenario::Permission,
        vec![
            (ms(0), selected(1, 5)),
            (ms(800), Action::Permission(false)),
            (ms(0), gone(1)),
            (ms(300), selected(2, 6)),
        ],
        Vec::new(),
        NOON,
        &short_steps(),
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(
        failures(&report)[0].contains("access state Ready (seen Some(NoPermission))"),
        "{report}"
    );
}

#[test]
fn permission_rechecks_the_node_once_the_access_state_settles() {
    // Not writable when the device is first seen, writable a moment later: udev applied the
    // rule's ACL after the sandbox's poll saw the device.
    static CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let env = Env {
        node_writable: |_| CALLS.fetch_add(1, Ordering::SeqCst) > 0,
        ..test_env()
    };
    let (outcome, report) = check(
        Scenario::Permission,
        permission_script(),
        Vec::new(),
        NOON,
        &env,
    );
    assert_eq!(outcome, Outcome::PreconditionFailed, "{report}");
}

#[test]
fn resume_fails_without_a_suspend() {
    let (outcome, report) = check(
        Scenario::Resume,
        vec![(ms(0), selected(1, 5))],
        Vec::new(),
        NOON,
        &short_steps(),
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(
        failures(&report)[0].contains("no suspend detected"),
        "{report}"
    );
}

#[test]
fn resume_fails_when_clock_1_is_not_resent() {
    // A jump of exactly a day changes the date but leaves clock 1 as it was, so the writer has
    // no clock 1 to send again and the check sees none.
    let env = Env {
        auto_timeout: Duration::from_secs(7),
        ..test_env()
    };
    let (outcome, report) = check(
        Scenario::Resume,
        vec![
            (ms(0), selected(1, 5)),
            (ms(1500), Action::Jump(SignedDuration::from_hours(24))),
        ],
        Vec::new(),
        NOON,
        &env,
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(
        failures(&report)[0].contains("clock 1 was not re-sent after the resume"),
        "{report}"
    );
}

#[test]
fn midnight_fails_when_other_offsets_are_sent() {
    // The writer follows Europe/Rome while the harness computed the offsets for UTC.
    let setup = Setup {
        zone: || TimeZone::get("Europe/Rome").unwrap(),
        ..Setup::default()
    };
    let (outcome, report) = check_with(
        Scenario::Midnight,
        vec![(ms(0), selected(1, 5))],
        Vec::new(),
        "2026-11-03T23:30:00Z",
        &test_env(),
        setup,
    );
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(
        failures(&report)
            .iter()
            .any(|line| line.contains("clock offsets sent")),
        "{report}"
    );
}

#[test]
fn x52d_installed_is_started_and_stopped() {
    let env = Env {
        x52d_installed: || true,
        ..test_env()
    };
    let (outcome, report) = check(
        Scenario::X52d,
        vec![
            (ms(500), Action::Emit(WatchEvent::X52dRunning(true))),
            (ms(500), Action::Emit(WatchEvent::X52dRunning(false))),
        ],
        Vec::new(),
        NOON,
        &env,
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(report.contains("Start x52d"), "{report}");
    assert!(report.contains("x52d detected as running"), "{report}");
    assert!(report.contains("x52d detected as stopped"), "{report}");
}

#[test]
fn x52d_running_counts_as_installed_where_its_binary_is_not_visible() {
    let setup = Setup {
        x52d_running: true,
        ..Setup::default()
    };
    let (outcome, report) = check_with(
        Scenario::X52d,
        vec![
            (ms(500), Action::Emit(WatchEvent::X52dRunning(false))),
            (ms(500), Action::Emit(WatchEvent::X52dRunning(true))),
        ],
        Vec::new(),
        NOON,
        &test_env(),
        setup,
    );
    assert_eq!(outcome, Outcome::Passed, "{report}");
    assert!(!report.contains("not installed"), "{report}");
    assert!(report.contains("Stop x52d"), "{report}");
}

#[test]
fn x52d_fails_when_the_daemon_is_not_seen_to_start() {
    let env = Env {
        x52d_installed: || true,
        ..short_steps()
    };
    let (outcome, report) = check(Scenario::X52d, Vec::new(), Vec::new(), NOON, &env);
    assert_eq!(outcome, Outcome::Failed, "{report}");
    assert!(
        failures(&report)[0].contains("x52d detected as running"),
        "{report}"
    );
}

/// The main thread clears the mode on a new selection, so the reader's initial mode of that
/// selection must never arrive before its `Selected`.
#[test]
fn a_selection_reaches_the_main_thread_before_its_initial_mode() {
    for _ in 0..50 {
        let backend = FakeBackend {
            script: vec![(ms(0), selected(1, 5))],
            allowed: Arc::new(AtomicBool::new(true)),
            clock: Arc::new(FakeClock::new(NOON.parse().unwrap())),
            keys: Vec::new(),
            setup: Setup::default(),
        };
        let clock = SharedClock(Arc::clone(&backend.clock));
        let (service, events) = DeviceService::start_with(clock, backend).unwrap();
        let mut selected_seen = false;
        loop {
            match events.recv_blocking().unwrap() {
                DeviceEvent::Selected(_) => selected_seen = true,
                DeviceEvent::Mode(_) => {
                    assert!(selected_seen, "the initial mode came before its selection");
                    break;
                }
                DeviceEvent::Gone(_)
                | DeviceEvent::Access(_)
                | DeviceEvent::InputNode(_)
                | DeviceEvent::X52dRunning(_)
                | DeviceEvent::Error(_)
                | DeviceEvent::Sent(_) => {}
            }
        }
        service.shutdown();
    }
}
