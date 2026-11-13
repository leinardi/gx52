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

//! The tray icon (docs/ui.md, "Tray menu"), a `StatusNotifierItem` served by `ksni` on its own
//! executor thread, as spike S5 built it.
//!
//! The tray item lives on `ksni`'s thread, so it holds only plain data: the [`TrayModel`] the
//! main thread last published and a channel end. Every click goes back to the main thread as a
//! [`TrayMessage`] tagged with the tray's generation, where the application applies it.

pub(crate) mod launch;
mod lifecycle;

use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use gettextrs::gettext;
use ksni::menu::{MenuItem, RadioGroup, RadioItem, StandardItem, SubMenu};

pub(crate) use self::lifecycle::{TrayGeneration, TrayLifecycle, WatcherReport};
use crate::config;
use crate::profile::ProfileId;
use crate::profile::list::{Row, RowKind};

/// What the tray menu shows, published by the main thread.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TrayModel {
    /// The disabled first line: the device, the selector's mode and the active profile.
    pub(crate) status: String,
    /// Whether a window is shown, for "Show GX52" or "Hide GX52".
    pub(crate) window_shown: bool,
    /// The connected device type's usable profiles; `None` while no device is connected.
    pub(crate) profiles: Option<ProfileChoices>,
}

/// The profiles the tray offers, in the profile list's order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileChoices {
    /// Each profile's id and name.
    pub(crate) options: Vec<(ProfileId, String)>,
    /// The position of the active profile in `options`; `None` when it is not among them (its
    /// file is damaged).
    pub(crate) active: Option<usize>,
}

impl ProfileChoices {
    /// The usable profiles among `rows` (the profile list's), with `active` checked.
    pub(crate) fn from_rows(rows: Vec<Row>, active: Option<&ProfileId>) -> Self {
        let options: Vec<(ProfileId, String)> = rows
            .into_iter()
            .filter(|row| match row.kind {
                RowKind::Usable(_) => true,
                RowKind::Newer | RowKind::Damaged => false,
            })
            .map(|row| (row.id, row.title))
            .collect();
        let active = active.and_then(|active| options.iter().position(|(id, _)| id == active));
        Self { options, active }
    }
}

/// A click in the tray, or a report about its host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TrayAction {
    /// "Show GX52" / "Hide GX52", or a click on the icon where the host sends one.
    ToggleWindow,
    /// A profile picked in the Profile submenu.
    ActivateProfile(ProfileId),
    /// "Preferences".
    Preferences,
    /// "About GX52".
    About,
    /// "Quit".
    Quit,
    /// The start returned: the tray is registered, unless a missing host was reported first.
    /// Sent after the start on the same channel, so it always follows that report.
    Registered,
    /// A tray host showed up.
    WatcherOnline,
    /// The tray host went away, or there is none.
    WatcherOffline,
}

/// A [`TrayAction`] of the tray started as `generation`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrayMessage {
    /// The tray it comes from; the application ignores every tray but the current one.
    pub(crate) generation: TrayGeneration,
    /// What happened.
    pub(crate) action: TrayAction,
    /// The XDG activation token the host gave with a click. GNOME's `AppIndicator` extension
    /// starts a launch (the busy cursor) for every click and hands its token over first; the
    /// launch lasts until the token is used, so every click's token has to be.
    pub(crate) activation_token: Option<String>,
}

/// The model the main thread publishes and the tray reads when the host asks for the menu.
///
/// A slot rather than a value moved in by each update, so updates in flight can never apply out
/// of order: the tray always reads the latest one.
#[derive(Debug, Clone, Default)]
pub(crate) struct ModelSlot(Arc<Mutex<TrayModel>>);

impl ModelSlot {
    /// Publishes `model`; the next update of the tray shows it.
    pub(crate) fn publish(&self, model: TrayModel) {
        // The model is replaced whole, so a lock poisoned by a panic still holds a whole one.
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = model;
    }

    /// The model last published.
    fn read(&self) -> TrayModel {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// A running tray, to update and shut down.
#[derive(Clone)]
pub(crate) struct TrayHandle(ksni::Handle<Gx52Tray>);

impl fmt::Debug for TrayHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrayHandle")
            .field("closed", &self.0.is_closed())
            .finish()
    }
}

