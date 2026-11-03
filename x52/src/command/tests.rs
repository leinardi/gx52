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

//! One test per row of docs/protocol.md "Commands the app may send", each sending the command
//! through [`send`] to a [`RecordingTransport`] and checking the exact `(wIndex, wValue)` pairs,
//! at the extremes of every range; then the constructors' rejections at each boundary.

use super::*;
use crate::transport::{RecordingTransport, send};

/// What `command` puts on the wire of an X52 Pro, which has every command.
fn wire(command: Command) -> Vec<(u16, u16)> {
    let mut transport = RecordingTransport::new();
    send(&mut transport, DeviceType::X52Pro, &command)
        .unwrap_or_else(|error| panic!("{command:?}: {error}"));
    transport.pairs()
}

fn brightness(level: u8) -> Brightness {
    Brightness::new(level).unwrap_or_else(|error| panic!("{error}"))
}

fn offset(minutes: i16) -> ClockOffset {
    ClockOffset::new(minutes).unwrap_or_else(|error| panic!("{error}"))
}

fn text(text: &str) -> MfdText {
    MfdText::new(text).unwrap_or_else(|error| panic!("{error}"))
}

#[test]
fn row_0xb1_mfd_brightness() {
    assert_eq!(wire(Command::MfdBrightness(brightness(0))), [(0xb1, 0)]);
    assert_eq!(wire(Command::MfdBrightness(brightness(128))), [(0xb1, 128)]);
}

#[test]
fn row_0xb2_led_brightness() {
    assert_eq!(wire(Command::LedBrightness(brightness(0))), [(0xb2, 0)]);
    assert_eq!(wire(Command::LedBrightness(brightness(128))), [(0xb2, 128)]);
}

#[test]
fn row_0xb4_blink() {
    assert_eq!(wire(Command::Blink(true)), [(0xb4, 0x51)]);
    assert_eq!(wire(Command::Blink(false)), [(0xb4, 0x50)]);
}

#[test]
fn row_0xb8_led_green_bit_then_red_bit() {
    let colours = [
        (LedColour::Off, 0, 0),
        (LedColour::Green, 1, 0),
        (LedColour::Red, 0, 1),
        (LedColour::Amber, 1, 1),
    ];
    for led in BiColourLed::ALL {
        let green = u16::from(led.green_bit()) << 8;
        let red = u16::from(led.red_bit()) << 8;
        for (colour, green_on, red_on) in colours {
            assert_eq!(
                wire(Command::Led(led, colour)),
                [(0xb8, green | green_on), (0xb8, red | red_on)],
                "{led:?} {colour:?}"
            );
        }
    }
    // The extremes of the bit range.
    assert_eq!(
        wire(Command::Led(BiColourLed::ButtonA, LedColour::Amber)),
        [(0xb8, 0x0301), (0xb8, 0x0201)]
    );
    assert_eq!(
        wire(Command::Led(BiColourLed::ButtonI, LedColour::Off)),
        [(0xb8, 0x1300), (0xb8, 0x1200)]
    );
}

#[test]
fn row_0xb8_single_colour_led() {
    assert_eq!(
        wire(Command::LedSwitch(SingleColourLed::Fire, true)),
        [(0xb8, 0x0101)]
    );
    assert_eq!(
        wire(Command::LedSwitch(SingleColourLed::Fire, false)),
        [(0xb8, 0x0100)]
    );
    assert_eq!(
        wire(Command::LedSwitch(SingleColourLed::Throttle, true)),
        [(0xb8, 0x1401)]
    );
    assert_eq!(
        wire(Command::LedSwitch(SingleColourLed::Throttle, false)),
        [(0xb8, 0x1400)]
    );
}

#[test]
fn row_0xc0_clock_1() {
    let clock = |hour, minute, h24| Command::Clock1 {
        time: ClockTime::new(hour, minute).unwrap_or_else(|error| panic!("{error}")),
        h24,
    };
    assert_eq!(wire(clock(0, 0, false)), [(0xc0, 0x0000)]);
    assert_eq!(wire(clock(23, 59, true)), [(0xc0, 0x8000 | 0x173b)]);
    assert_eq!(wire(clock(20, 46, false)), [(0xc0, 0x142e)]);
}

#[test]
fn rows_0xc1_0xc2_clock_offsets_sign_magnitude() {
    let cases = [
        (0, false, 0x0000),
        (720, false, 720),
        (-720, false, 0x0400 | 0x02d0),
        (-1, true, 0x8401),
        (60, true, 0x8000 | 0x003c),
        (-120, false, 0x0400 | 0x0078),
    ];
    for (clock, command) in [(OffsetClock::Clock2, 0xc1), (OffsetClock::Clock3, 0xc2)] {
        for (minutes, h24, value) in cases {
            assert_eq!(
                wire(Command::ClockOffset {
                    clock,
                    offset: offset(minutes),
                    h24,
                }),
                [(command, value)],
                "{clock:?} {minutes} {h24}"
            );
        }
    }
}

#[test]
fn rows_0xc4_0xc8_date() {
    let date = |first, second, third| {
        Command::Date(
            DateFields::new(first, second, third).unwrap_or_else(|error| panic!("{error}")),
        )
    };
    assert_eq!(wire(date(28, 10, 26)), [(0xc4, 0x0a1c), (0xc8, 26)]);
    assert_eq!(wire(date(0, 0, 0)), [(0xc4, 0), (0xc8, 0)]);
    assert_eq!(wire(date(99, 99, 99)), [(0xc4, 0x6363), (0xc8, 99)]);
}

