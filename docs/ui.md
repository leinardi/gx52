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
  content area. Selecting it does not change the active profile; the sidebar keeps a marker on the
  active profile while the tester is open.
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

Import Profile… (Ctrl+O), Preferences (Ctrl+,), Keyboard Shortcuts (Ctrl+?), About GX52.

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
as title widget, with three views: **LEDs** (Ctrl+1), **MFD** (Ctrl+2) and **Clocks** (Ctrl+3).
The profile name is not repeated there: the selected sidebar row names the profile. In the narrow
layout, where the sidebar is not visible, the header bar shows the profile name and the switcher
moves to the bottom ([Adaptive layout](#adaptive-layout)). Each view is an
`adw::PreferencesPage`-style scrolled page, content limited by an `adw::Clamp`.

Edits apply to the device at once and are saved automatically, a short moment after the last
change (500 ms), with the atomic write of profiles.md.

### Mode bar

On a per-mode profile, the LEDs and MFD views start with a bar:

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
┌──────────────────┐
│GX52              │
│Fire              │   Line 1   [ Custom text     ▾ ]  [ GX52           ]
│DCS World         │   Line 2   [ Last button     ▾ ]
│12:45     09.11.26│   Line 3   [ Profile name    ▾ ]
└──────────────────┘
  Clock [ 1 | 2 | 3 ]       Brightness ────●──── 128     Shift [  ]
```

- **Simulated display:** a small custom widget drawn like the MFD: three 16-character lines and
  the clock and date area. It shows **live values**: the line sources resolved now (the active
  profile, the current mode, the button being held), the converted characters (a character the MFD
  cannot show appears as its fallback, [protocol.md](protocol.md#mfd-character-set)), and the
  time ticking. The date uses dots, as the device draws it ("09.11.26",
  [protocol.md](protocol.md#usb-vendor-commands)). The clock area shows **clock 1**; a small
  "1 | 2 | 3" toggle under it shows the other clocks, with the note "Clock 1 is shown by default.
  Start/Stop and Reset on the throttle cycle the clocks on the device." The app cannot read which
  clock the MFD shows, so it never guesses.
- **Line rows**, one per line, each an `adw::ComboRow` for the source:

  | Source | Shows |
  | --- | --- |
  | Custom text | The text typed in an `adw::EntryRow` below it (shown only for this source); a counter "n/16" counts converted bytes, the entry stops accepting input at 16, and the row lists the characters that will be replaced ("ő → o") |
  | Profile name | The active profile's name |
  | Mode | The mode whose settings are on the device, "Mode 2": the selector position, or the mode picked in the [mode bar](#mode-bar) until the selector moves; with a Single profile active, the selector position |
  | Last button | The name of the button being held, POV 1 directions included; the mode selector positions never count. The line clears on release |
  | Empty | Nothing |

- **Below:** Brightness (0–128) and Shift (`adw::SwitchRow`, "Show SHIFT on the display"; X52 Pro
  only).

### Clocks view

Three `adw::PreferencesGroup`s and the date format:

- **Each clock group** shows that clock's current time and date as the MFD will show it, ticking.
- **Clock 1:** source Local time / UTC (`adw::ToggleGroup`), 24-hour (`adw::SwitchRow`).
- **Clocks 2 and 3:** a "Time zone | Fixed offset" `adw::ToggleGroup`.
  - **Time zone:** a row with the place and its time ("Tokyo · 21:45"); activating it opens a
    search dialog over cities and zones, like GNOME Clocks' "Add World Clock".
  - **Fixed offset:** an hours:minutes `adw::SpinRow`, UTC−12:00 to UTC+14:00 in 15-minute steps,
    subtitle "Fixed offset, no daylight saving time". An imported GX52 0.7 offset is shown here.
  - 24-hour switch.
- **Date format:** `adw::ComboRow` with DD-MM-YY, MM-DD-YY, YY-MM-DD, each with today's date as
  example.

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

- **Axes:** stick X/Y as a square with a dot, throttle Z as a vertical bar, X/Y/Z rotation and the
  slider as horizontal bars, each with its raw value.
- **Ministick:** a second square.
- **Toggles:** T1–T6 in pairs. **Mode:** 1 / 2 / 3, the current one highlighted.
- **Buttons:** one indicator per button, lit while pressed, labelled with the name on the device
  and the button number games show ("Fire · 2"); the evdev code in the tooltip. The toggles carry
  their numbers too. The buttons are grouped Stick, Throttle and MFD; the MFD buttons and the
  wheels are included.
- **X52 (non-Pro):** a warning line under the intro, "The X52 input map has not been verified on a
  device yet. Names may not match the controls." ([protocol.md](protocol.md#x52-non-pro)).
- **POVs:** POV 1, POV 2 and the throttle hat as 3×3 direction grids.
- The indicators are not focusable; each group has an accessible label, and the page's state is
  readable by a screen reader as text ("Buttons pressed: Fire").
- **Narrow:** the groups wrap and stack into one column.
- Without a connected device the page is an `adw::StatusPage` "Connect your X52 or X52 Pro".

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
| No device was ever connected (no last device type) | The whole window is an `adw::StatusPage`: "Connect your X52 or X52 Pro", with a "How to Fix" button when a device is present but not accessible. No profiles until a device appears |
| No device now, one was connected before | The last device type's profiles, editable; the footer says "Not connected" |
| Device present, no permission (USB) | The content area is replaced by the [udev guidance](#guidance-pages) until access works; the sidebar stays |
| `x52d` running | An `adw::Banner` over the editor: "x52d is running and may overwrite these settings", with "Learn More", which opens an `adw::Dialog` "x52d Is Running": what x52d is, that GX52 does not stop it, and the command `sudo systemctl disable --now x52d` with a copy button ([protocol.md](protocol.md#libx52s-x52d)) |
| A different supported model connected | The sidebar switches to that type's profiles |

## Guidance pages

- **udev (USB access):** replaces the content area. It shows the rule file with a copy button, the
  commands to install it, "no need to unplug", and what keeps working without access
  ([protocol.md](protocol.md#udev-rule)). It goes away by itself when access works, after
  `udevadm trigger`, without a replug; a **Try Again** button checks at once and shows the toast
  "Still no access to the device" when it still fails.
- **uinput (virtual mouse only):** in Preferences. When the virtual mouse switch is on and
  `/dev/uinput` is not writable, the row shows a warning and a "How to Fix" button that pushes a
  subpage of the preferences dialog with the second rule line and the security note.

## Preferences

An `adw::PreferencesDialog` with one page:

- **Background**
  - **Run in background** (`adw::SwitchRow`): closing the window keeps GX52 running and applying
    profiles. With a tray host the tray icon shows; without one (plain GNOME), GNOME lists GX52
    under Background Apps and launching GX52 again shows the window.
  - **Start on login** (`adw::SwitchRow`, subtitle "Start hidden when you log in. Needs Run in
    Background"): starts hidden, through the Background portal (works in the Flatpak).
    Only sensitive while "Run in background" is on, since starting hidden needs it;
    turning "Run in background" off also turns it off.
- **Virtual mouse**
  - **Use the ministick as a mouse** (`adw::SwitchRow`, off by default); the uinput warning above.
  - **Speed**, **Curve**, **Deadzone** (`adw::SpinRow` or scales) and **Invert wheel**
    (`adw::SwitchRow`), defaulting to the S9 values
    ([architecture.md](architecture.md#virtual-mouse)). Settings only, no live test area.

There is no tray switch: the tray icon shows whenever GX52 runs in the background and a tray host
exists.

## Tray menu

With `ksni`, when running in the background and a tray host exists:

1. A status line, disabled: "X52 Pro · Mode 2 · DCS World" (the mode only for a per-mode profile).
2. Show GX52 / Hide GX52.
3. **Profile** submenu: radio items with the profiles of the connected type; picking one makes it
   the active profile.
4. Preferences, About GX52.
5. Quit.

## Notifications

`adw::Toast`s in an `adw::ToastOverlay` over the content:

- the problem toasts of [profiles.md](profiles.md#validation-and-recovery), one per start;
- the GX52 0.7 import: "Imported 4 profiles from GX52 0.7"; with problems, a "Details" button that
  opens the list;
- the GX52 0.7 import deferred: "Close GX52 0.7 to import its profiles";
- "“DCS World” removed" with Undo;
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
| Ctrl+1, Ctrl+2, Ctrl+3 | LEDs, MFD, Clocks view |
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
- The LED cards, the MFD display and its rows, the clock groups and the input tester's groups
  wrap and stack into one column. The mode bar stays at the top of the page.
- The window remembers its size and maximised state (GSettings `window-width`, `window-height`,
  `is-maximized`).

## Decisions replaced in the session

Recorded so the review can see what changed on the way: the first draft had a per-type **mode
mapping** (a "Modes" section in the sidebar with one profile per mode), a **preview** of a selected
profile with a "Stop preview" banner, and a tray item that mapped a profile to the current mode. The
Single / Per-mode profile kinds replaced all three on 2026-11-09.
