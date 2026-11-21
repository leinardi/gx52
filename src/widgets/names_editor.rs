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

//! The Names view of the profile editor: the custom names of the controls in the mode shown, with
//! the simulated display, the search by pressing a control and the Copy and Clear actions
//! (docs/internal/ui.md, "Names view").

use std::cell::Cell;
use std::collections::BTreeMap;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};
use jiff::tz::TimeZone;
use x52::device::DeviceType;
use x52::input::{Input, InputMap, Mode};

use crate::device::names::input_name;
use crate::profile::editor::EditorState;
use crate::profile::mfd::{self, ShownClock};
use crate::profile::names::{self, NameGroup};
use crate::profile::{MfdLineSource, ProfileId, ProfileKind, mode_index};
use crate::time;
use crate::widgets::mfd_display::DateArea;
use crate::widgets::mfd_entry::MfdEntry;
use crate::widgets::ticker::{Ticker, Ticking};
use crate::window::Window;

/// How many frames a reveal tries to scroll its row into view: about a second.
const REVEAL_FRAMES: u32 = 60;

/// The room left above or below a revealed row, in pixels.
const REVEAL_MARGIN: f64 = 12.0;

/// The row of one control.
#[derive(Debug)]
struct NameRow {
    /// The control.
    input: Input,
    /// The entry of its name.
    entry: MfdEntry,
    /// Goes back to the built-in name.
    reset: gtk::Button,
}

/// The expander row of one group and the rows of its controls.
#[derive(Debug)]
struct GroupRows {
    /// Its expander row.
    expander: adw::ExpanderRow,
    /// Its controls, in order.
    rows: Vec<NameRow>,
}

mod imp {
    use std::cell::{Cell, RefCell};

    use adw::subclass::prelude::*;
    use gtk::glib;
    use gtk::prelude::*;
    use x52::device::DeviceType;
    use x52::input::{Input, Mode};

    use super::GroupRows;
    use crate::profile::{ProfileId, ProfileKind};
    use crate::widgets::ticker::Ticker;
    use crate::widgets::{MfdDisplay, ModeBar};

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/leinardi/gx52/ui/names-editor.ui")]
    pub struct NamesEditor {
        #[template_child]
        pub(super) scroller: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub(super) mode_bar: TemplateChild<ModeBar>,
        #[template_child]
        pub(super) display: TemplateChild<MfdDisplay>,
        #[template_child]
        pub(super) no_line_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) names_group: TemplateChild<adw::PreferencesGroup>,
        #[template_child]
        pub(super) menu_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub(super) find_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) finding_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub(super) cancel_find_button: TemplateChild<gtk::Button>,
        /// The groups and their rows, for the device type they were built for.
        pub(super) groups: RefCell<Vec<GroupRows>>,
        /// The device type the rows are built for.
        pub(super) built_for: Cell<Option<DeviceType>>,
        /// The profile and device type last shown: a find ends when another is shown.
        pub(super) shown: RefCell<Option<(ProfileId, DeviceType)>>,
        /// The profile and mode the groups were last expanded for: groups with a name start
        /// expanded.
        pub(super) expanded_for: RefCell<Option<(ProfileId, Mode)>>,
        /// The kind and mode the menu was built for.
        pub(super) menu_for: Cell<Option<(ProfileKind, Mode)>>,
        /// The control whose name has the focus, shown on the display as if held.
        pub(super) focused: Cell<Option<Input>>,
        /// The minute tick of the display.
        pub(super) ticker: Ticker,
        /// Set while the editor shows a state itself, so the change is not taken as the user's.
        pub(super) syncing: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NamesEditor {
        const NAME: &'static str = "GX52NamesEditor";
        type Type = super::NamesEditor;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            ModeBar::ensure_type();
            MfdDisplay::ensure_type();
            klass.bind_template();
            klass.install_action(
                "names.copy-from",
                Some(glib::VariantTy::BYTE),
                |editor, _, mode| {
                    let mode = mode.and_then(glib::Variant::get::<u8>).and_then(|number| {
                        Mode::ALL.get(usize::from(number).checked_sub(1)?).copied()
                    });
                    if let Some(mode) = mode
                        && let Some(app) = crate::widgets::app_of(editor)
                    {
                        app.copy_names_from(mode);
                    }
                },
            );
            klass.install_action("names.clear", None, |editor, _, _| {
                if let Some(app) = crate::widgets::app_of(editor) {
                    app.clear_names();
                }
            });
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for NamesEditor {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_find();
            Ticker::tick(&*obj);
        }
    }

    impl WidgetImpl for NamesEditor {
        fn map(&self) {
            self.parent_map();
            Ticker::tick(&*self.obj());
        }

        fn unmap(&self) {
            self.ticker.stop();
            // A find belongs to the view: leaving it, by another view, another page or the window
            // hidden, ends it.
            self.obj().cancel_own_find();
            self.parent_unmap();
        }
    }

    impl BinImpl for NamesEditor {}
}

