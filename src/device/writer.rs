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

//! The device writer: the thread that owns the opened device and keeps it showing the desired
//! state (docs/architecture.md, "Threads and data flow", "Writer").
//!
//! On every wake the writer resolves the desired state at the current instant and sends exactly
//! the commands whose transfers differ from what it last sent successfully. A newly opened device
//! starts with nothing sent, so its first wake is the full replay; a clock that moved (a minute,
//! midnight, daylight saving time, a resume, a wall-clock jump) differs from what was sent and is
//! corrected at the next wake, at most [`WAKE_INTERVAL`] later.

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

use jiff::tz::TimeZone;
use x52::clock::{self, Clock, ClockError};
use x52::command::{CommandSlot, ControlTransfer};
use x52::state::DeviceState;
use x52::transport::{self, SendError, Transport};

use super::access::AccessState;
use super::id::{DeviceId, Generation};
use super::monotonic::Monotonic;
use super::usb::{OpenError, Opener};

/// The longest the writer sleeps. Monotonic time does not advance during suspend, so a pending
/// wait ends at most this long after a resume, and the clocks are re-sent then.
pub(crate) const WAKE_INTERVAL: Duration = Duration::from_secs(5);

/// How long the writer waits before opening the selected device again after a denied permission
/// or a failure.
pub(crate) const RETRY_INTERVAL: Duration = Duration::from_secs(1);

/// A message to the writer.
#[derive(Debug)]
pub(crate) enum WriterMessage {
    /// The desired state; only the latest one queued counts.
    SetState(DeviceState),
    /// The device watch selected a device, or a new attachment of it.
    Selected(DeviceId),
    /// The selection of that generation is gone.
    Gone(Generation),
    /// The user asked to try opening the device again.
    Retry,
    /// Stop the thread.
    Shutdown,
}

/// What the writer reports to the main thread.
#[derive(Debug)]
pub(crate) enum WriterEvent {
    /// The access state changed.
    Access(AccessState),
    /// Something failed; reported once until it recovers.
    Error(WriterError),
}

