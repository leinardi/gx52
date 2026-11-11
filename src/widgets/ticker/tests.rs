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

use super::*;

/// A zone whose transitions are 30 seconds past the minute (2026-03-29 and 2026-10-25), so a
/// transition comes before the next minute boundary (as in x52's `next_resend` tests).
fn odd_zone() -> TimeZone {
    TimeZone::posix("XST0XDT,M3.5.0/0:00:30,M10.5.0/0:00:30").unwrap()
}

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn utc_only() -> TickSources {
    TickSources {
        clock1: Clock1Source::Utc,
        zones: Vec::new(),
    }
}

#[test]
fn the_delay_follows_the_sources_shown() {
    let local = odd_zone();
    let now = at("2026-03-29T00:00:10Z");
    assert_eq!(
        utc_only().delay(now, &local),
        Duration::from_secs(50),
        "clock 1 on UTC ignores the local transition"
    );
    assert_eq!(
        TickSources::default().delay(now, &local),
        Duration::from_secs(20),
        "clock 1 on local time ticks at the transition"
    );
    let zone_clock = TickSources {
        zones: vec![ZoneSource::Zone(odd_zone())],
        ..utc_only()
    };
    assert_eq!(
        zone_clock.delay(now, &TimeZone::UTC),
        Duration::from_secs(20),
        "a clock 2 or 3 zone ticks at its transition"
    );
}

#[test]
fn a_change_of_sources_reschedules_the_pending_tick() {
    // The timer was computed with clock 1 on UTC (50 s away); clock 1 then switches to local
    // time, whose transition is 20 s away, so the timer must be computed again.
    let ticker = Ticker::default();
    assert!(
        ticker.needs_reschedule(&utc_only()),
        "nothing is scheduled yet"
    );
    ticker.sources.replace(Some(utc_only()));
    assert!(
        !ticker.needs_reschedule(&utc_only()),
        "the same clocks keep it"
    );
    assert!(ticker.needs_reschedule(&TickSources::default()));
    let with_zone = TickSources {
        zones: vec![ZoneSource::Zone(odd_zone())],
        ..utc_only()
    };
    assert!(
        ticker.needs_reschedule(&with_zone),
        "a new zone for clock 2"
    );
    ticker.stop();
    assert!(
        ticker.needs_reschedule(&utc_only()),
        "a stopped ticker has no timer to keep"
    );
}
