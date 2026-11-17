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

const SECOND: Duration = Duration::from_secs(1);

fn driver() -> Driver {
    Driver::new(
        vec![
            ShotPlan {
                name: "leds",
                timeout: 5 * SECOND,
            },
            ShotPlan {
                name: "mfd",
                timeout: 5 * SECOND,
            },
        ],
        60 * SECOND,
    )
}

/// Reports the shot ready and lets the frames pass, an idle wait among them: the action after
/// the last frame.
fn ready_and_frames(driver: &mut Driver, at: Duration) -> Action {
    assert_eq!(driver.step(Input::Polled(Ok(())), at), Action::WaitFrame);
    assert_eq!(driver.step(Input::Idle, at), Action::WaitFrame);
    for _ in 1..FRAMES_BEFORE_CAPTURE {
        assert_eq!(driver.step(Input::Frame, at), Action::WaitFrame);
    }
    driver.step(Input::Frame, at)
}

#[test]
fn every_shot_is_set_up_polled_framed_and_captured_in_order() {
    let mut driver = driver();
    assert_eq!(driver.step(Input::Start, Duration::ZERO), Action::SetUp(0));
    assert_eq!(
        driver.step(Input::Polled(Err("not yet")), SECOND),
        Action::Poll(0)
    );
    assert_eq!(ready_and_frames(&mut driver, SECOND), Action::Capture(0));
    assert_eq!(driver.step(Input::Captured, SECOND), Action::SetUp(1));
    assert_eq!(
        ready_and_frames(&mut driver, 2 * SECOND),
        Action::Capture(1)
    );
    assert_eq!(driver.step(Input::Captured, 2 * SECOND), Action::Done);
}

#[test]
fn a_shot_that_never_gets_ready_fails_after_its_own_timeout() {
    let mut driver = driver();
    driver.step(Input::Start, Duration::ZERO);
    driver.step(Input::Polled(Ok(())), SECOND);
    driver.step(Input::Frame, SECOND);
    driver.step(Input::Frame, SECOND);
    // The second shot is set up at 2 s: its timeout counts from there, not from the start.
    assert_eq!(driver.step(Input::Captured, 2 * SECOND), Action::SetUp(1));
    assert_eq!(
        driver.step(Input::Polled(Err("the editor is mapped")), 7 * SECOND),
        Action::Poll(1)
    );
    assert_eq!(
        driver.step(
            Input::Polled(Err("the editor is mapped")),
            7 * SECOND + Duration::from_millis(1)
        ),
        Action::Fail {
            shot: "mfd",
            condition: "the editor is mapped",
        }
    );
}

#[test]
fn a_frame_clock_that_never_ticks_hits_the_overall_deadline() {
    let mut driver = driver();
    driver.step(Input::Start, Duration::ZERO);
    assert_eq!(
        driver.step(Input::Polled(Ok(())), SECOND),
        Action::WaitFrame
    );
    assert_eq!(driver.step(Input::Idle, 60 * SECOND), Action::WaitFrame);
    assert_eq!(
        driver.step(Input::Idle, 60 * SECOND + Duration::from_millis(1)),
        Action::Fail {
            shot: "leds",
            condition: "the run finished within its overall deadline",
        }
    );
}

#[test]
fn the_deadline_also_ends_a_shot_still_within_its_own_timeout() {
    let mut driver = Driver::new(
        vec![ShotPlan {
            name: "leds",
            timeout: 120 * SECOND,
        }],
        60 * SECOND,
    );
    driver.step(Input::Start, Duration::ZERO);
    assert!(matches!(
        driver.step(Input::Polled(Err("not yet")), 61 * SECOND),
        Action::Fail { shot: "leds", .. }
    ));
}

#[test]
fn an_empty_run_is_done_at_once() {
    let mut driver = Driver::new(Vec::new(), 60 * SECOND);
    assert_eq!(driver.step(Input::Start, Duration::ZERO), Action::Done);
}

#[test]
#[should_panic(expected = "cannot come in")]
fn an_input_out_of_order_is_a_bug() {
    let mut driver = driver();
    driver.step(Input::Frame, Duration::ZERO);
}
