//! Spike S1/S2: drive an X52 / X52 Pro with nusb vendor control transfers, without claiming
//! or detaching the HID interface, and exercise every known command so it can be verified on
//! the device. Throwaway code: findings go to docs/spikes/.

use std::{env, fs, path::PathBuf, process::ExitCode, thread::sleep, time::Duration};

use chrono::{Datelike, Local, Timelike};
use nusb::{
    MaybeFuture,
    transfer::{ControlOut, ControlType, Recipient},
};

const VENDOR: u16 = 0x06a3;
const PRODUCTS: [(u16, &str); 3] = [(0x0762, "X52 Pro"), (0x0255, "X52"), (0x075c, "X52")];
const REQUEST: u8 = 0x91;
const TIMEOUT: Duration = Duration::from_secs(5);

const LEDS: [(&str, Option<u16>, u16); 11] = [
    // name, red bit (None for single-colour), green/on bit
    ("fire", None, 1),
    ("a", Some(2), 3),
    ("b", Some(4), 5),
    ("d", Some(6), 7),
    ("e", Some(8), 9),
    ("t1", Some(10), 11),
    ("t2", Some(12), 13),
    ("t3", Some(14), 15),
    ("pov", Some(16), 17),
    ("i", Some(18), 19),
    ("throttle", None, 20),
];

struct X52 {
    device: nusb::Device,
    sysfs: PathBuf,
}

impl X52 {
    fn open() -> Result<Self, String> {
        let info = nusb::list_devices()
            .wait()
            .map_err(|e| format!("list_devices: {e}"))?
            .find(|d| d.vendor_id() == VENDOR && PRODUCTS.iter().any(|(p, _)| *p == d.product_id()))
            .ok_or("no X52 found")?;
        let name = PRODUCTS
            .iter()
            .find(|(p, _)| *p == info.product_id())
            .map_or("?", |(_, n)| n);
        println!(
            "found {name} {:04x}:{:04x} bus {} addr {} ({:?})",
            info.vendor_id(),
            info.product_id(),
            info.busnum(),
            info.device_address(),
            info.product_string()
        );
        let sysfs = info.sysfs_path().to_path_buf();
        let device = info.open().wait().map_err(|e| format!("open: {e}"))?;
        Ok(Self { device, sysfs })
    }

    fn cmd(&self, index: u16, value: u16) -> Result<(), String> {
        println!("  -> wIndex=0x{index:02x} wValue=0x{value:04x} ({value:016b})");
        self.device
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
            .map_err(|e| format!("control_out(0x{index:02x}, 0x{value:04x}): {e}"))
    }

    fn interface_drivers(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.sysfs) {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.contains(':') {
                    let driver = fs::read_link(e.path().join("driver"))
                        .ok()
                        .and_then(|p| p.file_name().map(|f| f.to_string_lossy().into_owned()))
                        .unwrap_or_else(|| "<none>".into());
                    out.push(format!("{name}={driver}"));
                }
            }
        }
        out.sort();
        out
    }

    fn led_bit(&self, bit: u16, on: bool) -> Result<(), String> {
        self.cmd(0xb8, bit << 8 | u16::from(on))
    }

    fn led(&self, name: &str, state: &str) -> Result<(), String> {
        let (_, red, green) = LEDS
            .iter()
            .find(|(n, ..)| *n == name)
            .ok_or("unknown LED")?;
        match (red, state) {
            (None, "on") => self.led_bit(*green, true),
            (None, "off") => self.led_bit(*green, false),
            (Some(r), s) => {
                let (r_on, g_on) = match s {
                    "off" => (false, false),
                    "green" => (false, true),
                    "red" => (true, false),
                    "amber" => (true, true),
                    _ => return Err("state must be off|green|red|amber".into()),
                };
                self.led_bit(*green, g_on)?;
                self.led_bit(*r, r_on)
            }
            _ => Err("single-colour LED: state must be on|off".into()),
        }
    }

    fn text(&self, line: u8, bytes: &[u8]) -> Result<(), String> {
        let base = match line {
            1 => 0xd1,
            2 => 0xd2,
            3 => 0xd4,
            _ => return Err("line must be 1..3".into()),
        };
        let mut buf = bytes.to_vec();
        buf.truncate(16);
        self.cmd(base | 0x08, 0)?; // clear line
        if buf.len() % 2 == 1 {
            buf.push(b' ');
        }
        for pair in buf.chunks(2) {
            self.cmd(base, u16::from(pair[1]) << 8 | u16::from(pair[0]))?;
        }
        Ok(())
    }
}

fn parse_num(s: &str) -> Result<u16, String> {
    let r = if let Some(h) = s.strip_prefix("0x") {
        u16::from_str_radix(h, 16)
    } else {
        s.parse()
    };
    r.map_err(|e| format!("bad number {s}: {e}"))
}

fn parse_i32(s: &str) -> Result<i32, String> {
    s.parse().map_err(|e| format!("bad number {s}: {e}"))
}

fn parse_u8(s: &str) -> Result<u8, String> {
    u8::try_from(parse_num(s)?).map_err(|e| format!("{s} does not fit in a byte: {e}"))
}

/// Narrows a calendar or clock field to the 16-bit `wValue` range.
fn field<T>(value: T, what: &str) -> Result<u16, String>
where
    T: TryInto<u16> + Copy + std::fmt::Display,
{
    value
        .try_into()
        .map_err(|_| format!("{what} {value} does not fit in 16 bits"))
}

