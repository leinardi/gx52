//! Sends LED states to the X52 from a worker thread, so USB transfers never block the GTK main
//! thread. The thread (re)opens the device when needed and then replays the full state, because
//! after plugging in the device starts with brightness 0 (S2).

use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::Duration,
};

use nusb::{
    MaybeFuture,
    transfer::{ControlOut, ControlType, Recipient},
};

const VENDOR: u16 = 0x06a3;
const PRODUCTS: [u16; 3] = [0x0762, 0x0255, 0x075c];
const REQUEST: u8 = 0x91;
const TIMEOUT: Duration = Duration::from_secs(1);
/// How often to look for the device while it is missing or not accessible.
const RETRY: Duration = Duration::from_secs(1);

const CMD_MFD_BRIGHTNESS: u16 = 0xb1;
const CMD_LED_BRIGHTNESS: u16 = 0xb2;
const CMD_LED: u16 = 0xb8;
const FULL_BRIGHTNESS: u16 = 128;

/// The bits that drive one LED: a single-colour LED has only `green` (its "on" bit).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedBits {
    pub red: Option<u16>,
    pub green: u16,
}

/// A requested LED state: which bits to set on and off.
#[derive(Debug, Clone, Copy)]
pub struct LedCommand {
    pub bits: LedBits,
    pub red_on: bool,
    pub green_on: bool,
}

impl LedCommand {
    /// Builds the command for a state name from the editor (`off`, `on`, `green`, `red`, `amber`).
    pub fn new(bits: LedBits, state: &str) -> Self {
        let (red_on, green_on) = match state {
            "on" | "green" => (false, true),
            "red" => (true, false),
            "amber" => (true, true),
            _ => (false, false),
        };
        Self {
            bits,
            red_on,
            green_on,
        }
    }
}

/// Starts the worker thread and returns the channel to send LED changes to it.
pub fn spawn() -> Sender<LedCommand> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("x52-device".into())
        .spawn(move || run(&receiver))
        .expect("spawning the device thread");
    sender
}

fn run(receiver: &Receiver<LedCommand>) {
    // Latest command per LED, replayed on every (re)connect.
    let mut state: BTreeMap<u16, LedCommand> = BTreeMap::new();
    let mut device: Option<nusb::Device> = None;
    loop {
        match receiver.recv_timeout(RETRY) {
            Ok(command) => {
                state.insert(command.bits.green, command);
                if let Some(open) = &device
                    && let Err(e) = send_led(open, command)
                {
                    tracing::warn!("device write failed, will reopen: {e}");
                    device = None;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if device.is_none() {
            device = open_and_replay(&state);
        }
    }
}

fn open_and_replay(state: &BTreeMap<u16, LedCommand>) -> Option<nusb::Device> {
    let info = nusb::list_devices()
        .wait()
        .ok()?
        .find(|d| d.vendor_id() == VENDOR && PRODUCTS.contains(&d.product_id()))?;
    let device = match info.open().wait() {
        Ok(device) => device,
        Err(e) => {
            tracing::warn!("X52 found but cannot be opened: {e}");
            return None;
        }
    };
    let replay = || -> Result<(), nusb::transfer::TransferError> {
        send(&device, CMD_LED_BRIGHTNESS, FULL_BRIGHTNESS)?;
        send(&device, CMD_MFD_BRIGHTNESS, FULL_BRIGHTNESS)?;
        for command in state.values() {
            send_led(&device, *command)?;
        }
        Ok(())
    };
    match replay() {
        Ok(()) => {
            tracing::info!("X52 connected, {} LED states sent", state.len());
            Some(device)
        }
        Err(e) => {
            tracing::warn!("X52 opened but writing failed: {e}");
            None
        }
    }
}

fn send_led(
    device: &nusb::Device,
    command: LedCommand,
) -> Result<(), nusb::transfer::TransferError> {
    send(
        device,
        CMD_LED,
        command.bits.green << 8 | u16::from(command.green_on),
    )?;
    if let Some(red) = command.bits.red {
        send(device, CMD_LED, red << 8 | u16::from(command.red_on))?;
    }
    Ok(())
}

fn send(
    device: &nusb::Device,
    index: u16,
    value: u16,
) -> Result<(), nusb::transfer::TransferError> {
    device
        .control_out(
            ControlOut {
                control_type: ControlType::Vendor,
                recipient: Recipient::Device,
                request: REQUEST,
                value,
                index,
                data: &[],
            },
            TIMEOUT,
        )
        .wait()
}
