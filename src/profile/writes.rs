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

//! The write queue: every save and delete of one profile, one at a time (docs/architecture.md,
//! "Persistence").
//!
//! A profile is saved a short moment after its last edit ([`DEBOUNCE`]), and removed in two steps:
//! taken out of the store at once, its file deleted when the Undo toast goes away. The queue keeps
//! one lane per profile so a slow save can never land after the delete, and it counts the creates
//! (new, duplicated and imported profiles) that have no lane yet, so the application can stay
//! alive until every file is written ([`WriteQueue::is_idle`]).
//!
//! The queue only decides; it returns [`Command`]s that the application carries out (timers, the
//! jobs of [`super::store`]) and reports back, so every order of events is tested without GTK.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::ProfileId;

/// How long after the last edit a profile is saved (docs/ui.md, "Profile editor").
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(500);

/// A file operation of one profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Op {
    /// Write the profile's file from its in-memory values.
    Save,
    /// Delete the file of a removed profile.
    Delete,
}

/// What the application has to do for the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    /// (Re)start the profile's debounce timer; when it fires, call
    /// [`WriteQueue::debounce_fired`].
    ArmDebounce(ProfileId),
    /// Stop the profile's debounce timer.
    CancelDebounce(ProfileId),
    /// Run the operation off the main thread; when it ends, call [`WriteQueue::finished`].
    Start(ProfileId, Op),
}

/// How an operation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// The file was written or deleted.
    Done,
    /// It failed: a failed save leaves the edits in memory for the next one; after a failed
    /// delete the caller puts the profile back.
    Failed,
}

/// A create that has no lane yet: see [`WriteQueue::create_started`].
#[derive(Debug, PartialEq, Eq)]
#[must_use = "a create keeps the queue busy until it is committed"]
pub(crate) struct CreateToken(u64);

/// One profile's state in the queue.
#[derive(Debug, Default)]
struct Lane {
    /// The debounce timer runs.
    armed: bool,
    /// The operation running off the main thread.
    in_flight: Option<Op>,
    /// The operation to start when the one in flight ends.
    next: Option<Op>,
    /// Taken out of the store; saves are ignored until it is restored.
    removing: bool,
    /// Edits a removal cancelled the save of, to save again when the removal is undone.
    unsaved: bool,
}

impl Lane {
    fn is_idle(&self) -> bool {
        !self.armed && self.in_flight.is_none() && self.next.is_none() && !self.removing
    }
}

/// Every pending save, delete and create.
#[derive(Debug, Default)]
pub(crate) struct WriteQueue {
    lanes: BTreeMap<ProfileId, Lane>,
    creates: BTreeSet<u64>,
    next_create: u64,
}

impl WriteQueue {
    /// `id` was edited: its save is due [`DEBOUNCE`] after the last edit. Ignored while it is
    /// removed.
    pub(crate) fn edited(&mut self, id: &ProfileId) -> Vec<Command> {
        let lane = self.lanes.entry(id.clone()).or_default();
        if lane.removing {
            return Vec::new();
        }
        lane.armed = true;
        vec![Command::ArmDebounce(id.clone())]
    }

    /// `id`'s debounce timer fired: its save starts, or follows the operation in flight.
    pub(crate) fn debounce_fired(&mut self, id: &ProfileId) -> Vec<Command> {
        let Some(lane) = self.lanes.get_mut(id) else {
            return Vec::new();
        };
        if !lane.armed {
            return Vec::new();
        }
        lane.armed = false;
        self.queue(id, Op::Save)
    }

    /// Starts every save that waits for its debounce, for the drain before the app exits.
    pub(crate) fn flush(&mut self) -> Vec<Command> {
        let armed: Vec<ProfileId> = self
            .lanes
            .iter()
            .filter(|(_, lane)| lane.armed)
            .map(|(id, _)| id.clone())
            .collect();
        let mut commands = Vec::new();
        for id in armed {
            commands.push(Command::CancelDebounce(id.clone()));
            commands.extend(self.debounce_fired(&id));
        }
        commands
    }

    /// The user removed `id`: a save not started yet is dropped (and remembered for an undo);
    /// a save in flight finishes first, before any delete.
    pub(crate) fn removed(&mut self, id: &ProfileId) -> Vec<Command> {
        let lane = self.lanes.entry(id.clone()).or_default();
        let mut commands = Vec::new();
        if lane.armed {
            lane.armed = false;
            lane.unsaved = true;
            commands.push(Command::CancelDebounce(id.clone()));
        }
        if lane.next == Some(Op::Save) {
            lane.next = None;
            lane.unsaved = true;
        }
        lane.removing = true;
        commands
    }

