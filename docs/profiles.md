# GX52 profiles

The profile data format: the TOML schema, where the files live, the default profiles, the per-type
mode mapping, validation and recovery, import and export, the legacy import from GX52 0.7 and the
migration policy. Sources: [spike S8](spikes/08-legacy-import.md) (the legacy database and the
importer decisions), [rewrite-analysis.md](rewrite-analysis.md) sections 1, 2.4 and 10, and the
device facts in [protocol.md](protocol.md). Where the profile store sits in the app is in
[architecture.md](architecture.md#persistence).

## Model

- A **profile** is one full device state for **one device type**: `x52-pro` or `x52` (the device
  types of [protocol.md](protocol.md#device-ids-and-access)). A profile never applies to the other
  type.
- A **mode mapping** per device type says which profile each position of the mode selector (1, 2,
  3) uses. By default all three modes use that type's default profile, so a user who never touches
  the mapping gets one profile, as in GX52 0.7.
- Each device type has exactly one **default profile**, with a stable id, that cannot be removed.

## TOML schema (version 1)

One file holds one profile. A complete X52 Pro profile:

```toml
schema_version = 1
device_type = "x52-pro"
name = "Night flying"

led_brightness = 64          # 0–128, the wire value of protocol.md
mfd_brightness = 32          # 0–128
shift = false                # SHIFT label on the MFD (x52-pro only)
blink = false                # i and POV 1 LEDs blink (x52-pro only)
date_format = "dd-mm-yy"     # "dd-mm-yy", "mm-dd-yy" or "yy-mm-dd"

[leds]                       # x52-pro only
fire = "on"                  # single-colour: "off" or "on"
throttle = "on"
a = "green"                  # bi-colour: "off", "green", "red" or "amber"
b = "green"
d = "red"
e = "red"
t1_t2 = "amber"
t3_t4 = "amber"
t5_t6 = "amber"
pov_2 = "green"
i = "off"

[mfd]
lines = ["GX52", "", "Night flying"]   # exactly 3 entries

[clock_1]
source = "local"             # "local" or "utc"
use_24h = true

[clock_2]
source = { iana = "Europe/Rome" }
use_24h = true

[clock_3]
source = { fixed_offset_minutes = 345 }
use_24h = false
```

An X52 profile has the same keys **except** `[leds]`, `shift` and `blink`: the non-Pro has no
individually controllable LEDs, and shift and blink are not sent to it until verified
([protocol.md, "X52 (non-Pro)"](protocol.md#x52-non-pro)).

| Key | Type | Values | Notes |
| --- | --- | --- | --- |
| `schema_version` | integer | `1` | Read first; see [Validation and recovery](#validation-and-recovery) and [Migration policy](#migration-policy) |
| `device_type` | string | `"x52-pro"`, `"x52"` | Decides which keys exist |
| `name` | string | 1–64 characters after trimming, unique per device type | Shown in the UI |
| `led_brightness`, `mfd_brightness` | integer | `0`–`128` | The wire scale of `0xb2` / `0xb1`. GX52 0.7 stored `0`–`32` and multiplied by 4; imported values are multiplied by 4 |
| `shift`, `blink` | boolean |  | `x52-pro` only |
| `date_format` | string | `"dd-mm-yy"`, `"mm-dd-yy"`, `"yy-mm-dd"` | Field order of [protocol.md](protocol.md#usb-vendor-commands) |
| `leds.<name>` | string | single-colour `fire`, `throttle`: `"off"`, `"on"`; bi-colour `a`, `b`, `d`, `e`, `t1_t2`, `t3_t4`, `t5_t6`, `pov_2`, `i`: `"off"`, `"green"`, `"red"`, `"amber"` | `x52-pro` only. The set of names comes from the `x52` device model (the [LED bits](protocol.md#led-bits)). POV 1 is not stored: its LED is fixed green |
| `mfd.lines` | array of 3 strings | each at most 16 bytes **after** conversion through the [MFD character set](protocol.md#mfd-character-set) | Characters without a glyph are stored as typed and converted (with the protocol.md fallback) when sent; the length is checked on the converted bytes |
| `clock_1.source` | string | `"local"`, `"utc"` |  |
| `clock_2.source`, `clock_3.source` | table | `{ iana = "<zone>" }` or `{ fixed_offset_minutes = <n> }` | See [Clocks](#clocks) |
| `clock_N.use_24h` | boolean |  | Per clock (S2: the 12/24 h flag is independent per clock) |

Every key is required in a file the app writes. A missing key in a file being read is handled like
an out-of-range value (see below).

In Rust (`gx52` crate, with `serde` and `toml`): `DeviceType`, `LedState`, `DateFormat` and
`ClockSource` are enums, never strings, past the parser. `ClockSource` for clocks 2/3 is an
externally tagged enum (`Iana(String)` / `FixedOffsetMinutes(i16)`, `rename_all = "snake_case"`),
which is exactly the `{ iana = ... }` / `{ fixed_offset_minutes = ... }` shape. Reading is two-step,
so one bad field does not lose the whole profile: parse the document into a `toml::Table`, read
`schema_version` and `device_type`, then convert field by field into the typed profile, collecting
a list of field problems instead of failing on the first.

### MFD lines

Schema 1 stores three lines of static text. GX52 0.7 filled the lines itself (mode name while the
selector moves, the pressed button, the profile name); whether the rewrite keeps that, and how a
line refers to dynamic content, is decided with the UI in milestone M4 and, if it needs new keys,
ships as a schema bump under the [migration policy](#migration-policy).

## Clocks

- **Clock 1** shows `local` time (the system zone, following its daylight saving) or `utc`.
- **Clocks 2 and 3** have a tagged source:
  - `{ iana = "Europe/Rome" }`: a geographic zone from the runtime's tz database, read through
    `jiff` (desk research: GNOME 51 ships `/usr/share/zoneinfo`), with its daylight saving
    transitions;
  - `{ fixed_offset_minutes = 345 }`: a fixed offset from UTC in minutes, `−720`–`+840` (UTC−12:00
    to UTC+14:00, the range GX52 0.7 offered). A fixed offset has **no daylight saving**.
- What is sent: the device only knows clock 1's time and offsets from clock 1, so the app sends
  `clock N offset − clock 1 offset`, normalised as [protocol.md](protocol.md#usb-vendor-commands)
  describes. That is a property of the wire, not of the profile.

**Legacy offsets** (GX52 0.7 stored minutes from UTC, S8) are imported **unchanged** as
`fixed_offset_minutes`. They are never guessed into a geographic zone: `-240` could be New York in
summer or Santiago in winter, and the app cannot tell. The user can switch a clock to a zone after
the import.

Worked examples, clock 1 on `local` with the system zone at UTC+1 (Central European winter time):

| GX52 0.7 value | Imported as | Meaning | Offset sent from clock 1 |
| --- | --- | --- | --- |
| `-240` | `{ fixed_offset_minutes = -240 }` | UTC−04:00, all year | `-240 − 60 = -300` |
| `480` | `{ fixed_offset_minutes = 480 }` | UTC+08:00, all year | `480 − 60 = +420` |
| `345` | `{ fixed_offset_minutes = 345 }` | UTC+05:45 (Nepal's offset; `Etc/GMT±N` cannot express it, S8) | `345 − 60 = +285` |

In summer (UTC+2) the same profile sends `-360`, `+360` and `+225`: the profile is unchanged, the
offsets from clock 1 move because clock 1 moved.

**Round-trip tests:** for each sample row of S8 and each worked example above, legacy row → profile
→ TOML → profile gives the same profile, and the TOML matches a stored expected file.

## Files

- **Directory:** `glib::user_config_dir().join("gx52/profiles")`, i.e.
  `$XDG_CONFIG_HOME/gx52/profiles/` as decided in rewrite analysis section 1.
- **One file per profile**, named `<id>.toml`. The id is stable for the profile's whole life and is
  not its name, so renaming never renames the file.
  - Default profiles: `x52-pro-default` and `x52-default`.
  - Other profiles: a random UUID (`glib::uuid_string_random()`), lowercase.
  - A file whose name ends in `.toml` but whose stem is not a valid id (only lowercase letters,
    digits and `-`) is ignored and reported, never touched. Files whose names do not end in `.toml`
    (`*.rejected-*`, `*.bak`, editor leftovers such as `x.toml~`) are ignored silently.
- **Atomic writes:** the new content is written to a temporary file in the same directory and
  renamed over the old one (`gio::File::replace_contents_future`, which does that), off the main
  thread. A crash leaves either the old file or the new one, never half of each.
- **Size limits:** a profile file (and an import file) larger than 64 KiB is not parsed; it is
  handled like a malformed file. A valid profile is well under 2 KiB. A legacy database larger than
  16 MiB is not opened and is handled like an unreadable one (the two S8 databases are 32 KiB each).
- **Production and Devel** are isolated by the Flatpak sandbox: each app ID has its own config
  directory, `~/.var/app/com.leinardi.gx52/config/gx52/profiles/` and
  `~/.var/app/com.leinardi.gx52.Devel/config/gx52-devel/profiles/` (directory name below).
- **Host Devel builds** (`meson devenv` or a source install of the Devel profile) are not
  sandboxed and would share `~/.config/gx52/` with a host production build. Decision: the Devel
  profile uses the directory name `gx52-devel` instead of `gx52`, taken from the build profile in
  `src/config.rs` (its constants are set by Meson through the environment), so a Devel build
  writes to `~/.config/gx52-devel/profiles/` on the host and to the path above in its Flatpak.
- **Legacy database lookup**, separate from the new store:
  `glib::user_config_dir().join("gx52/gx52.db")`, the path GX52 0.7 used, for every build profile.
  It is only ever opened **read-only** (`rusqlite` with the read-only open flag), never modified or
  deleted, so users can go back to `0.7.6` (rewrite analysis, section 10). Inside the production
  Flatpak that is `~/.var/app/com.leinardi.gx52/config/gx52/gx52.db`, the file the automatic import
  finds; a source-install database in `~/.config/gx52/gx52.db` is outside the sandbox and needs the
  documented manual copy (R10). A host Devel build reads the host's `~/.config/gx52/gx52.db`, which
  is safe because it is read-only.

## Defaults

- **One default profile per device type**, ids `x52-pro-default` and `x52-default`, named "Default".
  It cannot be removed or change device type; it can be renamed and edited.
- **Built-in values** (GX52 0.7's defaults, `gx52/model/*_profile.py` and `gx52/conf.py`): both
  brightness values `128`; on the X52 Pro every bi-colour LED green and Fire and Throttle on; clock
  1 `local`, 24 h; clock 2 `{ fixed_offset_minutes = -240 }`, clock 3 `{ fixed_offset_minutes = 480
  }`, both 24 h; date `yy-mm-dd`; shift and blink off; MFD lines empty. GX52 0.7's X52 table
  defaulted clock 1 to UTC while the Pro table used local time; the rewrite uses `local` for both.
- **Built-ins always exist in memory:** each default profile exists in memory with its stable id
  even when its file is missing, unreadable or of a future schema, so every mapping fallback has a
  target. See [Persisting the default](#persisting-the-default).

### Legacy import

At the first start that finds `gx52.db` (rewrite analysis, section 10; the S8 schema):

| S8 decision | Rule |
| --- | --- |
| Device type per table | `x52_pro_profile` rows become `x52-pro` profiles, `x52_profile` rows become `x52` profiles. Columns are read by name; extra columns are ignored |
| LED columns | Taken only from `x52_pro_profile`. `x52_profile` rows **never** take LED columns, even in the 2019 schema that has them |
| Per-type default | The row with `can_be_removed = 0` maps onto **that type's default profile** (its values replace the built-in values; the id stays `x52-…-default` and the name stays "Default", so `Default` / `Default Profile` from S8 is not carried over). If a table has more than one such row, the one with the lowest `id` is the default and the others are imported as normal profiles |
| Other rows | Become new profiles with new ids, names kept; a name that clashes with an existing profile of the same type gets `(2)`, `(3)`, ... appended |
| Brightness scale | `led_brightness`, `mfd_brightness` × 4 (`0`–`32` becomes `0`–`128`) |
| Clock 2/3 offsets | Unchanged as `{ fixed_offset_minutes = n }` (see [Clocks](#clocks)) |
| Other columns | `clock_1_use_local_time` `1` → `local`, `0` → `utc`; `clock_N_use_24h` as is; `date_format` `'0'` → `yy-mm-dd`, `'1'` → `dd-mm-yy`, `'2'` → `mm-dd-yy`; LED text digits `'0'`/`'1'` → `off`/`on` for Fire and Throttle, `'0'`–`'3'` → `off`/`green`/`red`/`amber` for the others. `shift`, `blink` (not in GX52 0.7) off; MFD lines empty |
| Settings | Only `settings_launch_on_login` (→ the `autostart` key) and `settings_show_app_indicator` (→ the `show-tray` key) are imported; all other keys are ignored |
| Bad values | A value outside its range (a brightness above `32`, an offset outside `−720`–`+840`, an unknown enum digit) gets the type default for that field and is listed in the import report; the row is still imported. A name is trimmed; an empty one becomes "Imported profile", one over 64 characters is cut to 64, and the result is made unique as above |
| Existing files | The import never overwrites a profile file. If a default profile's file already exists when the import runs, the legacy default row is imported as a normal profile with a unique name instead |

The result is reported once ("Imported N profiles from GX52 0.7", with any field problems).
`current_profile` is empty in both real databases (S8), so nothing is imported for the selected
profile: the mode mapping starts at its defaults. Test fixtures are two small databases reproducing
both S8 schemas, created by the test suite.

**Done flag.** The GSettings key `legacy-import-done` (`b`, default `false`) records that the import
is over. The import is due while the flag is unset and there is a `gx52.db` or a saved plan (below).
With neither, nothing happens and the flag stays unset, so a database copied in later (R10) is still
imported.

**Restart safety.** A start can stop between two file writes, and the next start must give the same
profiles as one clean run:

- **Plan first.** Before writing any profile, the import reads every row and decides its target
  (the type's default id when that default's file does not exist yet, otherwise a new random id), its
  final name and its file contents. It saves this plan atomically to `legacy-import.toml` in the
  app's config directory (`gx52/` or `gx52-devel/`, next to `profiles/`, outside the store's
  scan). Collisions are resolved once, there, and every retry reuses the saved plan instead of
  planning again.
- **Execute.** Each planned file is created with create-new semantics, never over an existing file.
  On a retry, a target that holds exactly the planned contents counts as done. A target that holds
  anything else was written by someone else since planning: it is kept and reported as not
  imported. Only ids the plan assigned are ever checked, so an unrelated file never stands in for an
  imported row. A plan file that cannot be read is logged and the import plans again; at worst
  that duplicates a profile, never loses one.
- **Two kinds of failure.** A **database** failure (too large, unreadable, not a GX52 database) is
  final: the flag is set, nothing is written, and the toast says so. A **write** failure (an I/O
  error in the config directory) leaves the flag unset, and the next start resumes from the plan.
- The settings are applied and the flag is set only after every planned entry is written or
  skipped. The plan file stays afterwards, as the record of the import.
- **No built-in default while pending.** While the import is due and not over, the store keeps a
  missing default profile in memory only and does not write its file (see
  [Persisting the default](#persisting-the-default)). Otherwise a failed write of the imported
  default would be followed by a built-in file, and the next start would import the legacy default
  as a normal profile.

## Mode mapping

GSettings keys, one per device type, each an array of three profile ids for modes 1, 2 and 3:

| Key | Type | Default |
| --- | --- | --- |
| `mode-profiles-x52-pro` | `as` | `['x52-pro-default', 'x52-pro-default', 'x52-pro-default']` |
| `mode-profiles-x52` | `as` | `['x52-default', 'x52-default', 'x52-default']` |

Resolution, per mode, when the active mode or the device changes:

- an id that names a loaded profile of the **same device type** is used;
- a missing id, an id of a profile that is not loaded (unreadable, future schema, removed), an id of
  a profile of the other type, or an array that does not have three entries falls back to **that
  type's default profile** for the affected entries. The setting itself is left as it is, so the
  mapping comes back when the profile does.

Removing a profile resets the entries that named it to the default id, after the user confirmed the
removal.

## Validation and recovery

The rule above all the others: **a profile file is never overwritten or deleted because it could not
be read.** The app rewrites a file only when the user saves that profile, and only after the cases
below allow it.

The table applies to every profile file. For the two **default ids** (`x52-pro-default`,
`x52-default`) the rows that leave a profile unloaded or read-only (malformed, over the size limit,
newer schema) are replaced by [Persisting the default](#persisting-the-default): the built-in
default takes the id and is listed and editable as usual, the bad file is not listed separately, and
it is replaced only through the confirm-and-back-up path there.

| Case | Outcome | File |
| --- | --- | --- |
| Malformed TOML, over the size limit, not UTF-8, or a missing or wrong `schema_version` / `device_type` | The profile is not loaded. The problem is logged and shown once: one toast per start that lists every file that failed. Other profiles load normally | Kept as is |
| Out-of-range value: brightness outside `0`–`128`, an MFD line over 16 bytes after conversion or not exactly 3 lines, `fixed_offset_minutes` outside `−720`–`+840`, an empty or too long name, a missing key | Rejected **per field**: that field gets the type default in memory, the rest of the profile loads. The problem is reported (log and the same toast) | Unchanged until the user saves the profile, which writes the corrected values |
| Unknown enum value: LED state, date format, clock 1 source, a clock 2/3 source tag, an IANA zone the tz database does not know | Same as out of range | Same |
| A key the schema does not have, or a key the device type does not have (`[leds]` in an `x52` file) | Ignored in memory and reported | Same: the key is dropped only when the user saves |
| Duplicate name within a type | Loaded with `(2)`, `(3)`, ... appended in memory and reported | Same |
| `schema_version` newer than supported | Listed **read-only** with a message ("made by a newer GX52"). It cannot be edited, applied to the device or selected in the mode mapping (the mapping falls back as above). It is never silently downgraded or re-saved in the old format | Never written |
| `schema_version` older than supported | Migrated in memory through the [migration functions](#migration-policy) and loaded | Rewritten only when the user saves, and only after the original is copied to `<id>.toml.schema-<old version>.bak` |

### Persisting the default

- If a default profile's file is **missing** (first start, or deleted by hand), it is written from
  the built-in values (or from the legacy default row, during the import), with the atomic write.
  The exception: while the legacy import is pending, the built-in values stay in memory and nothing
  is written (see [Legacy import](#legacy-import)).
- If it is **unreadable or of a future schema**, the file is preserved untouched. The built-in
  default is used in memory, and the problem is reported like any other failed file.
- **Edits to the default in that state** are saved only after the user explicitly confirms
  replacing the bad file in an `adw::AlertDialog`. Before writing, the bad file is copied to
  `<id>.toml.rejected-<timestamp>` (UTC, `YYYYMMDDTHHMMSSZ`), then the new file is written
  atomically. Cancelling keeps the edits in memory only.
- **Backups never overwrite backups:** if a `<id>.toml.rejected-<timestamp>` or
  `<id>.toml.schema-<N>.bak` name already exists, a counter is appended (`-2`, `-3`, ...) until the
  name is free. The copy is made with "create new file" semantics, so a race cannot overwrite
  either.
- **Nothing is overwritten automatically.** A default file with only per-field problems is handled
  like any other profile: fixed in memory, rewritten when the user saves.

## Import and export

- **Export** writes one profile, in the schema above, through `gtk::FileDialog` (the file chooser
  portal; no filesystem permission). The file has no id: the id belongs to the local store.
- **Import** reads one file through the same portal, with the same limits and validation as the
  store:
  - the import is started from one device type's profile list; a file whose `device_type` is the
    other type, or not a known type, is **refused with a message** that names the type the file is
    for;
  - a newer `schema_version` is refused with a message (it could only be read-only); an older one is
    migrated;
  - per-field problems are fixed to the type defaults and listed before the import is confirmed;
  - a name that clashes within the type gets `(2)`, `(3)`, ... appended;
  - the imported profile gets a new random id and is never a default profile.

## Migration policy

- `schema_version` is one integer, incremented by **every** change to the schema: a new key, a
  removed or renamed key, a changed meaning or range. Then an app always knows whether it
  understands a file completely.
- For each step there is one pure function `vN → vN+1` over the parsed table, with fixture files of
  version N and the expected result as tests. Loading an older file runs the steps in order, in
  memory.
- No migration ever goes down. An older app shows a newer file read-only (above), so profiles made
  by a newer GX52 survive a downgrade untouched.
- A migrated file is written only on the user's save, after the `<id>.toml.schema-<N>.bak` copy.
- A schema bump is a user-visible change: its commit is a `feat` or `fix` (AGENTS.md, "Commit
  messages"), and the release notes say that profiles saved by the new version cannot be edited by
  older ones.
