# Legacy import fixtures

The expected profile files of the GX52 0.7 import (`src/profile/legacy/tests.rs`), one per row of
the two fixture databases the tests build (`flatpak_database` and `source_database`, the two
schemas of [spike S8](../../../docs/spikes/08-legacy-import.md)). Each file is named
`<database>-<table>-<row id>.toml` and holds the row's profile as the import converts it, before
the plan gives the default rows their default name.

`GX52_BLESS=1 cargo test -p gx52 legacy_rows_round_trip_through_toml` rewrites them from the
current output; check every value against the row and profiles.md, "Legacy import", before
committing. The maintainer's real databases are never committed.
