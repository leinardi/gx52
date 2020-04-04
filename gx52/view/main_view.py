# This file is part of gx52.
#
# Copyright (c) 2018 Roberto Leinardi
#
# gx52 is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# gx52 is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with gx52.  If not, see <http://www.gnu.org/licenses/>.

import logging
from datetime import datetime
from typing import Optional, Tuple, Any, Union, List

from injector import inject, singleton
from gi.repository import Gtk

from gx52.driver.x52_driver import X52_BRIGHTNESS_MIN, X52_BRIGHTNESS_MAX, X52LedStatus, X52ColoredLedStatus

try:  # AppIndicator3 may not be installed
    import gi

    gi.require_version('AppIndicator3', '0.1')
    from gi.repository import AppIndicator3
except (ImportError, ValueError):
    AppIndicator3 = None
from gx52.di import MainBuilder
from gx52.util.view import hide_on_delete
from gx52.interactor.settings_interactor import SettingsInteractor
from gx52.view.preferences_view import PreferencesView
from gx52.model.x52_profile import X52Profile
from gx52.model.x52_pro_profile import X52ProProfile
from gx52.conf import APP_PACKAGE_NAME, APP_ID, APP_NAME, APP_VERSION, APP_SOURCE_URL, APP_ICON_NAME_SYMBOLIC
from gx52.presenter.main_presenter import MainPresenter, MainViewInterface

_LOG = logging.getLogger(__name__)
if AppIndicator3 is None:
    _LOG.warning("AppIndicator3 is not installed. The app indicator will not be shown.")


