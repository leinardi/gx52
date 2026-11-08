# Profile fixtures

Profile files for the tests of `src/profile/` (`schema/tests.rs` and `store/tests.rs`), one per
case of [profiles.md, "Validation and recovery"](../../../docs/profiles.md#validation-and-recovery).
The tests copy them into a temporary directory under a profile id and check that loading never
changes them.

| File | Case |
| --- | --- |
| `valid-x52-pro.toml` | The complete X52 Pro example of profiles.md, in the layout the app writes |
| `valid-x52.toml` | A complete X52 profile, in the layout the app writes |
| `malformed.txt` | Not TOML (not named `.toml`, so the `check-toml` hook skips it) |
| `not-utf8.bin` | Not UTF-8 |
| `missing-schema-version.toml`, `wrong-schema-version.toml`, `zero-schema-version.toml` | No usable `schema_version` |
| `missing-device-type.toml`, `unknown-device-type.toml` | No usable `device_type` |
| `out-of-range.toml` | Brightness above `128`, an MFD line over 16 bytes, an offset above `840`, an empty name, a missing key |
| `two-mfd-lines.toml` | `mfd.lines` without exactly 3 entries |
| `unknown-values.toml` | An unknown LED state, date format, clock 1 source, IANA zone and clock source tag |
| `unknown-keys-x52.toml` | Keys the schema does not have, and `shift`, `blink` and `[leds]` in an `x52` file |
| `newer-schema.toml` | `schema_version = 99` |

Cases built by the tests themselves: a file over the 64 KiB limit, duplicate names, an older schema
(through a test-only schema 2 with one migration step), and default profile files that are bad,
newer, or of the other device type.
