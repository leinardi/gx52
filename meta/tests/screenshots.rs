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

//! The metainfo screenshots (docs/release.md, "Screenshots"): every image the metainfo names is a
//! PNG in `data/screenshots/`, committed with it, and that directory holds no other PNG.

use std::collections::BTreeSet;
use std::fs;
use std::sync::LazyLock;

use meta::repo;
use regex::Regex;

const METAINFO: &str = "data/com.leinardi.gx52.metainfo.xml.in.in";
const SCREENSHOTS: &str = "data/screenshots";

/// The PNG signature.
const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// The repository path of an `<image>` URL under `data/screenshots/`.
static IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"<image>[^<]*/(data/screenshots/[a-z0-9-]+\.png)</image>")
        .unwrap_or_else(|error| panic!("invalid pattern: {error}"))
});

/// The repository paths of the metainfo's screenshots, in order.
fn metainfo_screenshots(metainfo: &str) -> Vec<String> {
    IMAGE
        .captures_iter(metainfo)
        .map(|captures| captures[1].to_owned())
        .collect()
}

#[test]
fn every_metainfo_screenshot_is_a_committed_png() {
    let named = metainfo_screenshots(&repo::read(METAINFO));
    assert!(!named.is_empty(), "{METAINFO}: no screenshot read");
    let files: BTreeSet<String> = repo::tracked(&[SCREENSHOTS])
        .into_iter()
        .filter(|path| {
            std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension == "png")
        })
        .collect();
    let mut problems = Vec::new();
    for path in &named {
        if !files.contains(path) {
            problems.push(format!("{METAINFO} names {path}, which git does not have"));
            continue;
        }
        let bytes = fs::read(repo::path(path))
            .unwrap_or_else(|error| panic!("cannot read {path}: {error}"));
        if !bytes.starts_with(PNG_MAGIC) {
            problems.push(format!("{path} is not a PNG"));
        }
    }
    let named: BTreeSet<&String> = named.iter().collect();
    for path in &files {
        if !named.contains(path) {
            problems.push(format!("{path} is in no <screenshot> of {METAINFO}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_reading_finds_the_image_paths() {
    let metainfo = "<image>https://example.org/raw/data/screenshots/leds.png</image>\n\
                    <image>https://example.org/elsewhere/other.png</image>";
    assert_eq!(
        metainfo_screenshots(metainfo),
        ["data/screenshots/leds.png"]
    );
}