fn h24(s: Option<&String>) -> u16 {
    u16::from(s.is_none_or(|v| v != "12")) << 15
}

/// Unescape `\xNN` sequences so arbitrary bytes can be sent to the MFD.
fn unescape(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && b.get(i + 1) == Some(&b'x') && i + 3 < b.len() {
            let hex = std::str::from_utf8(&b[i + 2..i + 4]).map_err(|e| e.to_string())?;
            out.push(u8::from_str_radix(hex, 16).map_err(|e| e.to_string())?);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(out)
}

const USAGE: &str = "usage: x52-probe <command>
  probe                          open the device, send one harmless command, check HID binding
  raw <wIndex> <wValue>          send a raw vendor command (decimal or 0x..)
  led <name> <state>             name: fire a b d e t1 t2 t3 pov i throttle;
                                 state: on|off (fire, throttle), off|green|red|amber (others)
  leds <state>                   all bi-colour LEDs to <state>
  cycle                          S1 demo: all LEDs red, green, amber, off, green (1 s each)
  bright <led|mfd> <raw>         brightness raw wValue (old app sent 0..128 in steps of 4)
  text <1|2|3> <text>            write MFD line (\\xNN escapes allowed, max 16 bytes)
  charset <1|2|3> <start>        write bytes start..start+15 to the line
  clock1 <hh> <mm> [12|24]       set clock 1
  offset <2|3> <minutes> [12|24] set clock 2/3 offset in minutes (may be negative)
  date <dd> <mm> <yy>            raw date fields: 0xc4 <- mm<<8|dd, 0xc8 <- yy
  now                            sync date (DD-MM-YY fields) and clock 1 with local time, 24h
  shift <on|off>                 shift indicator
  blink <on|off>                 blink throttle/POV LEDs";

fn run(args: &[String]) -> Result<(), String> {
    let cmd = args.first().map(String::as_str).ok_or(USAGE)?;
    let arg = |i: usize| args.get(i).ok_or_else(|| USAGE.to_string());
    let x = X52::open()?;
    match cmd {
        "probe" => {
            println!("interfaces before: {:?}", x.interface_drivers());
            x.led("fire", "on")?;
            println!("control transfer OK");
            println!("interfaces after:  {:?}", x.interface_drivers());
        }
        "raw" => x.cmd(parse_num(arg(1)?)?, parse_num(arg(2)?)?)?,
        "led" => x.led(arg(1)?, arg(2)?)?,
        "leds" => {
            for (n, red, _) in LEDS {
                if red.is_some() {
                    x.led(n, arg(1)?)?;
                }
            }
        }
        "cycle" => {
            for state in ["red", "green", "amber", "off", "green"] {
                println!("all LEDs {state}");
                for (n, red, _) in LEDS {
                    let s = if red.is_some() {
                        state
                    } else if state == "off" {
                        "off"
                    } else {
                        "on"
                    };
                    x.led(n, s)?;
                }
                sleep(Duration::from_secs(1));
            }
            println!("interfaces after: {:?}", x.interface_drivers());
        }
        "bright" => {
            let index = match arg(1)?.as_str() {
                "mfd" => 0xb1,
                "led" => 0xb2,
                _ => return Err("bright led|mfd <raw>".into()),
            };
            x.cmd(index, parse_num(arg(2)?)?)?;
        }
        "text" => x.text(parse_u8(arg(1)?)?, &unescape(arg(2)?)?)?,
        "charset" => {
            let start = parse_u8(arg(2)?)?;
            let bytes: Vec<u8> = (0..16).map(|i| start.wrapping_add(i)).collect();
            x.text(parse_u8(arg(1)?)?, &bytes)?;
        }
        "clock1" => {
            let (h, m) = (parse_num(arg(1)?)?, parse_num(arg(2)?)?);
            x.cmd(0xc0, h24(args.get(3)) | h << 8 | m)?;
        }
        "offset" => {
            let index = match arg(1)?.as_str() {
                "2" => 0xc1,
                "3" => 0xc2,
                _ => return Err("offset 2|3 <minutes>".into()),
            };
            let minutes = parse_i32(arg(2)?)?;
            let sign = if minutes < 0 { 1 << 10 } else { 0 };
            let magnitude = field(minutes.unsigned_abs(), "offset")?;
            let value = h24(args.get(3)) | sign | (magnitude & 0x3ff);
            x.cmd(index, value)?;
        }
        "date" => {
            let (dd, mm, yy) = (
                parse_num(arg(1)?)?,
                parse_num(arg(2)?)?,
                parse_num(arg(3)?)?,
            );
            x.cmd(0xc4, mm << 8 | dd)?;
            x.cmd(0xc8, yy)?;
        }
        "now" => {
            let now = Local::now();
            let month = field(now.month(), "month")?;
            let day = field(now.day(), "day")?;
            let year = field(now.year().rem_euclid(100), "year")?;
            let hour = field(now.hour(), "hour")?;
            let minute = field(now.minute(), "minute")?;
            x.cmd(0xc4, month << 8 | day)?;
            x.cmd(0xc8, year)?;
            x.cmd(0xc0, 1 << 15 | hour << 8 | minute)?;
        }
        "shift" | "blink" => {
            let index = if cmd == "shift" { 0xfd } else { 0xb4 };
            x.cmd(index, if arg(1)? == "on" { 0x51 } else { 0x50 })?;
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
