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
use crate::profile::exchange::{self, prepare_import, revalidate_name};
use crate::profile::store::DefaultPersistence;

const PRO: DeviceType = DeviceType::X52Pro;

/// A store with one X52 Pro profile per name.
fn store(names: &[&str]) -> (TempDir, ProfileStore, Vec<ProfileId>) {
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
    (dir, store, ids)
}

/// The bytes of an exported X52 Pro profile named `name`.
fn file_named(name: &str) -> Vec<u8> {
    exchange::export(&Profile::builtin(PRO, name.to_owned())).into_bytes()
}

#[test]
fn overlapping_creates_get_different_names() {
    let (_dir, store, _) = store(&[]);
    let mut reservations = Reservations::new();
    let first = reservations.claim_unique(&store, PRO, "New profile");
    let second = reservations.claim_unique(&store, PRO, "New profile");
    assert_eq!(first.name(), "New profile");
    assert_eq!(second.name(), "New profile (2)");
    reservations.release(first);
    let third = reservations.claim_unique(&store, PRO, "New profile");
    assert_eq!(third.name(), "New profile", "a released name is free again");
}

#[test]
fn names_are_reserved_per_device_type() {
    let (_dir, store, _) = store(&[]);
    let mut reservations = Reservations::new();
    let _pro = reservations.claim_unique(&store, PRO, "New profile");
    let other = reservations.claim_unique(&store, DeviceType::X52, "New profile");
    assert_eq!(other.name(), "New profile");
}

#[test]
fn an_import_does_not_take_the_name_of_a_pending_create() {
    let (_dir, store, _) = store(&[]);
    let mut reservations = Reservations::new();
    let _new = reservations.claim_unique(&store, PRO, "New profile");
    let preview = prepare_import(
        &file_named("New profile"),
        PRO,
        &reservations.names(&store, PRO),
    )
    .unwrap();
    assert_eq!(preview.profile.name, "New profile (2)");
    assert!(preview.needs_confirmation());
    assert_eq!(reservations.claim(&store, PRO, "New profile"), None);
}

#[test]
fn a_rename_cannot_take_a_reserved_name() {
    let (_dir, store, ids) = store(&["DCS World"]);
    let mut reservations = Reservations::new();
    let _import = reservations.claim(&store, PRO, "Falcon BMS").unwrap();
    assert!(reservations.taken(&store, PRO, "Falcon BMS", Some(&ids[0])));
    assert!(
        !reservations.taken(&store, PRO, "DCS World", Some(&ids[0])),
        "its own name"
    );
    assert!(reservations.taken(&store, PRO, "DCS World", None));
}

#[test]
fn an_import_sees_a_rename_made_while_its_file_was_read() {
    let (_dir, mut store, ids) = store(&["DCS World"]);
    let reservations = Reservations::new();
    let mut preview = prepare_import(
        &file_named("Falcon BMS"),
        PRO,
        &reservations.names(&store, PRO),
    )
    .unwrap();
    assert!(!preview.needs_confirmation(), "clean when it was read");
    let mut renamed = store.loaded(&ids[0]).unwrap().profile.clone();
    renamed.name = "Falcon BMS".to_owned();
    store.update(&ids[0], renamed).unwrap();
    assert!(revalidate_name(
        &mut preview,
        &reservations.names(&store, PRO)
    ));
    assert_eq!(preview.profile.name, "Falcon BMS (2)");
    assert!(preview.needs_confirmation(), "a changed name asks first");
}

#[test]
fn a_removed_profile_keeps_its_name_until_deleted_or_restored() {
    let (_dir, mut store, ids) = store(&["DCS"]);
    let mut reservations = Reservations::new();
    let entry = store.take(&ids[0]).unwrap();
    let removed = reservations.claim(&store, PRO, "DCS").unwrap();
    assert!(
        reservations.taken(&store, PRO, "DCS", None),
        "no rename to it"
    );
    assert_eq!(
        reservations.claim_unique(&store, PRO, "DCS").name(),
        "DCS (2)",
        "no new profile with it"
    );

    // Undo, or a failed delete: the same entry and name come back, once.
    store.restore(ids[0].clone(), entry);
    reservations.release(removed);
    let named_dcs = store
        .names(PRO)
        .iter()
        .filter(|name| *name == "DCS")
        .count();
    assert_eq!(named_dcs, 1);

    // A delete that succeeds frees the name.
    let _entry = store.take(&ids[0]).unwrap();
    let removed = reservations.claim(&store, PRO, "DCS").unwrap();
    reservations.release(removed);
    assert!(reservations.claim(&store, PRO, "DCS").is_some());
}
