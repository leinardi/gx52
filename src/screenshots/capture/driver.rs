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

//! The order of the screenshot run, without GTK: set a shot up, poll until it is ready, wait for
//! frames, capture, and go on; or fail, naming the shot and the condition.

use std::time::Duration;

/// How many frame-clock ticks pass between a ready shot and its capture, so the last layout is
/// painted.
pub(crate) const FRAMES_BEFORE_CAPTURE: u32 = 2;

/// What the driver knows of a shot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShotPlan {
    /// The shot's name: its PNG file name without the extension.
    pub(crate) name: &'static str,
    /// How long the shot may take to become ready after its set-up.
    pub(crate) timeout: Duration,
}

/// What happened since the driver's last action. Every input carries the time elapsed since the
/// run started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Input {
    /// The run starts.
    Start,
    /// The shot was polled: ready, or the first condition that does not hold yet.
    Polled(Result<(), &'static str>),
    /// The frame clock ticked.
    Frame,
    /// Time passed while waiting for a frame.
    Idle,
    /// The shot was saved.
    Captured,
}

/// What the GTK side does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    /// Sets shot `index` up, then polls it.
    SetUp(usize),
    /// Polls shot `index` again later.
    Poll(usize),
    /// Waits for the next frame-clock tick, or reports [`Input::Idle`].
    WaitFrame,
    /// Captures shot `index`.
    Capture(usize),
    /// Every shot is saved.
    Done,
    /// The run failed.
    Fail {
        /// The shot that failed.
        shot: &'static str,
        /// The condition that never held.
        condition: &'static str,
    },
}

/// Where the run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    NotStarted,
    /// Polling shot `index`, set up at `since`.
    Polling {
        index: usize,
        since: Duration,
    },
    /// Shot `index` is ready; `left` frames still to come.
    Frames {
        index: usize,
        left: u32,
    },
    /// Shot `index` is being captured.
    Capturing {
        index: usize,
    },
    Finished,
}

/// The run's state machine: see the module documentation.
#[derive(Debug)]
pub(crate) struct Driver {
    shots: Vec<ShotPlan>,
    /// The longest the whole run may take, frame waits included.
    deadline: Duration,
    state: State,
}

impl Driver {
    /// A run of `shots`, in order, that fails once `deadline` has passed.
    pub(crate) fn new(shots: Vec<ShotPlan>, deadline: Duration) -> Self {
        Self {
            shots,
            deadline,
            state: State::NotStarted,
        }
    }

    /// The next action, after `input` at `elapsed` since the run started.
    ///
    /// # Panics
    ///
    /// When `input` cannot come in the current state: the GTK side broke the protocol.
    pub(crate) fn step(&mut self, input: Input, elapsed: Duration) -> Action {
        let action = self.next(input, elapsed);
        if let Action::Fail { .. } | Action::Done = action {
            self.state = State::Finished;
        }
        action
    }

    fn next(&mut self, input: Input, elapsed: Duration) -> Action {
        if let Some(index) = self.current()
            && elapsed > self.deadline
        {
            return Action::Fail {
                shot: self.shots[index].name,
                condition: "the run finished within its overall deadline",
            };
        }
        match (self.state, input) {
            (State::NotStarted, Input::Start) => self.set_up(0, elapsed),
            (State::Polling { index, since }, Input::Polled(result)) => match result {
                Ok(()) => {
                    self.state = State::Frames {
                        index,
                        left: FRAMES_BEFORE_CAPTURE,
                    };
                    Action::WaitFrame
                }
                Err(condition) if elapsed.saturating_sub(since) > self.shots[index].timeout => {
                    Action::Fail {
                        shot: self.shots[index].name,
                        condition,
                    }
                }
                Err(_) => Action::Poll(index),
            },
            (State::Frames { index, left }, Input::Frame) => {
                if left <= 1 {
                    self.state = State::Capturing { index };
                    Action::Capture(index)
                } else {
                    self.state = State::Frames {
                        index,
                        left: left - 1,
                    };
                    Action::WaitFrame
                }
            }
            (State::Frames { .. }, Input::Idle) => Action::WaitFrame,
            (State::Capturing { index }, Input::Captured) => self.set_up(index + 1, elapsed),
            (state, input) => panic!("screenshot driver: {input:?} cannot come in {state:?}"),
        }
    }

    /// Sets shot `index` up, or ends the run when there is none.
    fn set_up(&mut self, index: usize, elapsed: Duration) -> Action {
        if index < self.shots.len() {
            self.state = State::Polling {
                index,
                since: elapsed,
            };
            Action::SetUp(index)
        } else {
            Action::Done
        }
    }

    /// The shot the run is at, if it is running.
    fn current(&self) -> Option<usize> {
        match self.state {
            State::Polling { index, .. }
            | State::Frames { index, .. }
            | State::Capturing { index } => Some(index),
            State::NotStarted | State::Finished => None,
        }
    }
}

#[cfg(test)]
mod tests;
