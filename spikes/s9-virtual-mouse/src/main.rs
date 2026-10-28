//! Spike S9: turn the X52 Pro throttle's ministick into a mouse pointer.
//!
//! Reads the joystick's evdev node without grabbing it (games keep seeing every input) and writes
//! pointer motion, clicks and wheel steps to a `uinput` virtual mouse. Throwaway code: findings go
//! to `docs/spikes/09-virtual-mouse.md`.
//!
//! Usage: `s9-virtual-mouse [--self-test]`. `--self-test` only creates the virtual mouse, checks
//! it appears as an input device and exits.

use std::{
    io::ErrorKind,
    process::ExitCode,
    thread::sleep,
    time::{Duration, Instant},
};

use evdev::{
    AbsoluteAxisCode, AttributeSet, Device, EventSummary, EventType, InputEvent, KeyCode,
    RelativeAxisCode, uinput::VirtualDevice,
};

const VENDOR: u16 = 0x06a3;
const PRODUCTS: [u16; 3] = [0x0762, 0x0255, 0x075c];
const VIRTUAL_NAME: &str = "GX52 virtual mouse";

/// Ministick axes: `ABS_MISC` and the code after it, both `0..=15`, centre 8.
const STICK_X: AbsoluteAxisCode = AbsoluteAxisCode::ABS_MISC;
const STICK_Y: AbsoluteAxisCode = AbsoluteAxisCode(AbsoluteAxisCode::ABS_MISC.0 + 1);
const STICK_CENTRE: i32 = 8;

/// X52 Pro evdev key codes of the mouse controls on the throttle.
const MOUSE_PRIMARY: KeyCode = KeyCode(303);
const WHEEL_DOWN: KeyCode = KeyCode(704);
const WHEEL_UP: KeyCode = KeyCode(705);
const MOUSE_SECONDARY: KeyCode = KeyCode(706);

/// One motion step every 10 ms (100 Hz), independent of how often the device reports.
const TICK: Duration = Duration::from_millis(10);
/// Offsets up to this size (in ministick steps) are treated as centred.
const DEADZONE: f64 = 1.0;
/// Pixels per tick at one step outside the deadzone.
const GAIN: f64 = 0.6;
/// Response curve: speed grows with `offset ^ CURVE`, slow near the centre, fast at the edge.
const CURVE: f64 = 1.6;

/// Pointer speed in pixels per tick for a ministick offset of `steps` from the centre.
fn speed(steps: f64) -> f64 {
    let past_deadzone = steps.abs() - DEADZONE;
    if past_deadzone <= 0.0 {
        0.0
    } else {
        steps.signum() * GAIN * past_deadzone.powf(CURVE)
    }
}

fn find_joystick() -> Option<Device> {
    evdev::enumerate().map(|(_, device)| device).find(|device| {
        let id = device.input_id();
        id.vendor() == VENDOR && PRODUCTS.contains(&id.product())
    })
}

fn build_virtual_mouse() -> std::io::Result<VirtualDevice> {
    let mut buttons = AttributeSet::<KeyCode>::new();
    buttons.insert(KeyCode::BTN_LEFT);
    buttons.insert(KeyCode::BTN_RIGHT);
    let mut axes = AttributeSet::<RelativeAxisCode>::new();
    for axis in [
        RelativeAxisCode::REL_X,
        RelativeAxisCode::REL_Y,
        RelativeAxisCode::REL_WHEEL,
        RelativeAxisCode::REL_WHEEL_HI_RES,
    ] {
        axes.insert(axis);
    }
    VirtualDevice::builder()?
        .name(VIRTUAL_NAME)
        .with_keys(&buttons)?
        .with_relative_axes(&axes)?
        .build()
}

fn self_test() -> Result<(), String> {
    let mut mouse = build_virtual_mouse().map_err(|e| format!("creating uinput mouse: {e}"))?;
    let syspath = mouse
        .get_syspath()
        .map_err(|e| format!("reading the virtual mouse's sysfs path: {e}"))?;
    println!("virtual mouse created: {}", syspath.display());
    // Waits until udev has created the device node.
    let nodes = mouse
        .enumerate_dev_nodes_blocking()
        .map_err(|e| format!("listing the virtual mouse's device nodes: {e}"))?;
    for node in nodes {
        // The node is root-only (only the compositor reads it), so just show where it is.
        let node = node.map_err(|e| format!("reading a device node: {e}"))?;
        println!("device node: {}", node.display());
    }
    // Give the compositor (libinput) a moment to pick the device up before it goes away.
    sleep(Duration::from_secs(2));
    Ok(())
}

/// What the ministick and the mouse buttons currently report.
#[derive(Debug)]
struct StickState {
    x: i32,
    y: i32,
    remainder_x: f64,
    remainder_y: f64,
}

