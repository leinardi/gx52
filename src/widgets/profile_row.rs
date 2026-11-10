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

//! A row of the profile list: the name, the active marker, the "1 2 3" badges of a per-mode
//! profile, the warning of a profile that failed to load, and the row menu (docs/ui.md,
//! "Sidebar" and "Profile row menu").
//!
//! The menu's actions are the row's own (`row.*`), so each row enables only what applies to its
//! profile; the window carries them out and checks the profile again first.

use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};

use crate::model::{ProfileObject, ProfileState};
use crate::profile::ProfileId;
use crate::window::Window;

/// What the row menu asks the window to do with the row's profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RowAction {
    /// Edit the name in place.
    Rename,
    /// Copy it right after itself.
    Duplicate,
    /// Save it to a file.
    Export,
    /// Give it one set of settings per mode.
    UsePerMode,
    /// Keep mode 1's settings only, after a confirmation.
    UseSingle,
    /// One place up.
    MoveUp,
    /// One place down.
    MoveDown,
    /// Remove it, after a confirmation.
    Remove,
    /// Open the folder of its file.
    ShowFile,
}

/// The row actions, by name, with what they do.
const ACTIONS: [(&str, RowAction); 9] = [
    ("rename", RowAction::Rename),
    ("duplicate", RowAction::Duplicate),
    ("export", RowAction::Export),
    ("use-per-mode", RowAction::UsePerMode),
    ("use-single", RowAction::UseSingle),
    ("move-up", RowAction::MoveUp),
    ("move-down", RowAction::MoveDown),
    ("remove", RowAction::Remove),
    ("show-file", RowAction::ShowFile),
];

mod imp {
    use std::cell::{Cell, OnceCell, RefCell};

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;

    use super::ACTIONS;
    use crate::model::ProfileObject;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/profile-row.ui")]
    pub struct ProfileRow {
        #[template_child]
        pub(super) drag_handle: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) active_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) warning_icon: TemplateChild<gtk::Image>,
        #[template_child]
        pub(super) name_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub(super) title_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) name_entry: TemplateChild<gtk::Entry>,
        #[template_child]
        pub(super) name_problem: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) badges: TemplateChild<gtk::Box>,
        #[template_child]
        pub(super) badge_one: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) badge_two: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) badge_three: TemplateChild<gtk::Label>,
        #[template_child]
        pub(super) menu_button: TemplateChild<gtk::MenuButton>,
        /// The profile shown, set once.
        pub(super) item: OnceCell<ProfileObject>,
        /// The handlers on `item`, which outlives the row.
        pub(super) handlers: RefCell<Vec<glib::SignalHandlerId>>,
        /// Whether the profile can move up and down, from its place in the list.
        pub(super) moves: Cell<(bool, bool)>,
        /// Whether the active marker shows: the profile is active but another row is selected.
        pub(super) marked: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ProfileRow {
        const NAME: &'static str = "GX52ProfileRow";
        type Type = super::ProfileRow;
        type ParentType = gtk::ListBoxRow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            for (name, action) in ACTIONS {
                klass.install_action(&format!("row.{name}"), None, move |row, _, _| {
                    row.run_action(action);
                });
            }
            klass.install_action(
                "row.move-here",
                Some(glib::VariantTy::STRING),
                |row, _, parameter| {
                    if let Some(dragged) = parameter.and_then(glib::Variant::get::<String>) {
                        row.move_here(&dragged);
                    }
                },
            );
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ProfileRow {
        fn constructed(&self) {
            self.parent_constructed();
            // The row's label says it all; the icons and badges would repeat it.
            for child in [
                self.active_icon.upcast_ref::<gtk::Widget>(),
                self.drag_handle.upcast_ref(),
                self.warning_icon.upcast_ref(),
                self.title_label.upcast_ref(),
                self.badges.upcast_ref(),
            ] {
                child.update_state(&[gtk::accessible::State::Hidden(true)]);
            }
            self.obj().setup_editing();
        }

        fn dispose(&self) {
            if let Some(item) = self.item.get() {
                for handler in self.handlers.take() {
                    item.disconnect(handler);
                }
            }
        }
    }

    impl WidgetImpl for ProfileRow {}
    impl ListBoxRowImpl for ProfileRow {}
}

glib::wrapper! {
    pub struct ProfileRow(ObjectSubclass<imp::ProfileRow>)
        @extends gtk::ListBoxRow, gtk::Widget,
        @implements gtk::Accessible, gtk::Actionable, gtk::Buildable, gtk::ConstraintTarget;
}

impl ProfileRow {
    /// A row showing `item`, following its changes.
    pub(crate) fn new(item: &ProfileObject) -> Self {
        let row: Self = glib::Object::new();
        let imp = row.imp();
        // Set once, right after construction.
        let _ = imp.item.set(item.clone());
        item.bind_property("title", &*imp.title_label, "label")
            .sync_create()
            .build();
        let handlers = ["title", "state", "per-mode", "active", "mode-shown"]
            .into_iter()
            .map(|property| {
                item.connect_notify_local(
                    Some(property),
                    glib::clone!(
                        #[weak]
                        row,
                        move |_, _| row.refresh()
                    ),
                )
            })
            .collect();
        imp.handlers.replace(handlers);
        row.setup_menu_gestures();
        row.setup_drag_and_drop();
        row.refresh();
        row
    }

