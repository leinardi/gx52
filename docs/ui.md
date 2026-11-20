# GX52 user interface

The UI of the rewrite: screens, navigation, the libadwaita widgets behind them, the adaptive layout
and the states the app can be in. Written in the M4 design session
([roadmap.md](roadmap.md#m4-ui-design-session)) on 2026-11-09, from an interview with the
maintainer, and reviewed on 2026-11-10 against the clickable prototype in
[`art/prototype/`](../art/prototype/) (open `Main.dc.html` through a local web server).

What the device does is in [protocol.md](protocol.md), the profile model in
[profiles.md](profiles.md), and how the pieces run in [architecture.md](architecture.md). Every
user-visible string is translatable and every interactive widget has an accessible label
(AGENTS.md).

## Principles

- **Set up once, tweak often.** People configure profiles and then let GX52 run in the background,
  but also open it between games to change LEDs or MFD text. The window favours clarity, with the
  common edits one click away.
- **What you select is what the device shows.** Selecting a profile makes it the active one; every
  edit goes to the device at once and is saved automatically. There is no preview mode and no Save
  button.
- **Never guess at the device.** The UI shows what the app sends and what evdev reports, never
  device state it cannot read (for example which clock the MFD shows).
- **Stock widgets.** libadwaita widgets with their default styles; the only custom-drawn pieces are
  the LED buttons, the simulated MFD and the input tester's indicators.
- **Adaptive down to 360×294**, the libadwaita minimum.
- **One device.** GX52 drives the first supported device it finds; several devices at once are not
  supported (rewrite analysis, section 12.2).

## Window

An `adw::ApplicationWindow` with an `adw::NavigationSplitView`: the sidebar on the left, the content
on the right.

```text
┌───────────────────────┬─────────────────────────────────────────────────┐
│ ☰  GX52             + │             [ LEDs | MFD | Clocks ]             │
├───────────────────────┼─────────────────────────────────────────────────┤
│ PROFILES              │ Editing mode   [ 1 | 2 | 3 ]                    │
│ ● Default             │   The mode selector on the stick picks the mode.│
│   DCS World      1 2 3│                                                 │
│   Elite Dangerous   ⋮ │  ┌ Stick ──────┐ ┌ Stick base ─┐ ┌ Throttle ─┐  │
│   Night flying        │  │ POV 2  Fire │ │ T1/T2 T3/T4 │ │  D    E   │  │
│                       │  │  A      B   │ │    T5/T6    │ │  i  Thr.  │  │
│ Test input          › │  │   POV 1     │ └─────────────┘ └───────────┘  │
│                       │  └─────────────┘                                │
│                       │                                                 │
│                       │  Brightness ───────────●──────── 96             │
│ ● X52 Pro             │  Blink                              [  ]        │
│   Connected · Mode 2  │                                                 │
└───────────────────────┴─────────────────────────────────────────────────┘
```

### Start-up

A new GX52 process opens on the **active profile's editor**: the selected row is the profile on
the device, so the first screen is one where the selection and the device agree, and the usual
reason to open the window, changing a LED colour or an MFD line before a game, takes no click.
The editor shows the view used last, LEDs, MFD, Clocks or Names (GSettings `last-view`), so
someone who only edits the MFD does not press Ctrl+2 each time. The input tester is never restored across
launches: it is a check made now, not a habit, and it selects no profile. A per-mode profile's
[mode bar](#mode-bar) starts on the selector's position.

- The [problem states](#problem-states) come first: with no device ever seen, the full-window
  status page; with a device GX52 has no permission for, the udev guidance; with `x52d` running,
  the editor and its banner.
- **Narrow:** the collapsed split view starts on the sidebar page, the profile list, with the
  editor one tap away.
- **Reopening a background instance** (a second launch, "Show GX52" in the tray) is not a start:
  the new window shows what the closed one showed, the input tester or a profile that failed to
  load included, and on a narrow window the same page of the split view. Only a new process
  follows the rules above. Start on Login starts hidden ([Preferences](#preferences)), so its
  first window also follows them.

### Sidebar

- **Header bar:** the main menu button (☰) on the left, the window title "GX52", and a "+" menu
  button on the right.
- **Profile list:** a `gtk::ListBox` with the `navigation-sidebar` style, the connected device
  type's profiles only ([profiles.md](profiles.md#model)).
  - The **default profile** is always first. It can be renamed and edited, never removed or moved.
  - The **other profiles** are in the user's order: drag a row to move it, by the handle
    (`list-drag-handle-symbolic`) that shows at its start while the pointer is over it; "Move Up"
    and "Move Down" in its menu do the same from the keyboard. The order is stored per device type
    ([profiles.md](profiles.md#profile-order)).
  - The **active profile** (the one on the device) is the selected row. Selecting a row makes that
    profile active at once. When another row is selected (a profile that failed to load), the
    active row shows a check mark next to its menu button.
  - **Renaming** in place: while the name is edited, the entry and the reason a name is refused
    take the whole row; the handle, the badges, the check mark and the menu button are hidden.
  - A **per-mode profile** shows small "1 2 3" badges, the mode shown highlighted while it is
    active: the mode picked in the [mode bar](#mode-bar), else the selector's.
  - A profile that **failed to load** (unreadable, newer schema) is listed greyed out with a
    warning icon; it can be selected to see why, but it is never sent to the device. A damaged
    file has no readable name, so its row shows the file name; a newer-schema profile shows its
    name when the file can be read.
- **"Test input"** row below the profiles, opening the [input tester](#input-tester) in the
  content area when it is clicked or activated with Enter; keyboard focus alone, which a list box
  turns into a selection, does not open it. Opening it does not change the active profile; the
  sidebar keeps a marker on the active profile while the tester is open.
- **Footer**, always visible: the [device status](#device-status).

### "+" menu

"New Single Profile" and "New Per-Mode Profile". The new profile gets the built-in values
([profiles.md](profiles.md#defaults)), appears at the end of the list as the active profile, and
its row opens for inline naming with "New profile" selected: Enter confirms, Escape keeps the
proposed name. An empty or duplicate name cannot be confirmed; the row shows why.

### Profile row menu

A "⋮" button on each row, also opened by right-click or long press:

| Item | Shortcut | Notes |
| --- | --- | --- |
| Rename | F2 | Inline, as for a new profile |
| Duplicate |  | "<name> (2)" right after the original ([profiles.md](profiles.md#profile-order)) |
| Export… | Ctrl+Shift+E | `gtk::FileDialog` save, as [profiles.md](profiles.md#import-and-export) describes |
| Use Per-Mode Settings / Use Single Settings |  | See [Profile kinds](#profile-kinds) |
| Move Up, Move Down |  | Not on the default profile |
| Remove | Delete | Not on the default profile; see below |

**Remove** asks first in an `adw::AlertDialog` ("Remove “DCS World”?", "Remove" in the destructive
style). After removing, a toast "“DCS World” removed" offers **Undo** for its timeout; the file is
deleted when the toast goes away. Removing the active profile makes the default profile active.

A profile that [failed to load](#a-profile-that-failed-to-load) has a shorter menu: **Show File**
and **Remove** (Delete).

### Main menu

Import Profile… (Ctrl+O), Preferences (Ctrl+,), Keyboard Shortcuts (Ctrl+?), About GX52, and in
a section of its own Quit (Ctrl+Q), which exits even with "Run in Background" on, as the tray's
Quit does.

**Import Profile…** opens the file chooser, then one of
([profiles.md](profiles.md#import-and-export)):

- a file without problems is added at once, with the toast "Imported “Falcon BMS”";
- a file with per-field problems opens an `adw::AlertDialog` "Import “Falcon BMS”?" that lists
  each value and what it is replaced with, with **Cancel** and **Import** (suggested);
- a file for the other model, or made by a newer GX52, is refused with a toast that says why
  ("This is a profile for the X52", "The profile was made by a newer version of GX52"); nothing
  is written.

## Profile kinds

A profile is **Single** or **Per-mode** ([profiles.md](profiles.md#model)):

- **Single:** one set of settings; the mode selector on the stick does nothing. The default profile
  starts as Single, like every GX52 0.7 profile.
- **Per-mode:** three sets of LED, MFD and brightness settings, one per position of the mode
  selector; the selector switches between them. Clocks and the date format are shared.

Changing the kind is in the row menu:

- **Single → Per-mode** copies the current settings into all three modes.
- **Per-mode → Single** keeps mode 1's settings after an `adw::AlertDialog` that says modes 2 and 3
  will be dropped.

## Profile editor

The content area for a profile: an `adw::ToolbarView` whose header bar has an `adw::ViewSwitcher`
as title widget, with four views: **LEDs** (Ctrl+1), **MFD** (Ctrl+2), **Clocks** (Ctrl+3) and
**Names** (Ctrl+4).
The profile name is not repeated there: the selected sidebar row names the profile. In the narrow
layout, where the sidebar is not visible, the header bar shows the profile name and the switcher
moves to the bottom ([Adaptive layout](#adaptive-layout)). Each view is an
`adw::PreferencesPage`-style scrolled page, content limited by an `adw::Clamp`.

Edits apply to the device at once and are saved automatically, a short moment after the last
change (500 ms), with the atomic write of profiles.md.

### Mode bar

On a per-mode profile, the LEDs, MFD and Names views start with a bar:

```text
Editing mode   [ 1 | 2 | 3 ]
The mode selector on the stick picks the mode.
```

- An `adw::ToggleGroup`. It **follows the stick**: when the selector moves, the bar switches to that
  mode.
- Picking a mode in the bar shows that mode's settings on the device, in the editor and in the
  profile row's badges until the selector on the stick moves again, another profile becomes active
  or the profile becomes single. Edits go to the mode shown. Picking the selector's mode drops the
  pick.
- Not shown on the Clocks view (shared settings) or on Single profiles.

### LEDs view

The schematic layout of [spike S7](spikes/07-led-editor.md#design-for-the-app):

- **One card per part of the device:** Stick, Stick base, Throttle, side by side when there is
  room and stacked when narrow. Inside a card the LED buttons sit in a grid that roughly follows the
  device, each with its name below.
- **LED button:** a round `gtk::MenuButton` coloured by state with libadwaita's palette (`--green-3`,
  `--red-2`, `--yellow-4`, as in the prototype); a click, Enter or Space opens a popover with an
  `adw::ToggleGroup` of the states (Off / Green / Amber / Red, or Off / On for Fire and Throttle).
  Choosing a state closes the popover; so does a change of the mode shown. Tooltip and accessible
  label: "<LED> LED: <state>".
- **POV 1** is a fixed green LED that opens nothing; its tooltip explains that it cannot be changed
  and follows brightness and blink.
- **Below the cards:** Brightness (`adw::ActionRow` with a `gtk::Scale`, 0–128, value shown) and
  Blink (`adw::SwitchRow`, subtitle "The i and POV 1 LEDs blink").
- **Brightness 0:** every LED is shown off, whatever its state; the states are kept and still
  editable, the accessible label adds ", brightness 0", and the group's description says
  "Brightness is 0: every LED is off. The states are kept."
- **Blink:** while Blink is on and the brightness above 0, the i LED (when not off) and POV 1 blink
  in the editor too, with a 1 s CSS animation; GTK stops it when animations are turned off.
- **X52 (non-Pro):** only the Brightness row, with the note "This model has no LEDs that can be
  set one by one."

### MFD view

```text
╭──────────────────────╮
│┌────────────────────┐│
││MODE                ││
││[SHIFT]   1   2   3 ││   Line 1   [ Custom text     ▾ ]  [ GX52           ]
│└────────────────────┘│   Line 2   [ Last button     ▾ ]
│┌────────────────────┐│   Line 3   [ Profile name    ▾ ]
││GX52                ││
││Fire                ││
││DCS World           ││
│└────────────────────┘│
│ ◷      12:45         │
│            09.11.26  │
╰──────────────────────╯
  Clock [ 1 | 2 | 3 ]       Brightness ────●──── 128     Shift [  ]
```

- **Simulated display:** a custom widget laid out as the device's MFD: the MODE area with the SHIFT
  label and the mode numbers 1, 2, 3, the three 16-character lines, the clock symbol at the start of
  the clock row with the time centred on the display's width, and the date under it. Segments that
  are off are drawn faint, as on the LCD: SHIFT when shift is off (always on the X52), and every
  mode number but the lit one. The lit mode is the **selector's position**, which the device shows
  by itself (the protocol has no command for it), so it can differ from a mode picked in the [mode
  bar](#mode-bar); while the selector's position is unknown or no device is connected, no mode is
  lit. It shows **live values**: the line sources resolved now (the active profile, the button being
  held: the display follows a held button and clears on release), the converted characters (a
  character the MFD cannot show appears as its fallback,
  [protocol.md](protocol.md#mfd-character-set)), and the time ticking at each minute. It is built
  from the state the device gets, and works with no device connected: edits to the last type's
  profiles change it ([Problem states](#problem-states)). The time is `HH:MM` in 24 h, and `h:MM`
  with a small `AM` or `PM` beside it in 12 h; the date uses dots, as the device draws it
  ("09.11.26", [protocol.md](protocol.md#usb-vendor-commands)). The clock area shows **clock 1**; a
  small "1 | 2 | 3" toggle under the display shows the other clocks, with the note "Clock 1 is shown
  by default. Start/Stop and Reset on the throttle cycle the clocks on the device." The app cannot
  read which clock the MFD shows, so it never guesses. Picking a clock shows its number in the date
  area for 3 seconds, as the device does when Start/Stop cycles the clocks, then the date again. The
  display is dark in both styles, as on the device, at most 368 pixels wide and narrower in a narrow
  window, and lit by the MFD brightness in four steps (0, up to 40, up to 90, above); at 0 the glass
  is dark and the ink dims. Assistive technologies read it as one image naming only what is lit:
  "Simulated MFD: mode 2, SHIFT on; GX52, Fire, blank; clock 1 12:45, date 09.11.26".
- **Line rows**, one per line, each an `adw::ComboRow` for the source, listed in this order:

  | Source | Shows |
  | --- | --- |
  | Custom text | The text typed in an `adw::EntryRow` below it (shown only for this source); a counter "n/16" counts converted bytes, the entry stops accepting input at 16 bytes or 64 typed characters, whichever comes first (a paste is cut to the room left, and the text already there is never cut), and the row lists the characters that will be replaced ("ő → o") |
  | Profile name | The active profile's name |
  | Last button | The name of the button being held, POV 1 directions included: its [custom name](#names-view) in the mode shown, else the built-in name. The mode selector positions never count. The line clears on release, except after a wheel notch, which stays for 0.7 s ([profiles.md](profiles.md#mfd-lines)) |
  | Empty | Nothing |

  A line switched from Custom text to another source keeps its text for the session: switching it
  back to Custom text in the same profile, mode settings and line brings the text back; anywhere
  else Custom text starts empty.
- **Below:** Brightness (0–128) and Shift (`adw::SwitchRow`, "Show SHIFT on the display"; X52 Pro
  only).

### Clocks view

Three `adw::PreferencesGroup`s and the date format, with no mode bar (the clocks are shared by
every mode). The times are the ones the MFD shows: clock 2 or 3 is clock 1's time plus the offset
sent. They tick at each minute and at a daylight saving change of any of the zones.

- **Clock 1:** description "Shows 14:05 · 11.11.26 · system time zone" (or "· UTC"): its time
  and the date, which follows clock 1 on the device. A "Source" row with a Local Time / UTC
  `adw::ToggleGroup`, and "24-Hour Format" (`adw::SwitchRow`).
- **Clocks 2 and 3:** description "Shows 21:45 · UTC+09:00", or "Shows 18:30 · UTC+05:45 all
  year" for a fixed offset. No date: the device shows one date, clock 1's. A "Source" row with a
  Time Zone / Fixed Offset `adw::ToggleGroup`.
  - **Time zone:** an activatable row "Time Zone" with the place and the clock's time ("Tokyo ·
    21:45"), which opens the search dialog below.
  - **Fixed offset:** "Offset From UTC", an `adw::SpinRow` written "UTC+05:45" (U+2212 for the
    minus), UTC−12:00 to UTC+14:00 in 15-minute steps, subtitle "Fixed offset, no daylight
    saving time". It also reads a typed offset ("+5:45", "-3"), which is kept as typed, on a
    15-minute step or not, as an imported GX52 0.7 offset is; the arrows step from it.
  - **Switching the source:** Time Zone → Fixed Offset takes the zone's offset now, rounded to
    15 minutes. Fixed Offset → Time Zone brings back the zone this clock of this profile had
    earlier in the session; with none, the toggle stays on Fixed Offset and the search dialog
    opens.
  - "24-Hour Format".
- **Date:** "Date Format", an `adw::ComboRow` with DD-MM-YY, MM-DD-YY, YY-MM-DD, each with today's
  date as example ("DD-MM-YY · 11.11.26").

**Time zone search:** an `adw::Dialog` "Select Time Zone" whose header bar holds a search entry
("Search cities and time zones"). The list shows one row per zone of the runtime's tz database
(`zone.tab`, with the country names of `iso3166.tab`, plus UTC), named by the city of its IANA name
("Buenos Aires" for `America/Argentina/Buenos_Aires`). The names are the database's, in English.
Each row reads "Tokyo · 21:45" over "Japan · UTC+09:00", ticking, and the clock's current zone is
marked "Current time zone". The search matches part of the city, the country or the IANA name, in
any case. Enter picks the first match, Down moves to the list, Escape closes. Nothing matching
shows "No Results Found". Without `zone.tab` the dialog lists every zone name the database knows,
without countries. The pick goes to the profile and clock the dialog was opened for. The dialog
closes when another profile is shown, and a pick still being looked up then is dropped.

### Names view

Custom names for the joystick's controls, shown by the MFD's "Last button" line and the [input
tester](#input-tester) in place of the built-in names ("Emerg. Lights" for E). Requested in
[issue #42](https://github.com/leinardi/gx52/issues/42); decided on 2026-11-19 against the
prototype ([roadmap.md](roadmap.md#1x)). The data is in [profiles.md](profiles.md#control-names).

- **Mode bar** on a per-mode profile: names are per mode and follow the bar like every other mode
  setting. Changing the profile's kind copies or drops them with the rest
  ([Profile kinds](#profile-kinds)).
- **Simulated display** at the top, the MFD view's widget. While a name field has the focus, it
  shows that control as if held, on the simulated display only: the device is not sent anything
  for it. A control held on the device takes its place.
- **No "Last button" line:** when none of the mode's MFD lines uses it, a row "Names aren't shown
  on the MFD" with an **Edit MFD Lines** button that opens the MFD view. Otherwise the page's
  description names the line ("Shown on MFD line 3…").
- **Find a Control by Pressing It:** a row that waits for the next press on the device; the press
  opens the control's group, scrolls to it and focuses its field. The mode selector is ignored.
  Without an open evdev node the row is insensitive and says why.
- **Controls:** one `adw::EntryRow` per control, in collapsible groups that follow the hardware:
  Stick, Throttle, Toggle Switches, POV 1, POV 2, Throttle Hat and MFD. That is the 36 buttons of
  the X52 Pro without the mode selector positions, plus the four POV 1 directions: 40 controls. A
  control the device type lacks is left out, as in the input tester: the X52 (non-Pro) has no MFD
  wheels (Page Up, Page Down, Up, Down) and no Select, so 35 controls.
- **Entry:** while empty, the row's title is the built-in name that applies. The entry follows the
  Custom text line of the [MFD view](#mfd-view): the "n/16" counter of converted bytes, a stop at
  16 bytes or 64 typed characters, a paste cut to the room left, and the list of characters that
  will be replaced. An empty or whitespace-only field is no name. A filled field has a reset button
  whose tooltip names the built-in name it goes back to.
- **At a glance:** the page description counts the names of the mode shown ("5 controls have
  custom names in mode 2"); each group's subtitle lists its own ("2 custom: Gear, Emerg. Lights").
  Groups with a name start expanded.
- **Menu:** **Copy Names From Mode N** (per-mode only, for the two other modes) and **Clear All
  Names** (the mode shown), each followed by a toast with Undo.
- **Narrow:** the Find row stays its own row; the header keeps only the menu button.

### A profile that failed to load

Selecting it shows an `adw::StatusPage` in the content area with the reason ("This
profile file is damaged", "Made by a newer GX52") and the actions **Show File** (opens the folder
through the portal) and **Remove**. A newer-schema profile is never edited or rewritten
([profiles.md](profiles.md#validation-and-recovery)).

## Input tester

The "Test input" page, in the content area. A live view of every input, read from evdev **without
grabbing** the device, so games keep receiving it. Layout from the maintainer's mockup, grouped by
kind of input ([spike S7](spikes/07-led-editor.md#input-tester-test-page)):

```text
Live view of the device. Input is read without grabbing it, so games keep receiving it.

Axes                                              Ministick      Toggles
┌──────────┐ ┃  X Rotation  ▬▬▬───────   77        ┌────────┐    [T1][T3][T5]
│    ●     │ ┃  Y Rotation  ▬▬▬▬▬─────  128        │   ●    │    [T2][T4][T6]
│          │ ┃  Z Rotation  ▬▬▬▬▬─────  490        └────────┘    Mode
└──────────┘ ┃  Slider      ▬─────────   31       Mouse X / Y    [ 1 | 2 | 3 ]
X / Y      Z    X 41210 · Y 20480 · Z 184

Buttons   (Trigger·1) (Fire·2) (A·3) (B·4) …          POVs   [3×3] [3×3] [3×3]
                                                       POV 1  POV 2  Throttle hat
```

- **Axes:** stick X/Y as a square with a dot, throttle Z as a vertical bar (filled as the lever
  goes forward: the X52 reports 0 at full throttle, so the bar is the inverse of the raw value),
  X/Y/Z rotation and the slider as horizontal bars, each with its raw value. Each axis fills its
  bar over the range the device reports for it (evdev's absinfo), not a range from the docs. The
  squares put the rest value in the middle, the upper of the two middle values of an even range
  (the ministick rests at 8 of 0–15), each half scaled on its own. The raw values keep the width
  of their widest values, so the cards do not change size as they move.
- **Ministick:** a second square, on both models. The X52 (non-Pro)'s codes are derived from the
  kernel's HID quirk and libx52, not tested on a device
  ([protocol.md, "X52 (non-Pro)"](protocol.md#x52-non-pro)).
- **Toggles:** T1–T6 in pairs. **Mode:** 1 / 2 / 3, the current one highlighted.
- **Buttons:** one indicator per button, lit while pressed, labelled with the name on the device
  and the button number games show ("Fire · 2"); the full name and the evdev code in the tooltip
  ("Button 2 · Fire · evdev code 289"). The number is the button's place among the device's key
  codes in ascending order, the order joydev and SDL number buttons in, the mode selector
  included. The toggles carry their numbers too. The buttons are grouped Stick, Throttle and MFD
  by control, never by evdev code, since a code is another control on the other device type; a
  control the device lacks is left out. The MFD buttons and the wheels are included. A wheel
  notch is a press and a release a few milliseconds apart, so an indicator stays lit for at least
  150 ms.
- **Wrapping:** the cards wrap and fill their line. A line of buttons asks for about 420 px, less
  than all of them in one line, so the Toggles card shares its line with the Buttons card as in
  the prototype; a card stretched alone keeps its chips at their size.
- **Custom names:** a control with a [custom name](#names-view) in the active profile, in the mode
  shown, shows that name with the built-in name smaller beside it; the tooltip and the accessible
  label give both. Named POV 1 directions are listed under its grid ("↑ Pips: SYS").
- **X52 (non-Pro):** a warning line under the intro, "The X52 input map has not been verified on a
  device yet. Names may not match the controls." ([protocol.md](protocol.md#x52-non-pro)).
- **POVs:** POV 1, POV 2 and the throttle hat as 3×3 direction grids, the cell the hat points at
  lit (a diagonal lights a corner).
- **Mode:** the selector's position after the debounce of
  [protocol.md](protocol.md#mode-selector), as the footer shows it.
- The indicators are not focusable; each group has an accessible label, and the page's state is
  readable by a screen reader as text: the Buttons group's description lists what is held
  ("Buttons pressed: Fire, A. Mode 2."), the Toggles group's names the mode. The axes have no
  such text, because of their noise.
- The axes are read only while the page is shown ([architecture.md](architecture.md)).
- **Narrow:** the groups wrap and stack into one column.
- Without a connected device, or while its evdev node is not open, the page is an
  `adw::StatusPage` "Connect your X52 or X52 Pro". Without access to the USB device (the udev
  page) the tester still works: it reads evdev, so it is shown in place of the udev page while
  "Test input" is selected.

## Device status

The sidebar footer, always visible: a status dot and two lines, the device name in bold and a
status line below it.

| State | Dot | Name | Status line |
| --- | --- | --- | --- |
| Connected, active profile Single | green | "X52 Pro" | "Connected" |
| Connected, active profile Per-mode | green | "X52 Pro" | "Connected · Mode 2" |
| No permission | red | "X52 Pro" | "No permission" |
| Waiting for a device | empty grey circle | "Not connected" | "Waiting for an X52 or X52 Pro" |
| `x52d` running | green | "X52 Pro" | a warning icon and "x52d is running" |

The mode is always the selector position on the stick, also while the editor's
[mode bar](#mode-bar) shows another mode; the same holds for the [tray](#tray-menu) status line.

The dot is decorative; the text carries the state, and the footer is one accessible element that
reads the whole line.

## Problem states

| State | What the window shows |
| --- | --- |
| No device was ever connected (no last device type) | The whole window is an `adw::StatusPage`: "Connect your X52 or X52 Pro". No profiles until a device appears. A device that is present always has a type, accessible or not, so this page never needs a "How to Fix" button: a device without permission shows its type's profiles with the udev guidance, as in the prototype (whose full-window guide cannot be reached) |
| No device now, one was connected before | The last device type's profiles, editable; the footer says "Not connected" |
| Device present, no permission (USB) | The content area is replaced by the [udev guidance](#guidance-pages) until access works; the sidebar stays |
| `x52d` running | An `adw::Banner` over the editor: "x52d is running and may overwrite these settings", with "Learn More", which opens an `adw::Dialog` "x52d Is Running": what x52d is, that GX52 does not stop it, and the command `sudo systemctl disable --now x52d` with a copy button ([protocol.md](protocol.md#libx52s-x52d)) |
| Start on Login refused with no Preferences dialog showing (as at login) | An `adw::Banner` over the content, also on the page with no device: "Start on Login is off: the desktop doesn’t allow GX52 to run in the background" (or "…: the desktop’s Background portal isn’t available"), with "How to Fix", which opens Preferences on the [Start on Login page](#guidance-pages). It goes once Preferences opens, which shows the refusal, or when the refusal ends |
| A different supported model connected | The sidebar switches to that type's profiles |

## Guidance pages

- **udev (USB access):** replaces the content area, titled "Device Access": "Allow Access to the
  Device", "GX52 found your X52 Pro but cannot open it…", then three groups. **Rule File**: "Save
  this line as /etc/udev/rules.d/70-gx52.rules" and the rule's first line (the USB one only; the
  uinput line is in Preferences, as in the prototype) with a copy button. **Load the Rule**: "No
  need to unplug…" and `sudo udevadm control --reload && sudo udevadm trigger` with a copy button.
  **Without Access**: test input keeps working; LEDs, MFD and clocks are unavailable
  ([protocol.md](protocol.md#udev-rule)). The page scrolls, so every row and Try Again stay
  reachable in the narrowest window. It goes away by itself when access works, after
  `udevadm trigger`, without a replug; a **Try Again** button checks at once and shows the toast
  "Still no access to the device" when it still fails.
- **uinput (virtual mouse only):** in Preferences. While the dialog shows, the virtual mouse switch
  is on and `/dev/uinput` is not writable (for any reason), a row "Virtual mouse unavailable",
  "GX52 cannot write to /dev/uinput", shows under the switch with a "How to Fix" button that
  pushes the subpage "Virtual Mouse Access": where to add the line, **Rule Line** (the second rule
  line, with a copy button), **Load the Rule** ("No need to log out or replug anything", the same
  command) and **Security** ("Write access to uinput lets any program you run inject keyboard and
  mouse input"). The access is checked again every second while the row shows, so it goes away by
  itself after `udevadm trigger`.
- **Start on Login (refused by the desktop):** in Preferences, as the uinput row. While the
  desktop refused Start on Login and "Run in Background" is on, a row "Start on Login is off"
  shows under the switch with the reason, "The desktop doesn’t allow GX52 to run in the
  background, so it also closes soon after its window closes" or "The desktop’s Background portal
  isn’t available", and "How to Fix", which pushes the subpage "Start on Login". Not allowed: why
  (GX52 won't start at login, and the same permission keeps it running with its window closed),
  **On GNOME** (Settings → Apps → GX52, turn background running back on), **Other Desktops** and
  **In a Terminal** (`flatpak permission-set background background <app id> yes`, with a copy
  button). Not available: **Install the Portal** (xdg-desktop-portal with the desktop's backend;
  1.19 or newer outside Flatpak). Both end with "GX52 can’t tell when this is fixed. After fixing
  it, try again." and **Try Again**, which turns Start on Login on: while the desktop answers it
  reads "Asking the Desktop…"; allowed, the subpage closes with the switch on; refused again, a
  row says "Still not allowed by the desktop" or "The Background portal is still not available".
  Unlike uinput, the row does not go away by itself: GX52 cannot read the permission, only ask.

## Preferences

An `adw::PreferencesDialog` with one page, General. Every row is bound to its GSettings key, so a
change applies at once and a change made elsewhere (the tray, `gsettings`) shows in the open dialog.

- **Background**
  - **Run in Background** (`adw::SwitchRow`, subtitle "Keep GX52 running and applying profiles
    when the window is closed"; key `run-in-background`, on by default): closing the window keeps
    GX52 running and applying profiles. With a tray host the tray icon shows; without one (plain
    GNOME, or after the tray host went away), GNOME lists GX52 under Background Apps and launching
    GX52 again shows the window.
    Turned off while no window is open (with `gsettings`), it shows the window, since the tray goes
    away with it.
  - **Start on Login** (`adw::SwitchRow`, subtitle "Start hidden when you log in. Needs Run in
    Background"; key `autostart`, off by default): starts hidden, through the Background portal
    (works in the Flatpak), with the reason "Apply joystick profiles when you log in". Only
    sensitive while "Run in Background" is on, since starting hidden needs it. Turning "Run in
    Background" off also turns it off; the application enforces this for every writer of the key,
    and at startup for values stored inconsistent while it was not running. While the desktop answers
    an enable, the switch is insensitive and its subtitle reads "Asking the desktop…". Refused by
    the user or the stored permission, or without a working portal, it turns back off with the
    [Start on Login warning](#guidance-pages) under it.
  - A start with `--hidden` (the autostart entry's command) shows no window while "Run in
    Background" is on, and the window otherwise.
- **Virtual Mouse**, description "Move the pointer with the ministick. Games still receive the
  ministick."
  - **Use the Ministick as a Mouse** (`adw::SwitchRow`; key `virtual-mouse`, off by default); the
    uinput warning above.
  - **Speed** (subtitle "Pixels per step, 100 times a second": the gain, in pixels per ministick
    step every 10 ms tick), **Curve** ("Higher values move slowly near the centre": the exponent)
    and **Deadzone** ("Steps around the centre treated as centred"), each an `adw::SpinRow`, and
    **Invert Wheel** (`adw::SwitchRow`). Insensitive while the switch is off. They default to the
    S9 values ([protocol.md](protocol.md#virtual-mouse-uinput)):

    | Row | Key | Default | Range | Step |
    | --- | --- | --- | --- | --- |
    | Speed | `virtual-mouse-speed` | 0.6 | 0.1–3.0 | 0.1 |
    | Curve | `virtual-mouse-curve` | 1.6 | 1.0–3.0 | 0.1 |
    | Deadzone | `virtual-mouse-deadzone` | 1 | 0–4 | 1 |
    | Invert Wheel | `virtual-mouse-invert-wheel` | off |  |  |

    The ranges are the schema's; the spin rows take their bounds from it. Settings only, no live
    test area.

There is no tray switch: the tray icon shows whenever GX52 runs in the background and a tray host
exists.

## Tray menu

With `ksni`, when running in the background and a tray host exists. The icon is the app's
symbolic icon (`com.leinardi.gx52-symbolic`, `.Devel` in development builds).

1. A status line, disabled: "X52 Pro · Mode 2 · DCS World" (the selector's mode, only for a
   per-mode profile). Without a device "Not connected"; with the device but no access "X52 Pro ·
   No permission"; with `x52d` running "X52 Pro · x52d is running"; before the profiles are
   loaded "X52 Pro · Connected".
2. Show GX52 / Hide GX52: first, because on GNOME a single click on the icon opens the menu
   ([S5](spikes/05-tray.md#visual-check-on-gnome-2026-11-02)); a click where the host sends one
   (KDE Plasma, a double click on GNOME) does the same. Hide closes the window with its dialogs,
   the same path as the close button.
3. **Profile** submenu: radio items with the usable profiles of the connected type, in the
   profile list's order; picking one makes it the active profile. Not shown while no device is
   connected. Names are shown as typed: an underscore is not taken as an access key.
4. Preferences, About GX52: with no window, the window is shown first and the dialog over it.
5. Quit.

## Notifications

`adw::Toast`s in an `adw::ToastOverlay` over the content:

- the problem toasts of [profiles.md](profiles.md#validation-and-recovery), one per start;
- the GX52 0.7 import: "Imported 4 profiles from GX52 0.7"; with problems, a "Details" button that
  opens the list;
- the GX52 0.7 import deferred: "Close GX52 0.7 to import its profiles";
- "“DCS World” removed" with Undo;
- "Names copied from mode 1" and "Names cleared" with Undo ([Names view](#names-view));
- "Imported “Falcon BMS”" after a profile import, and "Exported “DCS World”" after an export;
- "Copied to clipboard" after a copy button on a guidance page or dialog;
- "Still no access to the device" from the udev page's Try Again.

## About

An `adw::AboutDialog` filled from the metainfo: version and release notes, licence
GPL-3.0-or-later, developer, website, issue tracker, credits and translators. Opened from the main
menu and the tray.

## Keyboard shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+N | New Single Profile |
| Ctrl+O | Import Profile… |
| Ctrl+Shift+E | Export the selected profile |
| F2 | Rename the selected profile |
| Delete | Remove the selected profile (with the confirmation) |
| Ctrl+1, Ctrl+2, Ctrl+3, Ctrl+4 | LEDs, MFD, Clocks, Names view |
| Ctrl+, | Preferences |
| Ctrl+? | Keyboard Shortcuts |
| Ctrl+W | Close the window |
| Ctrl+Q | Quit |

## Adaptive layout

Breakpoints, checked down to 360×294:

- **Below about 600 px wide** the split view collapses: the sidebar is the first page (profiles,
  Test input, footer); selecting a profile or "Test input" pushes the content with a back button.
  The view switcher moves from the header bar to an `adw::ViewSwitcherBar` at the bottom, and the
  header bar shows the profile name instead.
- The LED cards, the MFD display and its rows, the clock groups, the name groups and the input
  tester's groups wrap and stack into one column. The mode bar stays at the top of the page.
- The window remembers its size and maximised state (GSettings `window-width`, `window-height`,
  `is-maximized`).

## Decisions replaced in the session

Recorded so the review can see what changed on the way: the first draft had a per-type **mode
mapping** (a "Modes" section in the sidebar with one profile per mode), a **preview** of a selected
profile with a "Stop preview" banner, and a tray item that mapped a profile to the current mode. The
Single / Per-mode profile kinds replaced all three on 2026-11-09.
