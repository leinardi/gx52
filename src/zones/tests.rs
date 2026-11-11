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

const ISO3166: &str = "\
# ISO 3166 alpha-2 country codes
#
AR\tArgentina
JP\tJapan
NP\tNepal
broken line without a tab
XYZ\tThree letters
";

const ZONE_TAB: &str = "\
# tz zone descriptions
#
JP\t+353916+1394441\tAsia/Tokyo
AR\t-3436-05827\tAmerica/Argentina/Buenos_Aires\tBuenos Aires (BA, CF)
NP\t+2743+08519\tAsia/Kathmandu
ZZ\t+0000+00000\tEtc/Unknown_Country
JP\t+353916+1394441\tAsia/Tokyo
short\tline
JP\t+0000+00000\t
JP\t+0000+00000\tBad//Name
";

fn names(entries: &[ZoneEntry]) -> Vec<&str> {
    entries.iter().map(|entry| entry.iana.as_str()).collect()
}

#[test]
fn zone_tab_lists_each_zone_once_by_city_with_utc() {
    let entries = parse_zone_tab(ZONE_TAB, &parse_iso3166(ISO3166));
    assert_eq!(
        names(&entries),
        [
            "America/Argentina/Buenos_Aires",
            "UTC",
            "Asia/Kathmandu",
            "Asia/Tokyo",
            "Etc/Unknown_Country",
        ]
    );
    let buenos_aires = &entries[0];
    assert_eq!(buenos_aires.city, "Buenos Aires", "the last part, spaced");
    assert_eq!(buenos_aires.country, "Argentina");
    assert_eq!(entries[1].city, "Coordinated Universal Time");
    assert_eq!(
        entries[4].country, "",
        "a code iso3166.tab lacks has no country"
    );
}

#[test]
fn iso3166_skips_comments_and_bad_lines() {
    let countries = parse_iso3166(ISO3166);
    assert_eq!(countries.len(), 3);
    assert_eq!(countries["JP"], "Japan");
}

#[test]
fn a_search_matches_city_country_or_name() {
    let entries = parse_zone_tab(ZONE_TAB, &parse_iso3166(ISO3166));
    let found = |query: &str| -> Vec<&str> {
        entries
            .iter()
            .filter(|entry| entry.matches(query))
            .map(|entry| entry.iana.as_str())
            .collect()
    };
    assert_eq!(found("tokyo"), ["Asia/Tokyo"]);
    assert_eq!(found("  JAPAN "), ["Asia/Tokyo"], "trimmed, any case");
    assert_eq!(found("asia/kath"), ["Asia/Kathmandu"]);
    assert_eq!(
        found("buenos_aires"),
        ["America/Argentina/Buenos_Aires"],
        "the IANA spelling"
    );
    assert_eq!(found("universal"), ["UTC"]);
    assert_eq!(found("").len(), entries.len());
    assert!(found("atlantis").is_empty());
}

#[test]
fn loading_reads_both_tables() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("zone.tab"), ZONE_TAB).unwrap();
    fs::write(dir.path().join("iso3166.tab"), ISO3166).unwrap();
    let entries = load_from(dir.path(), || panic!("the fallback is not needed"));
    assert_eq!(entries, parse_zone_tab(ZONE_TAB, &parse_iso3166(ISO3166)));
}

#[test]
fn without_iso3166_the_zones_have_no_country() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("zone.tab"), ZONE_TAB).unwrap();
    let entries = load_from(dir.path(), || panic!("the fallback is not needed"));
    assert_eq!(entries.len(), 5);
    assert!(entries.iter().all(|entry| entry.country.is_empty()));
}

#[test]
fn without_zone_tab_the_names_are_listed() {
    let dir = tempfile::tempdir().unwrap();
    let entries = load_from(dir.path(), || {
        vec![
            "Europe/Rome".to_owned(),
            "Asia/Tokyo".to_owned(),
            "Europe/Rome".to_owned(),
            "bad name".to_owned(),
        ]
    });
    assert_eq!(names(&entries), ["Europe/Rome", "Asia/Tokyo"]);
    assert_eq!(entries[0].city, "Rome");
}

#[test]
fn an_oversized_table_is_not_read() {
    let dir = tempfile::tempdir().unwrap();
    let big = "JP\t+0+0\tAsia/Tokyo\n".repeat(60_000);
    fs::write(dir.path().join("zone.tab"), big).unwrap();
    let entries = load_from(dir.path(), || vec!["Europe/Rome".to_owned()]);
    assert_eq!(names(&entries), ["Europe/Rome"], "the fallback is used");
}

#[test]
fn the_system_database_lists_zones_with_countries() {
    // The host and the GNOME runtime both ship zone.tab and iso3166.tab.
    let entries = load();
    let tokyo = entries
        .iter()
        .find(|entry| entry.iana == "Asia/Tokyo")
        .expect("Asia/Tokyo is listed");
    assert_eq!(tokyo.country, "Japan");
    assert!(entries.iter().any(|entry| entry.iana == "UTC"));
}

#[test]
fn a_zone_time_follows_the_zone() {
    let now: Timestamp = "2026-07-15T12:34:00Z".parse().unwrap();
    let entry = |iana: &str| ZoneEntry::from_name(iana, "");
    assert_eq!(
        zone_time(&entry("Asia/Kathmandu"), now),
        Some(ZoneTime {
            time: "18:19".to_owned(),
            offset_minutes: 345,
        })
    );
    assert_eq!(
        zone_time(&entry("America/St_Johns"), now).map(|time| time.offset_minutes),
        Some(-150),
        "daylight saving time in July"
    );
    assert_eq!(zone_time(&entry("Nowhere/Atlantis"), now), None);
}
