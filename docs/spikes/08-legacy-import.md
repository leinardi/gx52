# S8: legacy profile database

Two real GX52 0.7 databases on the maintainer's machine, opened read-only with SQLite:

| File | Last modified | Origin |
|---|---|---|
| `~/.var/app/com.leinardi.gx52/config/gx52/gx52.db` | 2021-03-16 | Flatpak install (the one the automatic import will find) |
| `~/.config/gx52/gx52.db` | 2021-03-07 | Source install (needs the documented manual copy) |

## Schema

Tables: `x52_pro_profile`, `x52_profile`, `current_profile`, `setting`, `sqlite_sequence`.

**The schema differs between databases.** The Flatpak database was created by a 2019 development
version, in which `x52_profile` (X52 non-Pro) also has all the LED columns. The source-install
database has the later schema, where `x52_profile` has no LED columns. The importer must read columns
by name, take the LED columns only from `x52_pro_profile`, and ignore extra columns.

Common profile columns (both tables):

| Column | Type | Values |
|---|---|---|
| `id` | integer | autoincrement |
| `name` | text | free text; the default profile is named `Default` (2019 schema) or `Default Profile` |
| `led_brightness`, `mfd_brightness` | integer | UI scale `0`-`32` (wire value is ×4) |
| `clock_1_use_local_time` | integer | `0` UTC, `1` local time |
| `clock_1_use_24h`, `clock_2_use_24h`, `clock_3_use_24h` | integer | `0` 12h, `1` 24h |
| `clock_2_offset`, `clock_3_offset` | integer | minutes from UTC (GX52 0.7 converts them to offsets from clock 1 when sending) |
| `date_format` | text | `'0'` YYMMDD, `'1'` DDMMYY, `'2'` MMDDYY |
| `can_be_removed` | integer | `0` for the default profile of each device type |
| `timestamp` | datetime | creation time |

LED columns (`x52_pro_profile`): `led_fire`, `led_throttle` store `'0'` off / `'1'` on;
`led_a`, `led_b`, `led_d`, `led_e`, `led_t1_t2`, `led_t3_t4`, `led_t5_t6`, `led_pov_2`, `led_i` store
`'0'` off / `'1'` green / `'2'` red / `'3'` amber. Enum values are stored as **text** digits.

`current_profile` is empty in both databases: there is no record of the last selected profile.

`setting` is a key/value table with integer values in a BLOB column. Keys seen:
`settings_launch_on_login`, `settings_load_last_profile`, `settings_show_app_indicator`,
`settings_check_new_version` and `settings_app_indicator_show_gpu_temp` (a leftover from another
project). Only `settings_launch_on_login` and `settings_show_app_indicator` have a meaning in the
rewrite; the importer ignores unknown keys.

## Sample content

- Flatpak DB: X52 Pro `Default` (mixed colours, LED brightness 7, MFD brightness 6, clock 2 `-240`,
  clock 3 `480`) and `Off` (both brightness values 0), X52 `Default`.
- Source DB: X52 Pro and X52 `Default Profile`, brightness 32, clock 2 `-240`, clock 3 `480`.

## Decisions for the importer

- Import `x52_pro_profile` rows as X52 Pro profiles and `x52_profile` rows as X52 profiles; never
  take LED columns from `x52_profile`.
- The row with `can_be_removed = 0` maps onto the new default profile of that device type instead of
  creating a duplicate; other rows become new profiles, names kept (made unique if needed).
- Brightness: multiply by 4 if the new model stores wire values, or keep the 0-32 scale; decided in
  `docs/profiles.md`.
- Clock 2/3: the stored values are offsets from UTC in minutes. If the new model uses time zones,
  map them to fixed-offset zones (`Etc/GMT±N` only covers whole hours, so non-hour offsets such as
  `345` need a fixed-offset representation).
- Settings: import `settings_launch_on_login` and `settings_show_app_indicator` only.
- Test fixtures: two small databases reproducing both schemas, created by the test suite (the real
  files are not committed).
