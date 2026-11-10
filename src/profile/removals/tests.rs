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

use tempfile::TempDir;

use super::*;
use crate::profile::Profile;
use crate::profile::store::DefaultPersistence;
use crate::profile::writes::{Op, Outcome};

const PRO: DeviceType = DeviceType::X52Pro;

/// Everything a removal touches, with one X52 Pro profile per name.
struct Fixture {
    _dir: TempDir,
    store: ProfileStore,
    reservations: Reservations,
    writes: WriteQueue,
    removals: Removals,
    ids: Vec<ProfileId>,
}

impl Fixture {
    fn new(names: &[&str]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (mut store, _) = ProfileStore::load(dir.path(), DefaultPersistence::MemoryOnly);
        let ids = names
            .iter()
            .map(|name| {
                let profile = Profile::builtin(PRO, (*name).to_owned());
                let id = store.add_job(&profile).run().unwrap();
                store.added(id.clone(), profile);
                id
            })
            .collect();
        Self {
            _dir: dir,
            store,
            reservations: Reservations::default(),
            writes: WriteQueue::default(),
            removals: Removals::default(),
            ids,
        }
    }

    fn remove(&mut self, index: usize, active: Option<usize>) -> Removed {
        let id = self.ids[index].clone();
        let active = active.map(|active| self.ids[active].clone());
        let positions = self.ids.clone();
        self.removals
            .remove(
                &mut self.store,
                &mut self.reservations,
                &mut self.writes,
                &id,
                &positions,
                active.as_ref(),
            )
            .unwrap()
    }

    fn undo(&mut self, index: usize) -> Option<PutBack> {
        let id = self.ids[index].clone();
        self.removals
            .undo(
                &mut self.store,
                &mut self.reservations,
                &mut self.writes,
                &id,
            )
            .map(|(put_back, _)| put_back)
    }

    fn delete_due(&mut self, index: usize, serial: u64) -> Vec<Command> {
        let id = &self.ids[index];
        self.removals.delete_due(&mut self.writes, id, serial)
    }

    fn delete_finished(&mut self, index: usize, succeeded: bool) -> DeleteEnd {
        let id = self.ids[index].clone();
        self.writes.finished(
            &id,
            if succeeded {
                Outcome::Done
            } else {
                Outcome::Failed
            },
        );
        self.removals
            .delete_finished(&mut self.store, &mut self.reservations, &id, succeeded)
    }

    fn taken(&self, name: &str) -> bool {
        self.reservations.taken(&self.store, PRO, name, None)
    }

    fn listed(&self, index: usize) -> bool {
        self.store.entry(&self.ids[index]).is_some()
    }
}

#[test]
fn a_removal_keeps_the_name_and_the_place() {
    let mut fixture = Fixture::new(&["A", "DCS", "C"]);
    let removed = fixture.remove(1, Some(1));
    assert_eq!(removed.title, "DCS");
    assert_eq!(removed.device_type, Some(PRO));
    assert!(removed.was_active);
    assert!(!fixture.listed(1));
    assert!(
        fixture.taken("DCS"),
        "the name stays taken until the file is gone"
    );
    assert!(fixture.writes.is_removing(&fixture.ids[1]));

    let put_back = fixture.undo(1).unwrap();
    assert!(fixture.listed(1));
    assert_eq!(put_back.previous.as_ref(), Some(&fixture.ids[0]));
    assert_eq!(put_back.index, 1);
    assert!(put_back.reactivate);
    assert!(
        fixture
            .reservations
            .names(&fixture.store, PRO)
            .iter()
            .filter(|name| *name == "DCS")
            .count()
            == 1,
        "one DCS after the undo, no reservation left"
    );
}

#[test]
fn the_first_profile_goes_back_first() {
    let mut fixture = Fixture::new(&["A", "B"]);
    fixture.remove(0, None);
    let put_back = fixture.undo(0).unwrap();
    assert_eq!(put_back.previous, None);
    assert_eq!(put_back.index, 0);
    assert!(!put_back.reactivate, "it was not active");
}

