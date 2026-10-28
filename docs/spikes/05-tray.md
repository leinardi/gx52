# S5: tray icon (ksni)

Spike `spikes/s5-tray` on the `spikes` branch: an `adw::Application` with one window and a `ksni`
0.3.7 tray (`async-io` feature, no tokio), spawned from `glib::spawn_future_local`. Tray callbacks
send a `TrayAction` through an `async_channel`; a `glib::spawn_future_local` loop applies it on the
GTK main thread. Closing the window hides it (with an application hold) when a tray is available and
quits otherwise. Tested on 2026-10-28, Ubuntu GNOME on Wayland with the Ubuntu AppIndicators
extension (`ubuntu-appindicators@ubuntu.com`).

Verification was automated over D-Bus: read `RegisteredStatusNotifierItems` from
`org.kde.StatusNotifierWatcher`, read the item's `Id`, list the menu with
`com.canonical.dbusmenu.GetLayout`, and click items with `com.canonical.dbusmenu.Event`, which is
the call the shell makes when the user clicks.

## Results

| Scenario | Result |
|---|---|
| Host, watcher present | Registers as `org.kde.StatusNotifierItem-<pid>-1`; menu "Show / Hide" hides then shows the window, "Quit" exits; every action is handled on the main thread |
| GNOME 51 sandbox, no network, `--talk-name=org.kde.StatusNotifierWatcher`, `disable_dbus_name(true)` | Registers under its unique bus name (`:1.767@/StatusNotifierItem`); menu clicks work the same |
| GNOME 51 sandbox **without** the talk permission (equivalent to no tray host) | `watcher_offline(Error(Watcher(ServiceUnknown)))` is reported; `spawn()` still returns `Ok` because of `assume_sni_available(true)` |

## Findings for the app

- The pattern from the desk research works as is: `ksni` with `async-io`, spawned from the GLib
  main context, actions forwarded over an `async_channel`, `disable_dbus_name` when sandboxed
  (detected via `/.flatpak-info`; the app can use `ashpd::is_sandboxed()`).
- **Availability is reported asynchronously and can arrive before `spawn()` returns.** The spike's
  first version set "available" on a successful spawn and overwrote an earlier offline report, so
  closing the window would have hidden it with no icon to bring it back. The fix is a tri-state
  (`None` unknown, `Some(true)`, `Some(false)`) where a successful spawn only fills `None`. The app's
  "close hides to tray" logic must use this state.
- `watcher_online` is not called for the initial registration, only when a watcher comes back.
- Only `--talk-name=org.kde.StatusNotifierWatcher` is needed in the manifest (the lint in S6 accepts
  it); no well-known name has to be owned by the tray.

## Not done yet

- Visual check of icon and menu in the panel (needs a look on a hands-on day), including the app's
  own symbolic icon name instead of `input-gaming-symbolic`.
- Background portal (`ashpd` `request_background` with autostart): the first request shows a
  permission dialog on GNOME, so it needs a human.
- KDE Plasma (native watcher).
