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

use toml::{Table, Value};

use super::*;

fn v1_to_v2(document: &mut Table) {
    document.insert("migrated".to_owned(), Value::Boolean(true));
}

const TWO: Schema = Schema::new(2, &[v1_to_v2]);

#[test]
fn the_current_version_is_read_as_is() {
    let mut document = Table::new();
    assert_eq!(
        Schema::CURRENT.migrate(&mut document, 1),
        Version::Readable {
            migrated_from: None
        }
    );
    assert!(document.is_empty());
}

#[test]
fn a_newer_version_is_not_touched() {
    let mut document = Table::new();
    assert_eq!(Schema::CURRENT.migrate(&mut document, 2), Version::Newer(2));
    assert!(document.is_empty());
}

#[test]
fn an_older_version_runs_its_steps_in_memory() {
    let mut document = Table::new();
    assert_eq!(
        TWO.migrate(&mut document, 1),
        Version::Readable {
            migrated_from: Some(1)
        }
    );
    assert_eq!(document.get("migrated"), Some(&Value::Boolean(true)));

    let mut current = Table::new();
    assert_eq!(
        TWO.migrate(&mut current, 2),
        Version::Readable {
            migrated_from: None
        }
    );
    assert!(current.is_empty());
}