impl StickState {
    fn centred() -> Self {
        Self {
            x: STICK_CENTRE,
            y: STICK_CENTRE,
            remainder_x: 0.0,
            remainder_y: 0.0,
        }
    }

    /// Whole pixels to move this tick; fractions carry over so slow speeds still move.
    fn motion(&mut self) -> (i32, i32) {
        // The ministick's X axis grows to the left (checked on an X52 Pro), so it is inverted.
        let dx = self.remainder_x + speed(f64::from(STICK_CENTRE - self.x));
        let dy = self.remainder_y + speed(f64::from(self.y - STICK_CENTRE));
        let (whole_x, whole_y) = (dx.trunc(), dy.trunc());
        // Back to the centre: drop the leftover so the pointer stops dead.
        self.remainder_x = if self.x == STICK_CENTRE {
            0.0
        } else {
            dx - whole_x
        };
        self.remainder_y = if self.y == STICK_CENTRE {
            0.0
        } else {
            dy - whole_y
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "at most a few pixels per tick"
        )]
        (whole_x as i32, whole_y as i32)
    }
}

/// Maps one joystick event to virtual mouse events, updating the ministick state.
fn translate(event: &InputEvent, state: &mut StickState, out: &mut Vec<InputEvent>) {
    match event.destructure() {
        EventSummary::AbsoluteAxis(_, axis, value) if axis == STICK_X => state.x = value,
        EventSummary::AbsoluteAxis(_, axis, value) if axis == STICK_Y => state.y = value,
        EventSummary::Key(_, key, value) if key == MOUSE_PRIMARY => {
            out.push(InputEvent::new(
                EventType::KEY.0,
                KeyCode::BTN_LEFT.0,
                value,
            ));
        }
        EventSummary::Key(_, key, value) if key == MOUSE_SECONDARY => {
            out.push(InputEvent::new(
                EventType::KEY.0,
                KeyCode::BTN_RIGHT.0,
                value,
            ));
        }
        // The wheel reports one press per detent; only the press counts.
        EventSummary::Key(_, key, 1) if key == WHEEL_UP || key == WHEEL_DOWN => {
            let steps = if key == WHEEL_UP { 1 } else { -1 };
            out.push(InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_WHEEL.0,
                steps,
            ));
            out.push(InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_WHEEL_HI_RES.0,
                steps * 120,
            ));
        }
        _ => {}
    }
}

fn run() -> Result<(), String> {
    let mut joystick = find_joystick().ok_or("no X52 input device found")?;
    println!("reading {:?} (not grabbed)", joystick.name());
    joystick
        .set_nonblocking(true)
        .map_err(|e| format!("set_nonblocking: {e}"))?;
    let mut mouse = build_virtual_mouse().map_err(|e| format!("creating uinput mouse: {e}"))?;
    println!("virtual mouse ready; move the ministick, Ctrl+C to stop");

    let mut state = StickState::centred();
    let mut out = Vec::new();
    let mut next_tick = Instant::now();
    loop {
        match joystick.fetch_events() {
            Ok(events) => {
                for event in events {
                    translate(&event, &mut state, &mut out);
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {}
            Err(e) => return Err(format!("joystick read error (unplugged?): {e}")),
        }
        let (dx, dy) = state.motion();
        if dx != 0 {
            out.push(InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_X.0,
                dx,
            ));
        }
        if dy != 0 {
            out.push(InputEvent::new(
                EventType::RELATIVE.0,
                RelativeAxisCode::REL_Y.0,
                dy,
            ));
        }
        if !out.is_empty() {
            // `emit` appends the SYN_REPORT.
            mouse.emit(&out).map_err(|e| format!("uinput write: {e}"))?;
            out.clear();
        }
        next_tick += TICK;
        sleep(next_tick.saturating_duration_since(Instant::now()));
    }
}

fn main() -> ExitCode {
    let result = if std::env::args().any(|arg| arg == "--self-test") {
        self_test()
    } else {
        run()
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadzone_and_curve() {
        assert!(speed(0.0).abs() < f64::EPSILON);
        assert!(speed(1.0).abs() < f64::EPSILON);
        assert!(speed(2.0) > 0.0);
        assert!(speed(-2.0) < 0.0);
        assert!(
            speed(7.0) > speed(3.0) * 3.0,
            "curve is steeper than linear"
        );
    }

    #[test]
    fn slow_speeds_accumulate_into_pixels() {
        let mut state = StickState::centred();
        state.x = STICK_CENTRE - 2;
        let moved: i32 = (0..10).map(|_| state.motion().0).sum();
        // 0.6 px per tick for 10 ticks, give or take float rounding.
        assert!((5..=6).contains(&moved), "moved {moved} px");
        state.x = STICK_CENTRE;
        assert_eq!(state.motion(), (0, 0));
    }
}