@singleton
class MainView(MainViewInterface):

    @inject
    def __init__(self,
                 presenter: MainPresenter,
                 preferences_view: PreferencesView,
                 builder: MainBuilder,
                 settings_interactor: SettingsInteractor,
                 ) -> None:
        _LOG.debug('init MainView')
        self._presenter: MainPresenter = presenter
        self._preferences_view = preferences_view
        self._presenter.main_view = self
        self._builder: Gtk.Builder = builder
        self._settings_interactor = settings_interactor
        self._first_refresh = True
        self._init_widgets()

    def _init_widgets(self) -> None:
        self._app_indicator: Optional[AppIndicator3.Indicator] = None
        self._window = self._builder.get_object("application_window")
        self._preferences_view.set_transient_for(self._window)
        self._main_menu: Gtk.Menu = self._builder.get_object("main_menu")
        self._main_infobar: Gtk.InfoBar = self._builder.get_object("main_infobar")
        self._main_infobar.connect("response", lambda b, _: b.set_revealed(False))
        self._main_infobar_label: Gtk.Label = self._builder.get_object("main_infobar_label")
        self._main_infobar.set_revealed(False)
        self._statusbar: Gtk.Statusbar = self._builder.get_object('statusbar')
        self._context = self._statusbar.get_context_id(APP_PACKAGE_NAME)
        self._app_version: Gtk.Label = self._builder.get_object('app_version')
        self._app_version.set_label(f"{APP_NAME} v{APP_VERSION}")
        self._about_dialog: Gtk.AboutDialog = self._builder.get_object("about_dialog")
        self._init_about_dialog()
        self._profile_remove_button: Gtk.Button = self._builder.get_object("profile_remove_button")
        self._main_content_stack: Gtk.Stack = self._builder.get_object('main_content_stack')
        # LEDs
        self._led_default_state_frame: Gtk.Frame = self._builder.get_object('led_default_state_frame')
        self._led_brightness_scale: Gtk.Scale = self._builder.get_object('led_brightness_scale')
        self._led_brightness_adjustment: Gtk.Adjustment = self._builder.get_object('led_brightness_adjustment')
        self._profile_treeselection: Gtk.TreeSelection = self._builder.get_object('profile_treeselection')
        self._profile_treeview: Gtk.TreeView = self._builder.get_object('profile_treeview')
        self._led_fire_combobox: Gtk.ListStore = self._builder.get_object('led_fire_combobox')
        self._led_a_combobox: Gtk.ListStore = self._builder.get_object('led_a_combobox')
        self._led_b_combobox: Gtk.ListStore = self._builder.get_object('led_b_combobox')
        self._led_pov_2_combobox: Gtk.ListStore = self._builder.get_object('led_pov_2_combobox')
        self._led_d_combobox: Gtk.ListStore = self._builder.get_object('led_d_combobox')
        self._led_e_combobox: Gtk.ListStore = self._builder.get_object('led_e_combobox')
        self._led_i_combobox: Gtk.ListStore = self._builder.get_object('led_i_combobox')
        self._led_throttle_combobox: Gtk.ListStore = self._builder.get_object('led_throttle_combobox')
        self._led_t1_t2_combobox: Gtk.ListStore = self._builder.get_object('led_t1_t2_combobox')
        self._led_t3_t4_combobox: Gtk.ListStore = self._builder.get_object('led_t3_t4_combobox')
        self._led_t5_t6_combobox: Gtk.ListStore = self._builder.get_object('led_t5_t6_combobox')
        self._profile_liststore: Gtk.ListStore = self._builder.get_object('profile_liststore')
        self._led_fire_liststore: Gtk.ListStore = self._builder.get_object('led_fire_liststore')
        self._led_a_liststore: Gtk.ListStore = self._builder.get_object('led_a_liststore')
        self._led_b_liststore: Gtk.ListStore = self._builder.get_object('led_b_liststore')
        self._led_pov_2_liststore: Gtk.ListStore = self._builder.get_object('led_pov_2_liststore')
        self._led_d_liststore: Gtk.ListStore = self._builder.get_object('led_d_liststore')
        self._led_e_liststore: Gtk.ListStore = self._builder.get_object('led_e_liststore')
        self._led_i_liststore: Gtk.ListStore = self._builder.get_object('led_i_liststore')
        self._led_throttle_liststore: Gtk.ListStore = self._builder.get_object('led_throttle_liststore')
        self._led_t1_t2_liststore: Gtk.ListStore = self._builder.get_object('led_t1_t2_liststore')
        self._led_t3_t4_liststore: Gtk.ListStore = self._builder.get_object('led_t3_t4_liststore')
        self._led_t5_t6_liststore: Gtk.ListStore = self._builder.get_object('led_t5_t6_liststore')

        # MFD
        self._mfd_brightness_scale: Gtk.Scale = self._builder.get_object('mfd_brightness_scale')
        self._mfd_brightness_adjustment: Gtk.Adjustment = self._builder.get_object('mfd_brightness_adjustment')
        self._mfd_clock_1_local_time_checkbutton: Gtk.CheckButton = self._builder \
            .get_object('mfd_clock_1_local_time_checkbutton')
        self._mfd_clock_1_12h_checkbutton: Gtk.CheckButton = self._builder \
            .get_object('mfd_clock_1_12h_checkbutton')
        self._mfd_clock_2_comboboxtext: Gtk.ComboBoxText = self._builder.get_object('mfd_clock_2_comboboxtext')
        self._mfd_clock_2_12h_checkbutton: Gtk.CheckButton = self._builder \
            .get_object('mfd_clock_2_12h_checkbutton')
        self._mfd_clock_3_comboboxtext: Gtk.ComboBoxText = self._builder.get_object('mfd_clock_3_comboboxtext')
        self._mfd_clock_3_12h_checkbutton: Gtk.CheckButton = self._builder \
            .get_object('mfd_clock_3_12h_checkbutton')
        self._mfd_date_settings_comboboxtext: Gtk.ComboBoxText = self._builder.get_object(
            'mfd_date_settings_comboboxtext')

    def _init_about_dialog(self) -> None:
        self._about_dialog.set_program_name(APP_NAME)
        self._about_dialog.set_version(APP_VERSION)
        self._about_dialog.set_website(APP_SOURCE_URL)
        self._about_dialog.connect("delete-event", hide_on_delete)
        self._about_dialog.connect("response", hide_on_delete)

    def show(self) -> None:
        self._presenter.on_start()
        self._init_app_indicator()

    def _init_app_indicator(self) -> None:
        if AppIndicator3:
            # Setting icon name in new() as '', because new() wants an icon path
            self._app_indicator = AppIndicator3.Indicator \
                .new(APP_ID, '', AppIndicator3.IndicatorCategory.HARDWARE)
            # Set the actual icon by name. If the app is not installed system-wide, the icon won't show up,
            # otherwise it will show up correctly. The set_icon_full() function needs a description for accessibility
            # purposes. I gave it the APP_NAME (should be 'gx52', maybe change it to 'GX52' in the future)
            self._app_indicator.set_icon_full(APP_ICON_NAME_SYMBOLIC, APP_NAME)
            if self._settings_interactor.get_bool('settings_show_app_indicator'):
                self._app_indicator.set_status(AppIndicator3.IndicatorStatus.ACTIVE)
            else:
                self._app_indicator.set_status(AppIndicator3.IndicatorStatus.PASSIVE)
            self._app_indicator.set_menu(self._main_menu)

    def show_main_infobar_message(self, message: str, markup: bool = False) -> None:
        if markup:
            self._main_infobar_label.set_markup(message)
        else:
            self._main_infobar_label.set_label(message)
        self._main_infobar.set_revealed(True)

    def toggle_window_visibility(self) -> None:
        if self._window.props.visible:
            self._window.hide()
        else:
            self._window.show()

    def get_use_24h(self) -> Tuple[bool, bool, bool]:
        return (not self._mfd_clock_1_12h_checkbutton.get_active(),
                not self._mfd_clock_2_12h_checkbutton.get_active(),
                not self._mfd_clock_3_12h_checkbutton.get_active())

    def get_use_local_time(self) -> bool:
        return self._mfd_clock_1_local_time_checkbutton.get_active()

    def show_about_dialog(self) -> None:
        self._about_dialog.show()

    def show_error_message_dialog(self, title: str, message: str) -> None:
        dialog = Gtk.MessageDialog(self._window, 0, Gtk.MessageType.ERROR, Gtk.ButtonsType.OK, title)
        dialog.format_secondary_text(message)
        dialog.run()
        dialog.destroy()

    def set_statusbar_text(self, text: str) -> None:
        self._statusbar.remove_all(self._context)
        self._statusbar.push(self._context, text)

    def refresh_profile_selector(self, data: List[Tuple[int, str]], active: int) -> None:
        self._profile_liststore.clear()
        for item in data:
            self._profile_liststore.append([item[0], item[1]])
        # self._profile_treeview.set_model(self._profile_liststore)
        self._profile_treeview.set_cursor(active)

    def refresh_profile_data(self, profile: Union[X52ProProfile, X52Profile]) -> None:
        if profile is not None:
            self._main_content_stack.set_sensitive(True)
            _LOG.debug('view refresh_profile_data()')
            self._profile_remove_button.set_sensitive(profile.can_be_removed)
            self._led_brightness_adjustment.set_lower(X52_BRIGHTNESS_MIN)
            self._led_brightness_adjustment.set_upper(X52_BRIGHTNESS_MAX)
            self._led_brightness_adjustment.set_value(profile.led_brightness)
            if isinstance(profile, X52ProProfile):
                self._led_default_state_frame.set_visible(True)
                self._update_led(profile, "led_fire", self._led_fire_combobox, self._led_fire_liststore)
                self._update_led(profile, "led_a", self._led_a_combobox, self._led_a_liststore)
                self._update_led(profile, "led_b", self._led_b_combobox, self._led_b_liststore)
                self._update_led(profile, "led_pov_2", self._led_pov_2_combobox, self._led_pov_2_liststore)
                self._update_led(profile, "led_d", self._led_d_combobox, self._led_d_liststore)
                self._update_led(profile, "led_e", self._led_e_combobox, self._led_e_liststore)
                self._update_led(profile, "led_i", self._led_i_combobox, self._led_i_liststore)
                self._update_led(profile, "led_throttle", self._led_throttle_combobox, self._led_throttle_liststore)
                self._update_led(profile, "led_t1_t2", self._led_t1_t2_combobox, self._led_t1_t2_liststore)
                self._update_led(profile, "led_t3_t4", self._led_t3_t4_combobox, self._led_t3_t4_liststore)
                self._update_led(profile, "led_t5_t6", self._led_t5_t6_combobox, self._led_t5_t6_liststore)
            else:
                self._led_default_state_frame.set_visible(False)

            self._mfd_brightness_adjustment.set_lower(X52_BRIGHTNESS_MIN)
            self._mfd_brightness_adjustment.set_upper(X52_BRIGHTNESS_MAX)
            self._mfd_brightness_adjustment.set_value(profile.mfd_brightness)
            self._mfd_clock_1_local_time_checkbutton.set_active(profile.clock_1_use_local_time)
            self._mfd_clock_1_12h_checkbutton.set_active(not profile.clock_1_use_24h)
            self._mfd_clock_2_comboboxtext.set_active_id(str(profile.clock_2_offset))
            self._mfd_clock_2_12h_checkbutton.set_active(not profile.clock_2_use_24h)
            self._mfd_clock_3_comboboxtext.set_active_id(str(profile.clock_3_offset))
            self._mfd_clock_3_12h_checkbutton.set_active(not profile.clock_3_use_24h)
            self._mfd_date_settings_comboboxtext.set_active(profile.date_format.value)
        else:
            self._main_content_stack.set_sensitive(False)

    @staticmethod
    def _update_led(profile: Union[X52ProProfile],
                    active_status_attr_name: str,
                    combobox: Gtk.ComboBox,
                    liststore: Gtk.ListStore) -> None:
        active_status = getattr(profile, active_status_attr_name)
        assert isinstance(active_status, (X52ColoredLedStatus, X52LedStatus))
        liststore.clear()
        for status in type(active_status):
            liststore.append([status.value, status.name.capitalize(), active_status_attr_name])
        combobox.set_model(liststore)
        combobox.set_sensitive(len(liststore) > 1)
        combobox.set_active(active_status.value)

    @staticmethod
    def _set_entry_text(label: Gtk.Entry, text: Optional[str], *args: Any) -> None:
        if text is not None and None not in args:
            label.set_sensitive(True)
            label.set_text(text.format(*args))
        else:
            label.set_sensitive(False)
            label.set_text('')

    @staticmethod
    def _set_label_markup(label: Gtk.Label, markup: Optional[str], *args: Any) -> None:
        if markup is not None and None not in args:
            label.set_sensitive(True)
            label.set_markup(markup.format(*args))
        else:
            label.set_sensitive(False)
            label.set_markup('')

    @staticmethod
    def _remove_level_bar_offsets(levelbar: Gtk.LevelBar) -> None:
        levelbar.remove_offset_value("low")
        levelbar.remove_offset_value("high")
        levelbar.remove_offset_value("full")
        levelbar.remove_offset_value("alert")

    @staticmethod
    def _set_level_bar(levelbar: Gtk.LevelBar, value: Optional[int]) -> None:
        if value is not None:
            levelbar.set_value(value / 100)
            levelbar.set_sensitive(True)
        else:
            levelbar.set_value(0)
            levelbar.set_sensitive(False)
