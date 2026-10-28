//! Spike S3: read X52 / X52 Pro buttons, hats and axes through evdev while the HID driver stays
//! bound. Prints every key event with its code; axes only when --abs is given.

use std::{env, process::ExitCode};

use evdev::EventSummary;

fn main() -> ExitCode {
    let show_abs = env::args().any(|a| a == "--abs");
    let Some((path, mut dev)) = evdev::enumerate().find(|(_, d)| {
        let id = d.input_id();
        id.vendor() == 0x06a3 && matches!(id.product(), 0x0762 | 0x0255 | 0x075c)
    }) else {
        eprintln!("no X52 input device found");
        return ExitCode::FAILURE;
    };
    println!("{} {:?}", path.display(), dev.name());
    if let Some(keys) = dev.supported_keys() {
        let codes: Vec<u16> = keys.iter().map(evdev::KeyCode::code).collect();
        println!("supported keys ({}): {codes:?}", codes.len());
    }
    if let Some(axes) = dev.supported_absolute_axes() {
        println!("supported axes: {:?}", axes.iter().collect::<Vec<_>>());
    }
    if let Ok(state) = dev.get_key_state() {
        let pressed: Vec<u16> = state.iter().map(evdev::KeyCode::code).collect();
        println!("initially pressed: {pressed:?}");
    }
    loop {
        let events = match dev.fetch_events() {
            Ok(e) => e.collect::<Vec<_>>(),
            Err(e) => {
                eprintln!("read error (unplugged?): {e}");
                return ExitCode::FAILURE;
            }
        };
        for ev in events {
            match ev.destructure() {
                EventSummary::Key(_, key, value) => {
                    println!("KEY code={} ({key:?}) value={value}", key.code());
                }
                EventSummary::AbsoluteAxis(_, axis, value) if show_abs => {
                    println!("ABS {axis:?} value={value}");
                }
                _ => {}
            }
        }
    }
}
