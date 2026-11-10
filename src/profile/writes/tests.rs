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

fn id(text: &str) -> ProfileId {
    ProfileId::parse(text).unwrap()
}

fn arm(text: &str) -> Command {
    Command::ArmDebounce(id(text))
}

fn cancel(text: &str) -> Command {
    Command::CancelDebounce(id(text))
}

fn start(text: &str, op: Op) -> Command {
    Command::Start(id(text), op)
}

const NONE: [Command; 0] = [];

#[test]
fn an_edit_is_saved_after_the_debounce() {
    let mut queue = WriteQueue::default();
    assert!(queue.is_idle());
    assert_eq!(queue.edited(&id("aaa")), [arm("aaa")]);
    assert_eq!(queue.edited(&id("aaa")), [arm("aaa")], "each edit re-arms");
    assert!(!queue.is_idle());
    assert_eq!(queue.debounce_fired(&id("aaa")), [start("aaa", Op::Save)]);
    assert!(!queue.is_idle());
    assert_eq!(queue.finished(&id("aaa"), Outcome::Done), NONE);
    assert!(queue.is_idle());
}

#[test]
fn saves_of_one_profile_run_one_at_a_time_and_collapse() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    let _ = queue.debounce_fired(&id("aaa"));
    for _ in 0..2 {
        let _ = queue.edited(&id("aaa"));
        assert_eq!(
            queue.debounce_fired(&id("aaa")),
            NONE,
            "waits for the save in flight"
        );
    }
    assert_eq!(
        queue.finished(&id("aaa"), Outcome::Done),
        [start("aaa", Op::Save)],
        "the two waiting saves are one"
    );
    assert_eq!(queue.finished(&id("aaa"), Outcome::Done), NONE);
    assert!(queue.is_idle());
}

#[test]
fn a_failed_save_waits_for_the_next_edit() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    let _ = queue.debounce_fired(&id("aaa"));
    assert_eq!(queue.finished(&id("aaa"), Outcome::Failed), NONE);
    assert!(queue.is_idle());
    assert_eq!(queue.edited(&id("aaa")), [arm("aaa")]);
}

#[test]
fn a_delete_waits_for_the_save_in_flight() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    let _ = queue.debounce_fired(&id("aaa"));
    assert_eq!(
        queue.removed(&id("aaa")),
        NONE,
        "the save in flight goes on"
    );
    assert_eq!(
        queue.delete_due(&id("aaa")),
        NONE,
        "the delete waits for it"
    );
    assert_eq!(
        queue.finished(&id("aaa"), Outcome::Done),
        [start("aaa", Op::Delete)]
    );
    assert!(!queue.is_idle());
    assert_eq!(queue.finished(&id("aaa"), Outcome::Done), NONE);
    assert!(queue.is_idle());
    assert!(!queue.is_removing(&id("aaa")));
}

#[test]
fn a_removal_cancels_the_waiting_save_and_ignores_edits() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    assert_eq!(queue.removed(&id("aaa")), [cancel("aaa")]);
    assert_eq!(queue.edited(&id("aaa")), NONE);
    assert_eq!(
        queue.debounce_fired(&id("aaa")),
        NONE,
        "a late timer does nothing"
    );
    assert!(queue.is_removing(&id("aaa")));
    assert!(!queue.is_idle(), "a removal waits for its delete");
    assert_eq!(queue.delete_due(&id("aaa")), [start("aaa", Op::Delete)]);
    assert_eq!(queue.delete_due(&id("aaa")), NONE, "once");
    assert_eq!(queue.finished(&id("aaa"), Outcome::Done), NONE);
    assert!(queue.is_idle());
}

#[test]
fn a_removal_drops_a_save_queued_behind_one_in_flight() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    let _ = queue.debounce_fired(&id("aaa"));
    let _ = queue.edited(&id("aaa"));
    let _ = queue.debounce_fired(&id("aaa"));
    let _ = queue.removed(&id("aaa"));
    let _ = queue.delete_due(&id("aaa"));
    assert_eq!(
        queue.finished(&id("aaa"), Outcome::Done),
        [start("aaa", Op::Delete)],
        "the delete, not the dropped save"
    );
}

#[test]
fn an_undo_saves_the_edits_the_removal_cancelled() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    let _ = queue.removed(&id("aaa"));
    assert_eq!(queue.undone(&id("aaa")), [arm("aaa")]);
    assert!(!queue.is_removing(&id("aaa")));
    assert_eq!(queue.debounce_fired(&id("aaa")), [start("aaa", Op::Save)]);

    let mut queue = WriteQueue::default();
    let _ = queue.removed(&id("bbb"));
    assert_eq!(queue.undone(&id("bbb")), NONE, "nothing to save");
    assert!(queue.is_idle());
}

#[test]
fn an_undo_comes_too_late_once_the_delete_is_due() {
    let mut queue = WriteQueue::default();
    let _ = queue.removed(&id("aaa"));
    let _ = queue.delete_due(&id("aaa"));
    assert_eq!(queue.undone(&id("aaa")), NONE);
    assert!(queue.is_removing(&id("aaa")));
}

#[test]
fn a_failed_delete_restores_the_profile_and_its_unsaved_edits() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    let _ = queue.removed(&id("aaa"));
    let _ = queue.delete_due(&id("aaa"));
    assert_eq!(queue.finished(&id("aaa"), Outcome::Failed), [arm("aaa")]);
    assert!(!queue.is_removing(&id("aaa")));
    assert_eq!(queue.edited(&id("aaa")), [arm("aaa")], "editable again");
    let _ = queue.debounce_fired(&id("aaa"));
    let _ = queue.finished(&id("aaa"), Outcome::Done);
    assert!(queue.is_idle());
}

#[test]
fn flush_starts_every_armed_save_at_once() {
    let mut queue = WriteQueue::default();
    let _ = queue.edited(&id("aaa"));
    let _ = queue.edited(&id("bbb"));
    let _ = queue.debounce_fired(&id("bbb"));
    let _ = queue.edited(&id("bbb"));
    assert_eq!(
        queue.flush(),
        [cancel("aaa"), start("aaa", Op::Save), cancel("bbb")],
        "bbb's save follows the one in flight"
    );
    assert_eq!(queue.flush(), NONE, "a second drain finds nothing new");
    assert_eq!(
        queue.finished(&id("bbb"), Outcome::Done),
        [start("bbb", Op::Save)]
    );
}

#[test]
fn creates_keep_the_queue_busy_until_committed() {
    let mut queue = WriteQueue::default();
    let new = queue.create_started();
    let import = queue.create_started();
    assert_eq!(queue.flush(), NONE, "a drain cannot hurry a create");
    assert!(!queue.is_idle());
    queue.create_committed(new);
    assert!(!queue.is_idle());
    queue.create_committed(import);
    assert!(queue.is_idle());
}