    /// The id of the profile shown.
    pub(crate) fn profile_id(&self) -> Option<ProfileId> {
        self.imp().item.get()?.profile_id()
    }

    /// Sets whether the profile can move up and down from its place in the list.
    pub(crate) fn set_moves(&self, up: bool, down: bool) {
        self.imp().moves.set((up, down));
        self.update_actions();
    }

    /// Shows the active marker: the profile is active, but another row is selected (a profile
    /// that failed to load, shown to see why), so the selection does not say which is active.
    pub(crate) fn set_marked(&self, marked: bool) {
        self.imp().marked.set(marked);
        self.refresh();
    }

    /// Opens the name for editing, all of it selected.
    pub(crate) fn start_rename(&self) {
        let imp = self.imp();
        let Some(item) = imp.item.get() else {
            return;
        };
        if !item.is_usable() {
            return;
        }
        imp.name_entry.set_text(&item.title());
        imp.name_problem.set_visible(false);
        imp.name_entry.remove_css_class("error");
        imp.name_stack.set_visible_child_name("entry");
        self.set_editing(true);
        imp.name_entry.grab_focus();
        imp.name_entry.select_region(0, -1);
    }

    fn setup_editing(&self) {
        let imp = self.imp();
        imp.name_entry.connect_activate(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |_| row.commit_rename()
        ));
        imp.name_entry.connect_changed(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |entry| {
                entry.remove_css_class("error");
                row.imp().name_problem.set_visible(false);
            }
        ));
        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = row)]
            self,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |_, key, _, _| {
                if key == gdk::Key::Escape {
                    row.stop_rename();
                    row.grab_focus();
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            }
        ));
        imp.name_entry.add_controller(keys);
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |_| row.stop_rename()
        ));
        imp.name_entry.add_controller(focus);
    }

    fn commit_rename(&self) {
        let imp = self.imp();
        let (Some(window), Some(id)) = (self.window(), self.profile_id()) else {
            return;
        };
        match window.commit_rename(&id, &imp.name_entry.text()) {
            Ok(()) => {
                self.stop_rename();
                self.grab_focus();
            }
            Err(problem) => {
                imp.name_entry.add_css_class("error");
                imp.name_problem.set_label(&problem.message());
                imp.name_problem.set_visible(true);
                imp.name_entry
                    .update_property(&[gtk::accessible::Property::Description(&problem.message())]);
            }
        }
    }

    /// Shows the name again instead of the entry: after Enter, Escape, or when the focus left
    /// the entry (a name not confirmed is dropped).
    fn stop_rename(&self) {
        let imp = self.imp();
        imp.name_stack.set_visible_child_name("label");
        imp.name_problem.set_visible(false);
        self.set_editing(false);
    }

    /// While the name is edited, the entry and its problem take the whole row: the marker, the
    /// badges and the menu button are hidden.
    fn set_editing(&self, editing: bool) {
        let imp = self.imp();
        let Some(item) = imp.item.get() else {
            return;
        };
        imp.active_icon
            .set_visible(!editing && item.active() && imp.marked.get());
        imp.drag_handle.set_visible(!editing);
        imp.badges.set_visible(!editing && item.per_mode());
        imp.menu_button.set_visible(!editing);
    }

    /// Right-click and long press open the row menu.
    fn setup_menu_gestures(&self) {
        let click = gtk::GestureClick::builder()
            .button(gdk::BUTTON_SECONDARY)
            .build();
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                row.imp().menu_button.popup();
            }
        ));
        self.add_controller(click);
        let press = gtk::GestureLongPress::new();
        press.connect_pressed(glib::clone!(
            #[weak(rename_to = row)]
            self,
            move |gesture, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                row.imp().menu_button.popup();
            }
        ));
        self.add_controller(press);
    }

    /// A profile other than the default can be dragged onto another row to take its place.
    fn setup_drag_and_drop(&self) {
        let Some(item) = self.imp().item.get() else {
            return;
        };
        if !item.is_usable() {
            return;
        }
        if !item.is_default() {
            let source = gtk::DragSource::builder()
                .actions(gdk::DragAction::MOVE)
                .content(&gdk::ContentProvider::for_value(&item.id().to_value()))
                .build();
            self.add_controller(source);
        }
        let target = gtk::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
        target.connect_drop(glib::clone!(
            #[weak(rename_to = row)]
            self,
            #[upgrade_or]
            false,
            move |_, value, _, _| {
                let Ok(dragged) = value.get::<String>() else {
                    return false;
                };
                row.activate_action("row.move-here", Some(&dragged.to_variant()))
                    .is_ok()
            }
        ));
        self.add_controller(target);
    }

    fn window(&self) -> Option<Window> {
        self.root().and_downcast::<Window>()
    }

    fn run_action(&self, action: RowAction) {
        if let (Some(window), Some(id)) = (self.window(), self.profile_id()) {
            window.run_row_action(action, &id);
        }
    }

    fn move_here(&self, dragged: &str) {
        if let (Some(window), Some(dragged), Some(target)) =
            (self.window(), ProfileId::parse(dragged), self.profile_id())
            && dragged != target
        {
            window.move_profile_onto(&dragged, &target);
        }
    }

    fn refresh(&self) {
        let imp = self.imp();
        let Some(item) = imp.item.get() else {
            return;
        };
        let editing = imp.name_stack.visible_child_name().as_deref() == Some("entry");
        imp.active_icon
            .set_visible(!editing && item.active() && imp.marked.get());
        if item.is_usable() && !item.is_default() {
            imp.drag_handle.add_css_class("movable");
        } else {
            imp.drag_handle.remove_css_class("movable");
        }
        imp.badges.set_visible(!editing && item.per_mode());
        let usable = item.is_usable();
        imp.warning_icon.set_visible(!usable);
        if usable {
            imp.title_label.remove_css_class("dim-label");
        } else {
            imp.title_label.add_css_class("dim-label");
        }
        for (badge, mode) in [
            (&*imp.badge_one, 1),
            (&*imp.badge_two, 2),
            (&*imp.badge_three, 3),
        ] {
            if item.mode_shown() == mode {
                badge.add_css_class("current");
            } else {
                badge.remove_css_class("current");
            }
        }
        imp.menu_button.set_menu_model(Some(&menu(item)));
        self.update_actions();
        self.update_property(&[gtk::accessible::Property::Label(&accessible_label(item))]);
    }

    /// Enables the actions that apply to this row's profile.
    fn update_actions(&self) {
        let Some(item) = self.imp().item.get() else {
            return;
        };
        let usable = item.is_usable();
        let movable = usable && !item.is_default();
        let (up, down) = self.imp().moves.get();
        for (name, enabled) in [
            ("row.rename", usable),
            ("row.duplicate", usable),
            ("row.export", usable),
            ("row.use-per-mode", usable && !item.per_mode()),
            ("row.use-single", usable && item.per_mode()),
            ("row.move-up", movable && up),
            ("row.move-down", movable && down),
            ("row.remove", !item.is_default()),
            ("row.show-file", !usable),
            ("row.move-here", usable),
        ] {
            self.action_set_enabled(name, enabled);
        }
    }
}