impl TrayHandle {
    /// Has the host read the tray again, so it shows the model last published.
    pub(crate) async fn refresh(&self) {
        self.0.update(|_| ()).await;
    }

    /// Stops the tray; returns at once, the service ends on its own thread.
    pub(crate) fn shut_down(&self) {
        drop(self.0.shutdown());
    }
}

/// The tray item served by `ksni`.
#[derive(Debug)]
pub(crate) struct Gx52Tray {
    generation: TrayGeneration,
    model: ModelSlot,
    actions: async_channel::Sender<TrayMessage>,
}

impl Gx52Tray {
    /// The tray item of the start `generation`, showing `model` and sending its clicks to
    /// `actions`.
    pub(crate) fn new(
        generation: TrayGeneration,
        model: ModelSlot,
        actions: async_channel::Sender<TrayMessage>,
    ) -> Self {
        Self {
            generation,
            model,
            actions,
        }
    }

    /// Serves the tray item until it is shut down. In the sandbox, without the right to own a
    /// well-known bus name, it registers under its unique name (S5); a missing tray host is
    /// reported through [`TrayAction::WatcherOffline`], not as an error.
    ///
    /// # Errors
    ///
    /// When the session bus cannot be reached.
    pub(crate) async fn spawn(self, sandboxed: bool) -> Result<TrayHandle, ksni::Error> {
        use ksni::TrayMethods as _;
        self.disable_dbus_name(sandboxed)
            .assume_sni_available(true)
            .spawn()
            .await
            .map(TrayHandle)
    }

    fn send(&self, action: TrayAction) {
        let message = TrayMessage {
            generation: self.generation,
            action,
            // Only set inside a click's callback, which is where this runs for a click.
            activation_token: ksni::current_xdg_activation_token(),
        };
        // Unbounded: never blocks ksni's thread. It only fails once the application is gone.
        if let Err(error) = self.actions.try_send(message) {
            tracing::debug!("Dropped a tray action: {error}");
        }
    }
}

impl ksni::Tray for Gx52Tray {
    fn id(&self) -> String {
        config::APP_ID.to_owned()
    }

    fn title(&self) -> String {
        gettext("GX52")
    }

    fn icon_name(&self) -> String {
        format!("{}-symbolic", config::APP_ID)
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    // KDE Plasma sends this on a click, GNOME's AppIndicator extension on a double click (S5).
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(TrayAction::ToggleWindow);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        menu(&self.model.read())
    }

    fn watcher_online(&self) {
        self.send(TrayAction::WatcherOnline);
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        tracing::info!("No tray host: {reason:?}");
        self.send(TrayAction::WatcherOffline);
        // Keeps the service, so the icon comes back with the host.
        true
    }
}

/// `text` as a menu label shows it: the menu protocol (dbusmenu) takes `_` for the access key
/// mark, so a profile named `A_B` would show as `AB`.
fn literal_label(text: &str) -> String {
    text.replace('_', "__")
}

/// A menu item that sends `action`.
fn action_item(label: String, action: TrayAction) -> MenuItem<Gx52Tray> {
    StandardItem {
        label,
        activate: Box::new(move |tray: &mut Gx52Tray| tray.send(action.clone())),
        ..Default::default()
    }
    .into()
}

