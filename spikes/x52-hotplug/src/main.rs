//! Spike S3/S4: how does the app notice the X52 being plugged in and out?
//!
//! Prints, with timestamps, three independent signals: `nusb::watch_devices` events (udev netlink
//! on Linux), and changes seen by polling the USB device list and the evdev device list every
//! 250 ms. Run on the host and inside the Flatpak sandbox to compare. Throwaway code.

use std::{
    thread,
    time::{Duration, Instant},
};

use futures_lite::{StreamExt, future::block_on};
use nusb::{MaybeFuture, hotplug::HotplugEvent};

const VENDOR: u16 = 0x06a3;
const PRODUCTS: [u16; 3] = [0x0762, 0x0255, 0x075c];
const POLL: Duration = Duration::from_millis(250);

fn usb_present() -> bool {
    nusb::list_devices().wait().is_ok_and(|mut devices| {
        devices.any(|d| d.vendor_id() == VENDOR && PRODUCTS.contains(&d.product_id()))
    })
}

fn evdev_present() -> bool {
    evdev::enumerate().any(|(_, device)| {
        let id = device.input_id();
        id.vendor() == VENDOR && PRODUCTS.contains(&id.product())
    })
}

fn main() {
    let start = Instant::now();
    let stamp = move || format!("{:8.3}s", start.elapsed().as_secs_f64());

    match nusb::watch_devices() {
        Ok(mut watch) => {
            thread::spawn(move || {
                while let Some(event) = block_on(watch.next()) {
                    match event {
                        HotplugEvent::Connected(info)
                            if info.vendor_id() == VENDOR
                                && PRODUCTS.contains(&info.product_id()) =>
                        {
                            println!("{} nusb watch: X52 connected ({:?})", stamp(), info.id());
                        }
                        HotplugEvent::Connected(_) => {}
                        HotplugEvent::Disconnected(id) => {
                            println!("{} nusb watch: a USB device disconnected ({id:?})", stamp());
                        }
                    }
                }
                println!("{} nusb watch: stream ended", stamp());
            });
        }
        Err(e) => println!("{} nusb watch: not available: {e}", stamp()),
    }

    let (mut usb, mut input) = (usb_present(), evdev_present());
    println!(
        "{} start: USB present {usb}, evdev present {input}",
        stamp()
    );
    loop {
        thread::sleep(POLL);
        let (now_usb, now_input) = (usb_present(), evdev_present());
        if now_usb != usb {
            println!("{} poll: USB present {now_usb}", stamp());
            usb = now_usb;
        }
        if now_input != input {
            println!("{} poll: evdev present {now_input}", stamp());
            input = now_input;
        }
    }
}