/// Why the writer could not show the desired state.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WriterError {
    /// The selected device could not be opened, for a reason other than the permission.
    #[error("cannot open the device")]
    Open(#[source] OpenError),
    /// A control transfer failed; the device is closed and opened again.
    #[error("sending the {command} command failed")]
    Transfer {
        /// The command's name.
        command: &'static str,
        /// The transport's error.
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
    /// A clock value could not be computed.
    #[error("cannot compute the clocks")]
    Clock(#[source] ClockError),
}

/// The connection to the selected device.
#[derive(Debug)]
enum Link<T> {
    /// Nothing is selected.
    Idle,
    /// A device is selected but not open; it is opened at `retry_at` (monotonic).
    Waiting { retry_at: Duration },
    /// The device is open.
    Open {
        transport: T,
        /// Per slot, the transfers last sent successfully.
        sent: HashMap<CommandSlot, Vec<ControlTransfer>>,
    },
}

/// The writer's state, driven by [`Writer::run`]; the tests drive it step by step.
pub(crate) struct Writer<O: Opener, C, M> {
    opener: O,
    clock: C,
    monotonic: M,
    /// The time zone clock 1 follows when it shows local time, read on every resolve.
    local_zone: fn() -> TimeZone,
    events: async_channel::Sender<WriterEvent>,
    desired: Option<DeviceState>,
    selected: Option<DeviceId>,
    link: Link<O::Transport>,
    /// The access state last reported.
    access: AccessState,
    /// Whether a failure has been reported since the last successful open or new selection.
    failure_reported: bool,
}

impl<O, C, M> Writer<O, C, M>
where
    O: Opener,
    <O::Transport as Transport>::Error: Send + Sync,
    C: Clock,
    M: Monotonic,
{
    /// A writer with nothing selected and no desired state.
    pub(crate) fn new(
        opener: O,
        clock: C,
        monotonic: M,
        local_zone: fn() -> TimeZone,
        events: async_channel::Sender<WriterEvent>,
    ) -> Self {
        Self {
            opener,
            clock,
            monotonic,
            local_zone,
            events,
            desired: None,
            selected: None,
            link: Link::Idle,
            access: AccessState::NoDevice,
            failure_reported: false,
        }
    }

    /// Runs until [`WriterMessage::Shutdown`] or until every sender is gone.
    pub(crate) fn run(mut self, messages: &Receiver<WriterMessage>) {
        while self.step(messages).is_continue() {}
    }

    /// Waits for a message or the next scheduled wake, handles every queued message, then wakes.
    pub(crate) fn step(&mut self, messages: &Receiver<WriterMessage>) -> ControlFlow<()> {
        match messages.recv_timeout(self.timeout()) {
            Ok(message) => {
                self.handle(message)?;
                // Coalescing: everything already queued is handled before the one wake.
                while let Ok(message) = messages.try_recv() {
                    self.handle(message)?;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return ControlFlow::Break(()),
        }
        self.wake();
        ControlFlow::Continue(())
    }

    /// Applies one message to the writer's state; nothing is sent until the next wake.
    pub(crate) fn handle(&mut self, message: WriterMessage) -> ControlFlow<()> {
        match message {
            WriterMessage::SetState(state) => self.desired = Some(state),
            WriterMessage::Selected(device) => {
                let now = self.monotonic.elapsed();
                if self.selected.as_ref() == Some(&device) {
                    if let Link::Waiting { retry_at } = &mut self.link {
                        *retry_at = now;
                    }
                } else {
                    // A new selection or a new attachment: the old handle and cache go.
                    self.selected = Some(device);
                    self.link = Link::Waiting { retry_at: now };
                    self.failure_reported = false;
                }
            }
            WriterMessage::Gone(generation) => {
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|device| device.generation == generation)
                {
                    self.selected = None;
                    self.link = Link::Idle;
                    self.report_access(AccessState::NoDevice);
                }
            }
            WriterMessage::Retry => {
                let now = self.monotonic.elapsed();
                if let Link::Waiting { retry_at } = &mut self.link {
                    *retry_at = now;
                }
            }
            WriterMessage::Shutdown => return ControlFlow::Break(()),
        }
        ControlFlow::Continue(())
    }

    /// Opens the selected device if it is due, then sends what differs from the cache.
    pub(crate) fn wake(&mut self) {
        let now = self.monotonic.elapsed();
        if let Link::Waiting { retry_at } = self.link
            && now >= retry_at
        {
            self.open(now);
        }
        self.sync(now);
    }

    /// How long to wait for a message before the next wake: until the next clock change, the
    /// next open attempt or [`WAKE_INTERVAL`], whichever comes first.
    pub(crate) fn timeout(&self) -> Duration {
        let mut timeout = WAKE_INTERVAL;
        match &self.link {
            Link::Idle => {}
            Link::Waiting { retry_at } => {
                timeout = timeout.min(retry_at.saturating_sub(self.monotonic.elapsed()));
            }
            Link::Open { .. } => {
                if let Some(until_resend) = self.until_resend() {
                    timeout = timeout.min(until_resend);
                }
            }
        }
        timeout
    }

    /// The wall-clock time until the clocks of the desired state change, if it can be computed.
    fn until_resend(&self) -> Option<Duration> {
        let state = self.desired.as_ref()?;
        let now = self.clock.now();
        let next = clock::next_resend(
            now,
            &(self.local_zone)(),
            state.clock1.source,
            &[&state.clock2.source, &state.clock3.source],
        )
        .ok()?;
        now.duration_until(next).try_into().ok()
    }

    fn open(&mut self, now: Duration) {
        let Some(device) = &self.selected else {
            return;
        };
        match self.opener.open(device) {
            Ok(transport) => {
                tracing::info!(
                    "Opened the {} at {} (generation {})",
                    device.device_type,
                    device.sysfs_path.display(),
                    device.generation
                );
                self.link = Link::Open {
                    transport,
                    sent: HashMap::new(),
                };
                self.failure_reported = false;
                self.report_access(AccessState::Ready);
            }
            Err(OpenError::PermissionDenied) => {
                self.link = Link::Waiting {
                    retry_at: now + RETRY_INTERVAL,
                };
                self.report_access(AccessState::NoPermission);
            }
            Err(error) => {
                self.link = Link::Waiting {
                    retry_at: now + RETRY_INTERVAL,
                };
                self.report_failure(WriterError::Open(error));
            }
        }
    }

    /// Sends the commands of the desired state whose transfers differ from the cache.
    fn sync(&mut self, now: Duration) {
        let (Some(state), Some(device)) = (&self.desired, &self.selected) else {
            return;
        };
        let Link::Open { transport, sent } = &mut self.link else {
            return;
        };
        let device_type = device.device_type;
        let commands = match state.resolve(device_type, self.clock.now(), &(self.local_zone)()) {
            Ok(commands) => commands,
            Err(error) => {
                self.report_failure(WriterError::Clock(error));
                return;
            }
        };
        for command in commands {
            let transfers = command.encode();
            if sent.get(&command.slot()) == Some(&transfers) {
                continue;
            }
            match transport::send(transport, device_type, &command) {
                Ok(()) => {
                    sent.insert(command.slot(), transfers);
                }
                Err(error) => {
                    let source: Box<dyn std::error::Error + Send + Sync> = match error {
                        SendError::Transport(source) => Box::new(source),
                        unsupported @ SendError::Unsupported { .. } => Box::new(unsupported),
                    };
                    // The handle and the cache go; the device is opened again later.
                    self.link = Link::Waiting {
                        retry_at: now + RETRY_INTERVAL,
                    };
                    self.report_failure(WriterError::Transfer {
                        command: command.name(),
                        source,
                    });
                    return;
                }
            }
        }
    }

    fn report_access(&mut self, access: AccessState) {
        if self.access != access {
            self.access = access;
            tracing::info!("Device access: {access:?}");
            self.send(WriterEvent::Access(access));
        }
    }

    fn report_failure(&mut self, error: WriterError) {
        if !self.failure_reported {
            self.failure_reported = true;
            tracing::warn!("{}", error_chain(&error));
            self.send(WriterEvent::Error(error));
        }
    }

    fn send(&self, event: WriterEvent) {
        // An unbounded channel only fails once the main thread has stopped listening.
        let _ = self.events.try_send(event);
    }
}

/// `error` and its sources, joined with `: `.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    message
}

#[cfg(test)]
mod tests;