/// The row menu of `item`: what applies to a usable profile, the default one, or a file that
/// failed to load.
fn menu(item: &ProfileObject) -> gio::Menu {
    let menu = gio::Menu::new();
    if !item.is_usable() {
        let section = gio::Menu::new();
        section.append(Some(&gettext("_Show File")), Some("row.show-file"));
        section.append(Some(&gettext("_Remove")), Some("row.remove"));
        menu.append_section(None, &section);
        return menu;
    }
    let edit = gio::Menu::new();
    edit.append(Some(&gettext("_Rename")), Some("row.rename"));
    edit.append(Some(&gettext("_Duplicate")), Some("row.duplicate"));
    edit.append(Some(&gettext("_Export…")), Some("row.export"));
    menu.append_section(None, &edit);
    let kind = gio::Menu::new();
    if item.per_mode() {
        kind.append(
            Some(&gettext("Use _Single Settings")),
            Some("row.use-single"),
        );
    } else {
        kind.append(
            Some(&gettext("Use _Per-Mode Settings")),
            Some("row.use-per-mode"),
        );
    }
    menu.append_section(None, &kind);
    if !item.is_default() {
        let place = gio::Menu::new();
        place.append(Some(&gettext("Move _Up")), Some("row.move-up"));
        place.append(Some(&gettext("Move Do_wn")), Some("row.move-down"));
        menu.append_section(None, &place);
        let remove = gio::Menu::new();
        remove.append(Some(&gettext("_Remove")), Some("row.remove"));
        menu.append_section(None, &remove);
    }
    menu
}

/// What a screen reader says for the row of `item`.
fn accessible_label(item: &ProfileObject) -> String {
    let mut parts = vec![item.title()];
    if item.active() {
        parts.push(gettext("active"));
    }
    if item.per_mode() {
        parts.push(match item.mode_shown() {
            0 => gettext("per-mode"),
            // Translators: {mode} is the mode the device shows, 1, 2 or 3.
            mode => gettext("per-mode, mode {mode}").replace("{mode}", &mode.to_string()),
        });
    }
    match item.state() {
        ProfileState::Usable => {}
        ProfileState::Newer => parts.push(gettext("made by a newer GX52")),
        ProfileState::Damaged => parts.push(gettext("damaged")),
    }
    parts.join(", ")
}