    /// The removal of `id` was undone before its delete started: the edits the removal
    /// cancelled the save of are saved again.
    pub(crate) fn undone(&mut self, id: &ProfileId) -> Vec<Command> {
        let Some(lane) = self.lanes.get_mut(id) else {
            return Vec::new();
        };
        if !lane.removing || lane.in_flight == Some(Op::Delete) || lane.next == Some(Op::Delete) {
            return Vec::new();
        }
        lane.removing = false;
        let commands = Self::rearm(id, lane);
        self.tidy(id);
        commands
    }

    /// The removal of `id` can no longer be undone: its file is deleted, after the operation in
    /// flight.
    pub(crate) fn delete_due(&mut self, id: &ProfileId) -> Vec<Command> {
        match self.lanes.get(id) {
            Some(lane)
                if lane.removing && lane.next.is_none() && lane.in_flight != Some(Op::Delete) =>
            {
                self.queue(id, Op::Delete)
            }
            _ => Vec::new(),
        }
    }

    /// The operation in flight for `id` ended; starts the next one.
    ///
    /// After a failed delete the profile counts as restored: the caller puts it back in the
    /// store, and edits a removal cancelled the save of are saved again.
    pub(crate) fn finished(&mut self, id: &ProfileId, outcome: Outcome) -> Vec<Command> {
        let Some(lane) = self.lanes.get_mut(id) else {
            return Vec::new();
        };
        let Some(op) = lane.in_flight.take() else {
            return Vec::new();
        };
        let mut commands = Vec::new();
        match (op, outcome) {
            (Op::Delete, Outcome::Done) => {
                self.lanes.remove(id);
                return commands;
            }
            (Op::Delete, Outcome::Failed) => {
                lane.removing = false;
                commands = Self::rearm(id, lane);
            }
            (Op::Save, _) => {}
        }
        if let Some(next) = lane.next.take() {
            lane.in_flight = Some(next);
            commands.push(Command::Start(id.clone(), next));
        }
        self.tidy(id);
        commands
    }

    /// A create started: the queue is busy until [`Self::create_committed`].
    pub(crate) fn create_started(&mut self) -> CreateToken {
        let token = self.next_create;
        self.next_create += 1;
        self.creates.insert(token);
        CreateToken(token)
    }

    /// The create of `token` is over: its file is written and its profile in the store, or it
    /// failed and that was handled.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "taking it by value ends the create, so it cannot be committed twice"
    )]
    pub(crate) fn create_committed(&mut self, token: CreateToken) {
        self.creates.remove(&token.0);
    }

    /// Whether nothing is pending: no debounce, no operation, no removal waiting for its delete,
    /// no create. The application stays alive until it is.
    pub(crate) fn is_idle(&self) -> bool {
        self.creates.is_empty() && self.lanes.values().all(Lane::is_idle)
    }

    /// Whether `id` was removed and waits for its delete or an undo.
    pub(crate) fn is_removing(&self, id: &ProfileId) -> bool {
        self.lanes.get(id).is_some_and(|lane| lane.removing)
    }

    /// Starts `op` for `id`, or makes it the next one while another is in flight; a later save
    /// replaces a save that waits.
    fn queue(&mut self, id: &ProfileId, op: Op) -> Vec<Command> {
        let lane = self.lanes.entry(id.clone()).or_default();
        if lane.in_flight.is_some() {
            lane.next = Some(op);
            return Vec::new();
        }
        lane.in_flight = Some(op);
        vec![Command::Start(id.clone(), op)]
    }

    /// Arms the save of the edits a removal cancelled.
    fn rearm(id: &ProfileId, lane: &mut Lane) -> Vec<Command> {
        if !lane.unsaved {
            return Vec::new();
        }
        lane.unsaved = false;
        lane.armed = true;
        vec![Command::ArmDebounce(id.clone())]
    }

    /// Forgets `id`'s lane once nothing is pending for it.
    fn tidy(&mut self, id: &ProfileId) {
        if self
            .lanes
            .get(id)
            .is_some_and(|lane| lane.is_idle() && !lane.unsaved)
        {
            self.lanes.remove(id);
        }
    }
}

#[cfg(test)]
mod tests;