glib::wrapper! {
    pub struct NamesEditor(ObjectSubclass<imp::NamesEditor>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl NamesEditor {
    /// Starts and ends a find from the rows.
    fn setup_find(&self) {
        let imp = self.imp();
        imp.find_row.connect_activated(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| {
                let window = editor.root().and_downcast::<Window>();
                if let Some(window) = window
                    && let Some(app) = window.app()
                {
                    app.start_find(&window);
                    editor.show_finding(true);
                }
            }
        ));
        imp.cancel_find_button.connect_clicked(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| {
                editor.cancel_own_find();
                editor.show_finding(false);
            }
        ));
    }

    /// Ends the find of this view's window, if it waits; a find of another window goes on.
    fn cancel_own_find(&self) {
        if let Some(window) = self.root().and_downcast::<Window>()
            && let Some(app) = window.app()
        {
            app.cancel_find_for(&window);
        }
    }

    /// Shows the find as waiting for a press, or not.
    pub(crate) fn show_finding(&self, finding: bool) {
        let imp = self.imp();
        imp.find_row.set_visible(!finding);
        imp.finding_row.set_visible(finding);
    }

    /// Builds the groups and rows of `device_type`, unless they are already.
    fn build_for(&self, device_type: DeviceType) {
        let imp = self.imp();
        if imp.built_for.replace(Some(device_type)) == Some(device_type) {
            return;
        }
        for group in imp.groups.take() {
            imp.names_group.remove(&group.expander);
        }
        imp.expanded_for.take();
        let map = InputMap::for_device(device_type);
        let groups = NameGroup::ALL
            .into_iter()
            .map(|group| {
                // The subtitle lists names as typed: never markup.
                let expander = adw::ExpanderRow::builder()
                    .title(group.title())
                    .use_markup(false)
                    .build();
                let rows = group
                    .nameable(&map)
                    .map(|input| {
                        let row = self.name_row(&map, input);
                        expander.add_row(row.entry.row());
                        row
                    })
                    .collect();
                imp.names_group.add(&expander);
                GroupRows { expander, rows }
            })
            .collect();
        imp.groups.replace(groups);
    }

    /// The row of `input`'s name on a device of `map`, connected.
    fn name_row(&self, map: &InputMap, input: Input) -> NameRow {
        let builtin = input_name(input);
        let row = adw::EntryRow::builder()
            .title(&builtin)
            .use_markup(false)
            .build();
        // The button number or the POV 1 arrow, as the input tester shows it. Hidden from screen
        // readers: the title already names the control.
        if let Some(badge) = names::badge(map, input) {
            let label = gtk::Label::builder()
                .label(badge)
                .valign(gtk::Align::Center)
                .accessible_role(gtk::AccessibleRole::Presentation)
                .css_classes(["input-number"])
                .build();
            row.add_prefix(&label);
        }
        // Translators: {name} is the built-in name of a control, such as "E".
        let reset_label = gettext("Reset to “{name}”").replace("{name}", &builtin);
        let reset = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .valign(gtk::Align::Center)
            .tooltip_text(&reset_label)
            .css_classes(["flat"])
            .build();
        reset.update_property(&[gtk::accessible::Property::Label(&reset_label)]);
        row.add_suffix(&reset);
        let entry = MfdEntry::new(
            row.clone(),
            mfd::accepted_name_insert,
            glib::clone!(
                #[weak(rename_to = editor)]
                self,
                #[upgrade_or]
                true,
                move || editor.imp().syncing.get()
            ),
        );
        row.connect_changed(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |row| {
                if editor.imp().syncing.get() {
                    return;
                }
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_name(input, &row.text());
                }
            }
        ));
        reset.connect_clicked(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| {
                if let Some(app) = crate::widgets::app_of(&editor) {
                    app.set_name(input, "");
                }
            }
        ));
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| {
                editor.imp().focused.set(Some(input));
                editor.refresh_display();
            }
        ));
        focus.connect_leave(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            move |_| {
                let imp = editor.imp();
                if imp.focused.get() == Some(input) {
                    imp.focused.set(None);
                    editor.refresh_display();
                }
            }
        ));
        row.add_controller(focus);
        NameRow {
            input,
            entry,
            reset,
        }
    }

    /// Shows `state`.
    pub(crate) fn show(&self, state: &EditorState) {
        let imp = self.imp();
        let shown = (state.profile.clone(), state.device_type);
        if imp
            .shown
            .replace(Some(shown.clone()))
            .is_some_and(|last| last != shown)
        {
            // Another profile or device type: the find was for the one before.
            self.cancel_own_find();
        }
        self.build_for(state.device_type);
        let per_mode = state.kind == ProfileKind::PerMode;
        imp.mode_bar.set_visible(per_mode);
        imp.mode_bar.show(state.mode);
        let names = &state.settings.names;
        imp.syncing.set(true);
        for group in imp.groups.borrow().iter() {
            for row in &group.rows {
                let text = names.get(&row.input).map_or("", String::as_str);
                let entry = row.entry.row();
                // Only a different text is set: setting it again would move the cursor. A blank
                // field is no name, as the stored one is, so a space typed first stays.
                let typed = entry.text();
                let same = typed == text || (text.is_empty() && typed.trim().is_empty());
                if !same {
                    entry.set_text(text);
                }
                row.entry.show_info(text);
                row.reset.set_visible(!text.is_empty());
            }
            let custom: Vec<&str> = group
                .rows
                .iter()
                .filter_map(|row| names.get(&row.input).map(String::as_str))
                .collect();
            group.expander.set_subtitle(&names::group_summary(&custom));
        }
        imp.syncing.set(false);
        self.expand_named(&state.profile, state.mode, names);
        self.show_summary(state, per_mode);
        self.show_menu(state.kind, state.mode);
        self.action_set_enabled("names.clear", !names.is_empty());
        self.show_find_row();
        self.refresh_display();
    }

    /// Expands the groups that have a name, once per profile and mode shown.
    fn expand_named(&self, profile: &ProfileId, mode: Mode, names: &BTreeMap<Input, String>) {
        let imp = self.imp();
        let key = (profile.clone(), mode);
        if imp.expanded_for.borrow().as_ref() == Some(&key) {
            return;
        }
        imp.expanded_for.replace(Some(key));
        for group in imp.groups.borrow().iter() {
            let has_custom = group.rows.iter().any(|row| names.contains_key(&row.input));
            group.expander.set_expanded(has_custom);
        }
    }

    /// The group description and the row shown without a "Last button" line.
    fn show_summary(&self, state: &EditorState, per_mode: bool) {
        let imp = self.imp();
        let count = state.settings.names.len();
        let mut description = names::count_summary(count, per_mode.then_some(state.mode));
        let line = state
            .settings
            .mfd_lines
            .iter()
            .position(|line| *line == MfdLineSource::LastButton);
        if let Some(line) = line {
            description.push(' ');
            description.push_str(&names::shown_on_line(line));
        }
        imp.names_group.set_description(Some(&description));
        imp.no_line_group.set_visible(line.is_none());
    }

    /// The menu: Copy from each other mode of a per-mode profile, and Clear.
    fn show_menu(&self, kind: ProfileKind, mode: Mode) {
        let imp = self.imp();
        if imp.menu_for.replace(Some((kind, mode))) == Some((kind, mode)) {
            return;
        }
        let menu = gio::Menu::new();
        if kind == ProfileKind::PerMode {
            let copy = gio::Menu::new();
            for from in Mode::ALL.into_iter().filter(|from| *from != mode) {
                let number = u8::try_from(mode_index(from) + 1).unwrap_or(1);
                // Translators: {mode} is the number of a mode of the mode selector, 1 to 3.
                let label =
                    gettext("_Copy Names From Mode {mode}").replace("{mode}", &number.to_string());
                let item = gio::MenuItem::new(Some(&label), None);
                item.set_action_and_target_value(
                    Some("names.copy-from"),
                    Some(&number.to_variant()),
                );
                copy.append_item(&item);
            }
            menu.append_section(None, &copy);
        }
        let clear = gio::Menu::new();
        clear.append(Some(&gettext("C_lear All Names")), Some("names.clear"));
        menu.append_section(None, &clear);
        imp.menu_button.set_menu_model(Some(&menu));
    }

    /// The Find row: sensitive only while the device's input node is open, and saying why not.
    fn show_find_row(&self) {
        let imp = self.imp();
        let Some(app) = crate::widgets::app_of(self) else {
            return;
        };
        let status = app.device_status();
        let ready = status.device_type.is_some() && status.input_open;
        imp.find_row.set_sensitive(ready);
        imp.find_row.set_subtitle(&if ready {
            gettext("Press a button or hat on the device to go to its name")
        } else {
            gettext("Connect the joystick to find a control by pressing it")
        });
        let finding = self
            .root()
            .and_downcast::<Window>()
            .is_some_and(|window| app.is_finding(&window));
        self.show_finding(finding && ready);
    }

    /// Opens the group of `input`, scrolls to its row and focuses its name.
    pub(crate) fn reveal_name(&self, input: Input) {
        self.show_finding(false);
        let row = self.imp().groups.borrow().iter().find_map(|group| {
            let row = group.rows.iter().find(|row| row.input == input)?;
            group.expander.set_expanded(true);
            Some(row.entry.row().clone())
        });
        let Some(row) = row else {
            return;
        };
        row.grab_focus();
        // A group that was closed gives its rows a place only in a later layout, and the page
        // grows while it opens: the scroll is tried again each frame until the row is in view.
        let frames = Cell::new(0_u32);
        row.add_tick_callback(glib::clone!(
            #[weak(rename_to = editor)]
            self,
            #[upgrade_or]
            glib::ControlFlow::Break,
            move |row, _| {
                frames.set(frames.get() + 1);
                if editor.scroll_to(row) || frames.get() >= REVEAL_FRAMES {
                    glib::ControlFlow::Break
                } else {
                    glib::ControlFlow::Continue
                }
            }
        ));
    }

    /// Scrolls the page so `row` is in view; whether it is.
    fn scroll_to(&self, row: &adw::EntryRow) -> bool {
        let Some((top, bottom)) = self.row_span(row) else {
            return false;
        };
        let adjustment = self.imp().scroller.vadjustment();
        let value = adjustment.value();
        let page = adjustment.page_size();
        if top < value {
            adjustment.set_value(top);
        } else if bottom > value + page {
            adjustment.set_value(bottom - page);
        }
        self.in_view(row)
    }

    /// The top and bottom of `row` in the page, with a margin, once it is laid out.
    fn row_span(&self, row: &adw::EntryRow) -> Option<(f64, f64)> {
        // The scroller wraps the page in a viewport, whose own coordinates move with the scroll.
        let content = self
            .imp()
            .scroller
            .child()
            .and_downcast::<gtk::Viewport>()?
            .child()?;
        if !row.is_mapped() {
            return None;
        }
        let bounds = row.compute_bounds(&content)?;
        if bounds.height() <= 0.0 {
            return None;
        }
        let top = f64::from(bounds.y()) - REVEAL_MARGIN;
        let bottom = f64::from(bounds.y() + bounds.height()) + REVEAL_MARGIN;
        Some((top.max(0.0), bottom))
    }

    /// Whether `row` is laid out and wholly in view.
    fn in_view(&self, row: &adw::EntryRow) -> bool {
        let Some((top, bottom)) = self.row_span(row) else {
            return false;
        };
        let adjustment = self.imp().scroller.vadjustment();
        let value = adjustment.value();
        top >= value - 1.0 && bottom <= value + adjustment.page_size() + 1.0
    }

    /// Whether the row of `input` is laid out and wholly in view.
    pub(crate) fn shows_name(&self, input: Input) -> bool {
        self.imp()
            .groups
            .borrow()
            .iter()
            .flat_map(|group| &group.rows)
            .find(|row| row.input == input)
            .is_some_and(|row| self.in_view(row.entry.row()))
    }

    /// The simulated display.
    pub(crate) fn display(&self) -> crate::widgets::MfdDisplay {
        self.imp().display.get()
    }

    /// Whether the rows of a device type are built.
    pub(crate) fn rows_built(&self) -> bool {
        self.imp().built_for.get().is_some()
    }

    /// Shows the display again: the dynamic lines and the time follow the device and the clock.
    pub(crate) fn refresh_display(&self) {
        let imp = self.imp();
        let Some(zone) = imp.ticker.zone() else {
            // The zone is read when the editor is mapped; the display waits for it.
            return;
        };
        let Some(app) = crate::widgets::app_of(self) else {
            return;
        };
        let preview = app.mfd_preview(ShownClock::One, time::now(), &zone, imp.focused.get());
        if let Some(preview) = preview {
            imp.display.show(&preview, DateArea::Date);
        }
    }
}

impl Ticking for NamesEditor {
    fn ticker(&self) -> &Ticker {
        &self.imp().ticker
    }

    fn on_tick(&self, _zone: &TimeZone) {
        self.refresh_display();
    }
}