#[test]
fn picking_another_profile_keeps_undo_from_overriding_it() {
    let mut fixture = Fixture::new(&["A", "B"]);
    fixture.remove(0, Some(0));
    fixture.removals.forget_active(DeviceType::X52);
    assert!(fixture.undo(0).unwrap().reactivate, "another type's choice");

    fixture.remove(0, Some(0));
    fixture.removals.forget_active(PRO);
    assert!(!fixture.undo(0).unwrap().reactivate);
}

#[test]
fn undo_is_too_late_once_the_delete_started() {
    let mut fixture = Fixture::new(&["A"]);
    let removed = fixture.remove(0, None);
    assert_eq!(
        fixture.delete_due(0, removed.serial),
        vec![Command::Start(fixture.ids[0].clone(), Op::Delete)]
    );
    assert_eq!(fixture.undo(0), None);
    assert!(!fixture.listed(0));
    assert!(fixture.taken("A"));
}

#[test]
fn a_finished_delete_frees_the_name() {
    let mut fixture = Fixture::new(&["A"]);
    let removed = fixture.remove(0, None);
    fixture.delete_due(0, removed.serial);
    assert_eq!(fixture.delete_finished(0, true), DeleteEnd::Gone(Some(PRO)));
    assert!(!fixture.listed(0));
    assert!(!fixture.taken("A"));
    assert!(fixture.writes.is_idle());
    assert_eq!(fixture.undo(0), None);
}

#[test]
fn a_failed_delete_puts_the_profile_back_once() {
    let mut fixture = Fixture::new(&["A", "DCS"]);
    let removed = fixture.remove(1, Some(1));
    fixture.delete_due(1, removed.serial);
    let DeleteEnd::PutBack(put_back) = fixture.delete_finished(1, false) else {
        panic!("a failed delete puts the profile back");
    };
    assert_eq!(put_back.previous.as_ref(), Some(&fixture.ids[0]));
    assert!(put_back.reactivate);
    assert!(fixture.listed(1));
    assert!(
        !fixture
            .reservations
            .taken(&fixture.store, PRO, "DCS", Some(&fixture.ids[1])),
        "the name is the profile's own again, not reserved"
    );
    assert!(!fixture.writes.is_removing(&fixture.ids[1]));
    assert_eq!(
        fixture.delete_finished(1, false),
        DeleteEnd::Unknown,
        "nothing left to put back"
    );
}

#[test]
fn a_late_toast_of_an_undone_removal_does_not_end_the_next_one() {
    let mut fixture = Fixture::new(&["A"]);
    let first = fixture.remove(0, None);
    fixture.undo(0).unwrap();
    let second = fixture.remove(0, None);
    assert_ne!(first.serial, second.serial);
    assert_eq!(fixture.delete_due(0, first.serial), vec![]);
    assert!(
        fixture.undo(0).is_some(),
        "the second removal can still be undone"
    );
}

#[test]
fn the_drain_deletes_every_pending_removal() {
    let mut fixture = Fixture::new(&["A", "B"]);
    fixture.remove(0, None);
    fixture.remove(1, None);
    let commands = fixture.removals.drain(&mut fixture.writes);
    assert_eq!(commands.len(), 2, "{commands:?}");
    for id in &fixture.ids {
        assert!(commands.contains(&Command::Start(id.clone(), Op::Delete)));
    }
    assert_eq!(
        fixture.removals.drain(&mut fixture.writes),
        vec![],
        "a second drain starts nothing twice"
    );
}

#[test]
fn the_default_profile_is_not_removed() {
    let mut fixture = Fixture::new(&[]);
    let default = ProfileId::default_for(PRO);
    let result = fixture.removals.remove(
        &mut fixture.store,
        &mut fixture.reservations,
        &mut fixture.writes,
        &default,
        &[],
        Some(&default),
    );
    assert!(matches!(result, Err(StoreError::DefaultNotRemovable(_))));
    assert!(fixture.writes.is_idle());
}