/// The menu of docs/ui.md, "Tray menu". Show / Hide comes right after the status line: on GNOME a
/// single click opens the menu instead of activating the icon (S5).
fn menu(model: &TrayModel) -> Vec<MenuItem<Gx52Tray>> {
    let toggle = if model.window_shown {
        gettext("Hide GX52")
    } else {
        gettext("Show GX52")
    };
    let mut items = vec![
        StandardItem {
            label: literal_label(&model.status),
            enabled: false,
            ..Default::default()
        }
        .into(),
        action_item(toggle, TrayAction::ToggleWindow),
    ];
    if let Some(profiles) = &model.profiles {
        // The ids of the menu being built: a pick refers to this list, whatever changes later.
        let ids: Vec<ProfileId> = profiles.options.iter().map(|(id, _)| id.clone()).collect();
        let group = RadioGroup {
            // An index past the options checks none.
            selected: profiles.active.unwrap_or(usize::MAX),
            select: Box::new(move |tray: &mut Gx52Tray, index| {
                if let Some(id) = ids.get(index) {
                    tray.send(TrayAction::ActivateProfile(id.clone()));
                }
            }),
            options: profiles
                .options
                .iter()
                .map(|(_, name)| RadioItem {
                    label: literal_label(name),
                    ..Default::default()
                })
                .collect(),
        };
        items.push(
            SubMenu {
                label: gettext("Profile"),
                submenu: vec![group.into()],
                ..Default::default()
            }
            .into(),
        );
    }
    items.extend([
        MenuItem::Separator,
        action_item(gettext("Preferences"), TrayAction::Preferences),
        action_item(gettext("About GX52"), TrayAction::About),
        MenuItem::Separator,
        action_item(gettext("Quit"), TrayAction::Quit),
    ]);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::ProfileKind;

    fn id(text: &str) -> ProfileId {
        ProfileId::parse(text).expect("a valid id")
    }

    fn tray(model: TrayModel) -> (Gx52Tray, async_channel::Receiver<TrayMessage>) {
        let mut lifecycle = TrayLifecycle::<()>::default();
        let generation = lifecycle.enable().expect("the tray is off");
        let slot = ModelSlot::default();
        slot.publish(model);
        let (sender, receiver) = async_channel::unbounded();
        (Gx52Tray::new(generation, slot, sender), receiver)
    }

    /// Each item's label (an empty one for a separator), the submenu's labels indented.
    fn labels(items: &[MenuItem<Gx52Tray>]) -> Vec<String> {
        let mut labels = Vec::new();
        for item in items {
            match item {
                MenuItem::Standard(item) => labels.push(item.label.clone()),
                MenuItem::Separator => labels.push(String::new()),
                MenuItem::SubMenu(menu) => {
                    labels.push(menu.label.clone());
                    for inner in &menu.submenu {
                        if let MenuItem::RadioGroup(group) = inner {
                            labels.extend(
                                group
                                    .options
                                    .iter()
                                    .map(|option| format!("  {}", option.label)),
                            );
                        }
                    }
                }
                MenuItem::Checkmark(item) => labels.push(item.label.clone()),
                MenuItem::RadioGroup(_) => labels.push("radio".to_owned()),
            }
        }
        labels
    }

    fn connected() -> TrayModel {
        TrayModel {
            status: "X52 Pro · DCS World".to_owned(),
            window_shown: true,
            profiles: Some(ProfileChoices {
                options: vec![
                    (id("x52-pro-default"), "Default".to_owned()),
                    (id("abc"), "DCS World".to_owned()),
                ],
                active: Some(1),
            }),
        }
    }

    #[test]
    fn the_menu_follows_the_ui_spec() {
        let (tray, _) = tray(connected());
        let items = ksni::Tray::menu(&tray);
        assert_eq!(
            labels(&items),
            [
                "X52 Pro · DCS World",
                "Hide GX52",
                "Profile",
                "  Default",
                "  DCS World",
                "",
                "Preferences",
                "About GX52",
                "",
                "Quit",
            ]
        );
        let MenuItem::Standard(status) = &items[0] else {
            panic!("the status line is a standard item");
        };
        assert!(!status.enabled, "the status line cannot be clicked");
        let MenuItem::SubMenu(profiles) = &items[2] else {
            panic!("the profiles are a submenu");
        };
        let MenuItem::RadioGroup(group) = &profiles.submenu[0] else {
            panic!("the profiles are radio items");
        };
        assert_eq!(group.selected, 1, "the active profile is checked");
    }

    #[test]
    fn a_hidden_window_offers_show_and_no_device_no_profiles() {
        let model = TrayModel {
            status: "Not connected".to_owned(),
            window_shown: false,
            profiles: None,
        };
        let (tray, _) = tray(model);
        assert_eq!(
            labels(&ksni::Tray::menu(&tray)),
            [
                "Not connected",
                "Show GX52",
                "",
                "Preferences",
                "About GX52",
                "",
                "Quit"
            ]
        );
    }

    #[test]
    fn underscores_in_names_are_shown_not_taken_as_access_keys() {
        let mut model = connected();
        model.status = "X52 Pro · My_Sim".to_owned();
        if let Some(profiles) = &mut model.profiles {
            profiles.options[1].1 = "My_Sim".to_owned();
        }
        let (tray, _) = tray(model);
        let labels = labels(&ksni::Tray::menu(&tray));
        assert_eq!(labels[0], "X52 Pro · My__Sim");
        assert_eq!(labels[4], "  My__Sim");
    }

    #[test]
    fn clicks_carry_the_trays_generation() {
        let (mut tray, receiver) = tray(connected());
        let items = ksni::Tray::menu(&tray);
        let MenuItem::Standard(toggle) = &items[1] else {
            panic!("Show / Hide is a standard item");
        };
        (toggle.activate)(&mut tray);
        let MenuItem::SubMenu(profiles) = &items[2] else {
            panic!("the profiles are a submenu");
        };
        let MenuItem::RadioGroup(group) = &profiles.submenu[0] else {
            panic!("the profiles are radio items");
        };
        (group.select)(&mut tray, 0);
        (group.select)(&mut tray, 7);
        ksni::Tray::activate(&mut tray, 0, 0);
        let actions: Vec<TrayAction> = std::iter::from_fn(|| receiver.try_recv().ok())
            .map(|message| {
                assert_eq!(message.generation, tray.generation);
                message.action
            })
            .collect();
        assert_eq!(
            actions,
            [
                TrayAction::ToggleWindow,
                TrayAction::ActivateProfile(id("x52-pro-default")),
                TrayAction::ToggleWindow,
            ],
            "an index past the list sends nothing"
        );
    }

    #[test]
    fn a_pick_refers_to_the_menu_it_was_made_in() {
        let (mut tray, receiver) = tray(connected());
        let items = ksni::Tray::menu(&tray);
        // The list changes before the click arrives.
        tray.model.publish(TrayModel::default());
        let MenuItem::SubMenu(profiles) = &items[2] else {
            panic!("the profiles are a submenu");
        };
        let MenuItem::RadioGroup(group) = &profiles.submenu[0] else {
            panic!("the profiles are radio items");
        };
        (group.select)(&mut tray, 1);
        assert_eq!(
            receiver.try_recv().map(|message| message.action),
            Ok(TrayAction::ActivateProfile(id("abc")))
        );
    }

    fn row(id_text: &str, title: &str, kind: RowKind) -> Row {
        Row {
            id: id(id_text),
            title: title.to_owned(),
            kind,
        }
    }

    #[test]
    fn only_usable_profiles_are_offered_in_list_order() {
        let rows = vec![
            row(
                "x52-pro-default",
                "Default",
                RowKind::Usable(ProfileKind::Single),
            ),
            row("abc", "DCS World", RowKind::Usable(ProfileKind::PerMode)),
            row("newer", "newer.toml", RowKind::Newer),
            row("damaged", "damaged.toml", RowKind::Damaged),
        ];
        let choices = ProfileChoices::from_rows(rows.clone(), Some(&id("abc")));
        assert_eq!(
            choices,
            ProfileChoices {
                options: vec![
                    (id("x52-pro-default"), "Default".to_owned()),
                    (id("abc"), "DCS World".to_owned()),
                ],
                active: Some(1),
            }
        );
        assert_eq!(
            ProfileChoices::from_rows(rows.clone(), Some(&id("damaged"))).active,
            None,
            "an active profile that is not usable checks none"
        );
        assert_eq!(ProfileChoices::from_rows(rows, None).active, None);
    }

    #[test]
    fn the_tray_reads_the_latest_model() {
        let (tray, _) = tray(connected());
        let mut hidden = connected();
        hidden.window_shown = false;
        tray.model.publish(hidden);
        assert_eq!(labels(&ksni::Tray::menu(&tray))[1], "Show GX52");
    }
}