#[test]
fn rows_0xd1_to_0xdc_mfd_text_clear_then_pairs() {
    for (line, write, clear) in [
        (MfdLine::One, 0xd1, 0xd9),
        (MfdLine::Two, 0xd2, 0xda),
        (MfdLine::Three, 0xd4, 0xdc),
    ] {
        assert_eq!(
            wire(Command::MfdText(line, text("GX52"))),
            [
                (clear, 0),
                (write, u16::from(b'X') << 8 | u16::from(b'G')),
                (write, 0x3235)
            ],
            "{line:?}"
        );
        // An empty line is a clear and nothing else.
        assert_eq!(wire(Command::MfdText(line, text(""))), [(clear, 0)]);
    }
}

#[test]
fn mfd_text_pads_an_odd_last_byte_with_a_space() {
    assert_eq!(
        wire(Command::MfdText(MfdLine::One, text("abc"))),
        [(0xd9, 0), (0xd1, 0x6261), (0xd1, 0x2063)]
    );
}

#[test]
fn mfd_text_of_16_bytes_is_8_writes() {
    let pairs = wire(Command::MfdText(MfdLine::Two, text(&"¯".repeat(16))));
    assert_eq!(pairs.len(), 9);
    assert_eq!(pairs.first(), Some(&(0xda, 0)));
    assert!(pairs.iter().skip(1).all(|pair| *pair == (0xd2, 0xffff)));
}

#[test]
fn rows_0xfd_shift() {
    assert_eq!(wire(Command::Shift(true)), [(0xfd, 0x51)]);
    assert_eq!(wire(Command::Shift(false)), [(0xfd, 0x50)]);
}

#[test]
fn brightness_rejects_above_128() {
    assert_eq!(Brightness::new(128).map(Brightness::level), Ok(128));
    assert_eq!(
        Brightness::new(129),
        Err(RangeError::OutOfRange {
            field: "brightness",
            value: 129,
            min: 0,
            max: 128
        })
    );
    assert!(Brightness::new(255).is_err());
}

#[test]
fn clock_time_rejects_hour_24_and_minute_60() {
    assert!(ClockTime::new(23, 59).is_ok());
    assert_eq!(
        ClockTime::new(24, 0),
        Err(RangeError::OutOfRange {
            field: "hour",
            value: 24,
            min: 0,
            max: 23
        })
    );
    assert_eq!(
        ClockTime::new(0, 60),
        Err(RangeError::OutOfRange {
            field: "minute",
            value: 60,
            min: 0,
            max: 59
        })
    );
}

#[test]
fn clock_offset_rejects_beyond_half_a_day() {
    assert!(ClockOffset::new(720).is_ok());
    assert!(ClockOffset::new(-720).is_ok());
    assert!(ClockOffset::new(721).is_err());
    assert_eq!(
        ClockOffset::new(-721),
        Err(RangeError::OutOfRange {
            field: "clock offset",
            value: -721,
            min: -720,
            max: 720
        })
    );
    assert!(ClockOffset::new(i16::MIN).is_err());
    assert!(ClockOffset::new(i16::MAX).is_err());
}

#[test]
fn date_fields_reject_100() {
    assert!(DateFields::new(99, 99, 99).is_ok());
    for (fields, field) in [
        ((100, 0, 0), "first date field"),
        ((0, 100, 0), "second date field"),
        ((0, 0, 100), "third date field"),
    ] {
        let (first, second, third) = fields;
        assert_eq!(
            DateFields::new(first, second, third),
            Err(RangeError::OutOfRange {
                field,
                value: 100,
                min: 0,
                max: 99
            })
        );
    }
}

#[test]
fn mfd_bytes_reject_17_bytes_and_blank_bytes() {
    assert!(MfdBytes::new(&[b'x'; 16]).is_ok());
    assert_eq!(
        MfdBytes::new(&[b'x'; 17]),
        Err(RangeError::OutOfRange {
            field: "MFD line length",
            value: 17,
            min: 0,
            max: 16
        })
    );
    for blank in (0x00..=0x07).chain([0xa0]) {
        assert_eq!(
            MfdBytes::new(&[b'a', blank]),
            Err(RangeError::BlankByte {
                byte: blank,
                position: 1
            })
        );
    }
    // The bytes next to the blanks are glyphs.
    assert!(MfdBytes::new(&[0x08, 0x9f, 0xa1, 0x20]).is_ok());
}

#[test]
fn mfd_bytes_keep_what_was_given() {
    assert_eq!(text("hello").as_bytes(), b"hello");
    assert_eq!(text("").as_bytes(), b"");
}

#[test]
fn every_command_is_on_the_allowlist() {
    let allowlist = [
        0xb1, 0xb2, 0xb4, 0xb8, 0xc0, 0xc1, 0xc2, 0xc4, 0xc8, 0xd1, 0xd2, 0xd4, 0xd9, 0xda, 0xdc,
        0xfd,
    ];
    let commands = [
        Command::MfdBrightness(brightness(1)),
        Command::LedBrightness(brightness(1)),
        Command::Blink(true),
        Command::Led(BiColourLed::Pov2, LedColour::Amber),
        Command::LedSwitch(SingleColourLed::Throttle, true),
        Command::Clock1 {
            time: ClockTime::new(1, 2).unwrap_or_else(|error| panic!("{error}")),
            h24: true,
        },
        Command::ClockOffset {
            clock: OffsetClock::Clock3,
            offset: offset(-5),
            h24: false,
        },
        Command::Date(DateFields::new(1, 2, 3).unwrap_or_else(|error| panic!("{error}"))),
        Command::MfdText(MfdLine::Three, text("x")),
        Command::Shift(false),
    ];
    for command in commands {
        for transfer in command.encode() {
            assert!(
                allowlist.contains(&transfer.index()),
                "{command:?} sends {:#04x}",
                transfer.index()
            );
        }
    }
}
