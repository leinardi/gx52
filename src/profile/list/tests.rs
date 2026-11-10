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

use std::fs;

use super::*;
use crate::profile::store::DefaultPersistence;
use crate::profile::tests::fixture;

fn id(text: &str) -> ProfileId {
    ProfileId::parse(text).unwrap()
}

fn row(text: &str, title: &str, kind: RowKind) -> Row {
    Row {
        id: id(text),
        title: title.to_owned(),
        kind,
    }
}

fn text(name: &str) -> String {
    String::from_utf8(fixture(name)).unwrap()
}

#[test]
fn each_type_lists_its_profiles_then_the_files_it_cannot_use() {
    let dir = tempfile::tempdir().unwrap();
    let valid = text("valid-x52-pro.toml");
    for (name, contents) in [
        ("bbb", valid.replace("Night flying", "Bravo")),
        ("aaa", valid.replace("Night flying", "Alpha")),
        ("per-mode", text("valid-x52-pro-per-mode.toml")),
        ("x52", text("valid-x52.toml")),
        ("newer", text("newer-schema.toml")),
        ("damaged-pro", text("missing-schema-version.toml")),
    ] {
        fs::write(dir.path().join(format!("{name}.toml")), contents).unwrap();
    }
    fs::write(dir.path().join("broken.toml"), fixture("malformed.txt")).unwrap();
    let (store, _) = ProfileStore::load(dir.path(), DefaultPersistence::MemoryOnly);
    let name = |text: &str| store.loaded(&id(text)).unwrap().profile.name.clone();
    let single = RowKind::Usable(ProfileKind::Single);

    assert_eq!(
        rows(&store, DeviceType::X52Pro, &["bbb"]),
        [
            row("x52-pro-default", "Default", single),
            row("bbb", "Bravo", single),
            row("aaa", "Alpha", single),
            row(
                "per-mode",
                &name("per-mode"),
                RowKind::Usable(ProfileKind::PerMode)
            ),
            row("broken", "broken.toml", RowKind::Damaged),
            row("damaged-pro", "damaged-pro.toml", RowKind::Damaged),
            row("newer", "Night flying", RowKind::Newer),
        ]
    );
    assert_eq!(
        rows(&store, DeviceType::X52, &["bbb"]),
        [
            row("x52-default", "Default", single),
            row("x52", &name("x52"), single),
            row("broken", "broken.toml", RowKind::Damaged),
        ],
        "a damaged file of unknown type is listed for every type"
    );
}
