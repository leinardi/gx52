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
import datetime
import logging
import multiprocessing
from datetime import timedelta
from typing import Optional, Any, List, Tuple, Union

import rx
from evdev import ecodes, categorize, InputEvent
from gi.repository import Gtk, GLib
from injector import inject, singleton
from rx import Observable, operators
from rx.disposable import CompositeDisposable
from rx.scheduler import ThreadPoolScheduler
from rx.scheduler.mainloop import GtkScheduler

from gx52.conf import APP_NAME, APP_SOURCE_URL, APP_VERSION, APP_ID, APP_PACKAGE_NAME
from gx52.driver.x52_driver import X52Driver, X52DeviceType, X52DateFormat, _X52_MFD_LINE_SIZE, X52EvdevKeyMapping
from gx52.interactor.check_new_version_interactor import CheckNewVersionInteractor
from gx52.interactor.settings_interactor import SettingsInteractor
from gx52.interactor.udev_interactor import UdevInteractor
from gx52.interactor.x52_driver_interactor import X52DriverInteractor
from gx52.model.x52_profile import X52Profile
from gx52.model.x52_pro_profile import X52ProProfile
from gx52.presenter.preferences_presenter import PreferencesPresenter
from gx52.util.view import show_notification, open_uri, get_default_application
from gx52.util.x52 import get_button_name

_LOG = logging.getLogger(__name__)
_ADD_NEW_PROFILE_INDEX = -10


class MainViewInterface:
    def toggle_window_visibility(self) -> None:
        raise NotImplementedError()

    def refresh_profile_data(self, profile: Union[X52ProProfile, X52Profile]) -> None:
        raise NotImplementedError()

    def refresh_profile_selector(self, data: List[Tuple[int, str]], active: Optional[int]) -> None:
        raise NotImplementedError()

    def get_use_24h(self) -> Tuple[bool, bool, bool]:
        raise NotImplementedError()

    def get_use_local_time(self) -> bool:
        raise NotImplementedError()

    def set_statusbar_text(self, text: str) -> None:
        raise NotImplementedError()

    def show_main_infobar_message(self, message: str, markup: bool = False) -> None:
        raise NotImplementedError()

    def show_about_dialog(self) -> None:
        raise NotImplementedError()

    def show_error_message_dialog(self, title: str, message: str) -> None:
        raise NotImplementedError()


@singleton
class MainPresenter:
    @inject
    def __init__(self,
                 preferences_presenter: PreferencesPresenter,
                 x52_driver_interactor: X52DriverInteractor,
                 udev_interactor: UdevInteractor,
                 settings_interactor: SettingsInteractor,
                 check_new_version_interactor: CheckNewVersionInteractor,
                 composite_disposable: CompositeDisposable,
                 ) -> None:
        _LOG.debug("init MainPresenter ")
        self.main_view: MainViewInterface = MainViewInterface()
        self._preferences_presenter = preferences_presenter
        self._scheduler = ThreadPoolScheduler(multiprocessing.cpu_count())
        self._x52_driver_interactor = x52_driver_interactor
        self._udev_interactor = udev_interactor
        self._settings_interactor = settings_interactor
        self._check_new_version_interactor = check_new_version_interactor
        self._composite_disposable: CompositeDisposable = composite_disposable
        self._profile_selected: Optional[Union[X52ProProfile, X52Profile]] = None
        self._last_applied_profile: Optional[Union[X52ProProfile, X52Profile]] = None
        self._driver_list: List[X52Driver] = []
        self._driver_index = 0

    def on_start(self) -> None:
        self._register_db_listeners()
        self._udev_interactor.monitor_device_events(self._get_devices)
        self._get_devices()
        # self._check_new_version()

    def on_application_window_delete_event(self, *_: Any) -> bool:
        if self._settings_interactor.get_int('settings_minimize_to_tray'):
            self.on_toggle_app_window_clicked()
            return True
        return False

    def _get_devices(self) -> None:
        self._composite_disposable.add(self._x52_driver_interactor.get_devices().pipe(
            operators.subscribe_on(self._scheduler),
            operators.observe_on(GtkScheduler(GLib)),
        ).subscribe(on_next=self._handle_get_devices_result, on_error=self._handle_get_devices_result))

    def _get_current_device_type(self) -> X52DeviceType:
        return self._driver_list[self._driver_index].x52_device.device_type

    def on_menu_settings_clicked(self, *_: Any) -> None:
        self._preferences_presenter.show()

    def on_menu_changelog_clicked(self, *_: Any) -> None:
        open_uri(self._get_changelog_uri())

    def on_menu_about_clicked(self, *_: Any) -> None:
        self.main_view.show_about_dialog()

    def on_profile_selected(self, tree_selection: Gtk.TreeSelection) -> None:
        list_store, tree_iter = tree_selection.get_selected()
        if self._driver_list:
            device_type = self._get_current_device_type()
            if device_type == X52DeviceType.X52_PRO:
                profile_class = X52ProProfile
            elif device_type == X52DeviceType.X52:
                profile_class = X52Profile
            else:
                raise ValueError(f"Unsupported device type {device_type.name}")
            profile = None if tree_iter is None else profile_class.get_or_none(id=list_store.get_value(tree_iter, 0))
            if profile is not None:
                self._profile_selected = profile
                self._update_mfd_profile_name(profile.name, True)
                self.main_view.refresh_profile_data(self._profile_selected)

    def on_profile_remove_clicked(self, *_: Any) -> None:
        self._profile_selected.delete_instance(recursive=True)
        self._profile_selected = None
        self._last_applied_profile = None
        self._get_devices()

    def on_led_brightness_value_changed(self, widget: Any, *_: Any) -> None:
        brightness = int(widget.get_value())
        if brightness != self._last_applied_profile.led_brightness:
            self._last_applied_profile.led_brightness = brightness
            self._composite_disposable.add(
                self._x52_driver_interactor.set_led_brightness(self._driver_list[self._driver_index], brightness).pipe(
                    operators.subscribe_on(self._scheduler),
                    operators.observe_on(GtkScheduler(GLib)),
                ).subscribe(on_error=lambda e: self._handle_generic_set_result(e, "LED brightness")))
        if brightness != self._profile_selected.led_brightness:
            self._profile_selected.led_brightness = brightness
            self._profile_selected.save()

    def on_mfd_brightness_value_changed(self, widget: Any, *_: Any) -> None:
        brightness = int(widget.get_value())
        if brightness != self._last_applied_profile.mfd_brightness:
            self._last_applied_profile.mfd_brightness = brightness
            self._composite_disposable.add(
                self._x52_driver_interactor.set_mfd_brightness(self._driver_list[self._driver_index], brightness).pipe(
                    operators.subscribe_on(self._scheduler),
                    operators.observe_on(GtkScheduler(GLib)),
                ).subscribe(on_error=lambda e: self._handle_generic_set_result(e, "LED brightness")))
        if brightness != self._profile_selected.mfd_brightness:
            self._profile_selected.mfd_brightness = brightness
            self._profile_selected.save()

    def on_mfd_checkbuttons_toggled(self, widget: Any, *_: Any) -> None:
        _LOG.debug("on_mfd_checkbuttons_toggled")
        use_24h = self.main_view.get_use_24h()
        self._profile_selected.clock_1_use_local_time = self.main_view.get_use_local_time()
        self._profile_selected.clock_1_use_24h = use_24h[0]
        self._profile_selected.clock_2_use_24h = use_24h[1]
        self._profile_selected.clock_3_use_24h = use_24h[2]
        self._profile_selected.save()
        self._update_mfd_date_time()

    def on_mfd_clock_2_changed(self, widget: Any, *_: Any) -> None:
        offset = int(widget.get_active_id())
        if self._profile_selected.clock_2_offset != offset:
            self._profile_selected.clock_2_offset = offset
            self._profile_selected.save()
            self._update_mfd_date_time()

    def on_mfd_clock_3_changed(self, widget: Any, *_: Any) -> None:
        offset = int(widget.get_active_id())
        if self._profile_selected.clock_3_offset != offset:
            self._profile_selected.clock_3_offset = offset
            self._profile_selected.save()
            self._update_mfd_date_time()

    def on_mfd_date_settings_changed(self, widget: Any, *_: Any) -> None:
        date_format = X52DateFormat(int(widget.get_active_id()))
        if self._profile_selected.date_format != date_format:
            self._profile_selected.date_format = date_format
            self._profile_selected.save()
            self._update_mfd_date_time()

    def on_profile_name_icon_release(self, widget: Any, *_: Any) -> None:
        _LOG.debug(">>> Icon release")

    def on_profile_name_activate(self, widget: Any, *_: Any) -> None:
        profile_name = widget.get_text()
        device_type = self._get_current_device_type()
        if device_type == X52DeviceType.X52_PRO:
            self._profile_selected = X52ProProfile.create(name=profile_name)
        elif device_type == X52DeviceType.X52:
            self._profile_selected = X52Profile.create(name=profile_name)
        else:
            raise ValueError(f"Unsupported device type {device_type.name}")
        self._refresh_profile_combobox()

    def on_led_status_selected(self, widget: Any, *_: Any) -> None:
        active = widget.get_active()
        if active >= 0:
            assert self._profile_selected is not None
            enum_value = widget.get_model()[active][0]
            attr_name = widget.get_model()[active][2]
            old_led_status = getattr(self._profile_selected, attr_name)
            last_applied_led_status = getattr(self._last_applied_profile, attr_name)
            new_led_status = type(old_led_status)(enum_value)
            if last_applied_led_status != new_led_status:
                setattr(self._last_applied_profile, attr_name, new_led_status)
            self._composite_disposable.add(
                self._x52_driver_interactor.set_led_status(
                    self._driver_list[self._driver_index], new_led_status, attr_name).pipe(
                    operators.subscribe_on(self._scheduler),
                    operators.observe_on(GtkScheduler(GLib)),
                ).subscribe(on_error=lambda e: self._handle_generic_set_result(e, "LED status")))
            new_led_status = type(old_led_status)(enum_value)
            if old_led_status != new_led_status:
                setattr(self._profile_selected, attr_name, new_led_status)
                self._profile_selected.save()

    @staticmethod
    def on_quit_clicked(*_: Any) -> None:
        get_default_application().quit()

    def on_toggle_app_window_clicked(self, *_: Any) -> None:
        self.main_view.toggle_window_visibility()

    def _update_mfd_mode_line(self, text: str) -> None:
        _LOG.debug("update_mfd_mode_line")
        self._composite_disposable.add(
            self._x52_driver_interactor.set_mfd_mode_line(self._driver_list[self._driver_index], text).pipe(
                operators.subscribe_on(self._scheduler),
                operators.observe_on(GtkScheduler(GLib)),
            ).subscribe(on_error=lambda e: self._handle_generic_set_result(e, "MFD Mode")))

    def _update_mfd_button_line(self, text: str) -> None:
        _LOG.debug("update_mfd_button_line")
        self._composite_disposable.add(
            self._x52_driver_interactor.set_mfd_button_line(self._driver_list[self._driver_index], text).pipe(
                operators.subscribe_on(self._scheduler),
                operators.observe_on(GtkScheduler(GLib)),
            ).subscribe(on_error=lambda e: self._handle_generic_set_result(e, "MFD Button")))

    def _update_mfd_profile_name(self, name: str, clear_mfd: bool = False) -> None:
        self._composite_disposable.add(
            self._x52_driver_interactor.set_mfd_profile_name_line(self._driver_list[self._driver_index],
                                                                  name,
                                                                  clear_mfd).pipe(
                operators.subscribe_on(self._scheduler),
                operators.observe_on(GtkScheduler(GLib)),
            ).subscribe(on_error=lambda e: self._handle_generic_set_result(e, "MFD Profile name")))

    def _register_db_listeners(self) -> None:
        # self._speed_step_changed_subject.subscribe(on_next=self._on_speed_step_list_changed,
        #                                            on_error=lambda e: _LOG.exception(f"Db signal error: {str(e)}"))
        # self._fan_profile_changed_subject.subscribe(on_next=self._on_fan_profile_list_changed,
        #                                             on_error=lambda e: _LOG.exception(f"Db signal error: {str(e)}"))
        # self._overclock_profile_changed_subject.subscribe(on_next=self._on_overclock_profile_list_changed,
        #                                                   on_error=lambda e: _LOG.exception(
        #                                                       f"Db signal error: {str(e)}"))
        pass  # TODO

    def _refresh_profile_combobox(self) -> None:
        data: List[Tuple[int, str]] = []
        active = 0
        if self._profile_selected is not None:
            device_type = self._get_current_device_type()
            if device_type == X52DeviceType.X52_PRO:
                profile_class = X52ProProfile
            elif device_type == X52DeviceType.X52:
                profile_class = X52Profile
            else:
                raise ValueError(f"Unsupported device type {device_type.name}")
            for index, profile in enumerate(profile_class.select()):
                data.append((profile.id, profile.name))
                if profile.id == self._profile_selected.id:
                    active = index
        self.main_view.refresh_profile_selector(data, active)
        self.main_view.refresh_profile_data(self._profile_selected)

    def _log_exception_return_empty_observable(self, ex: Exception) -> Observable:
        _LOG.exception(f"Err = {ex}")
        self.main_view.set_statusbar_text(str(ex))
        return Observable.just(None)

    def _get_status(self) -> Observable:
        return self._get_status_interactor.get_devices() \
            .catch_exception(self._log_exception_return_empty_observable)

    def _check_new_version(self) -> None:
        self._composite_disposable.add(self._check_new_version_interactor.execute().pipe(
            operators.subscribe_on(self._scheduler),
            operators.observe_on(GtkScheduler(GLib)),
        ).subscribe(on_next=self._handle_new_version_response,
                    on_error=lambda e: _LOG.exception(f"Check new version error: {str(e)}"))
                                       )

    def _start_periodic_refresh(self) -> None:
        _LOG.debug("start refresh")
        self._composite_disposable.add(rx.interval(timedelta(milliseconds=999), scheduler=self._scheduler).pipe(
            operators.start_with(0),
            operators.subscribe_on(self._scheduler),
            operators.observe_on(GtkScheduler(GLib)),
        ).subscribe(on_next=self._on_periodic_refresh_tick,
                    on_error=lambda e: _LOG.exception(f"Refresh error: {str(e)}")))

    def _on_periodic_refresh_tick(self, _: Any) -> None:
        now = datetime.datetime.now()
        if now.second == 0:
            self._update_mfd_date_time()

    def _update_mfd_date_time(self) -> None:
        _LOG.debug("update_mfd_date_time")
        if self._driver_list:
            self._composite_disposable.add(
                self._x52_driver_interactor.set_date_time(self._driver_list[self._driver_index],
                                                          self._profile_selected.clock_1_use_local_time,
                                                          (self._profile_selected.clock_1_use_24h,
                                                           self._profile_selected.clock_2_use_24h,
                                                           self._profile_selected.clock_3_use_24h),
                                                          timedelta(minutes=self._profile_selected.clock_2_offset),
                                                          timedelta(minutes=self._profile_selected.clock_3_offset),
                                                          self._profile_selected.date_format).pipe(
                    operators.subscribe_on(self._scheduler),
                    operators.observe_on(GtkScheduler(GLib)),
                ).subscribe(on_error=lambda e: self._handle_generic_set_result(e, "Date")))

    def _handle_get_devices_result(self, result: Any) -> None:
        if not isinstance(result, List):
            _LOG.exception(f"Set overclock error: {str(result)}")
            self.main_view.set_statusbar_text(f'Error fetching USB devices! {str(result)}')
        else:
            assert isinstance(result, List)
            self._driver_list = result
            if result:
                device_type = self._get_current_device_type()
                if device_type == X52DeviceType.X52_PRO:
                    self._profile_selected = X52ProProfile.get(X52ProProfile.can_be_removed == False)
                    self._last_applied_profile = X52ProProfile.get_empty_profile()
                elif device_type == X52DeviceType.X52:
                    self._profile_selected = X52Profile.get(X52Profile.can_be_removed == False)
                    self._last_applied_profile = X52Profile.get_empty_profile()
                else:
                    raise ValueError(f"Unsupported device type {device_type.name}")
                self._monitor_evdev_events()
            else:
                _LOG.error("Unable to find supported X52 device!")
                self.main_view.show_error_message_dialog(
                    "Unable to find supported X52 devices",
                    "It was not possible to connect to any of the supported Logitech X52 devices.\n\n"
                    f"{APP_NAME} currently supports only Logitech X52 and X52 Pro.\n\n"
                    "If one of the supported devices is connected, try to run:\n\n"
                    f"{APP_PACKAGE_NAME} --add-udev-rule"
                )
                self._driver_list = []
                self._driver_index = 0
                self._profile_selected = None
            self._refresh_profile_combobox()
            self._update_mfd_date_time()
            self._start_periodic_refresh()

    def _monitor_evdev_events(self) -> None:
        _LOG.error("monitor_evdev_events")
        self._composite_disposable.add(
            self._x52_driver_interactor.get_evdev_events(self._driver_list[self._driver_index]).pipe(
                operators.subscribe_on(self._scheduler),
                operators.observe_on(GtkScheduler(GLib)),
            ).subscribe(on_next=self._on_evdev_event,
                        on_error=lambda e: self._handle_generic_set_result(e, "Evdev events")))

    def _on_evdev_event(self, event: InputEvent) -> None:
        _LOG.debug(f"{event.code} {event.value}")
        if event.type == ecodes.EV_KEY:
            key = X52EvdevKeyMapping(event.code)
            if X52EvdevKeyMapping.MODE_1.value <= event.code <= X52EvdevKeyMapping.MODE_3.value:
                if event.value == 0:
                    self._update_mfd_mode_line("")
                else:
                    self._update_mfd_mode_line(get_button_name(key))
            else:
                if event.value == 0:
                    self._update_mfd_button_line("")
                else:
                    self._update_mfd_button_line(get_button_name(key))
        # elif event.type == ecodes.EV_ABS:

    def _handle_generic_set_result(self, e: Exception, name: str) -> None:
        _LOG.exception(f"Set {name} error: {str(e)}")
        if e and hasattr(e, 'errno') and e.errno != 19:
            self.main_view.set_statusbar_text(f'Error changing {name}! {str(e)}')

    def _handle_new_version_response(self, version: Optional[str]) -> None:
        if version is not None:
            message = f"{APP_NAME} version <b>{version}</b> is available! " \
                      f"Click <a href=\"{self._get_changelog_uri(version)}\"><b>here</b></a> to see what's new."
            self.main_view.show_main_infobar_message(message, True)
            message = f"Version {version} is available! " \
                      f"Click here to see what's new: {self._get_changelog_uri(version)}"
            show_notification("GX52 update available!", message, APP_ID)

    @staticmethod
    def _get_changelog_uri(version: str = APP_VERSION) -> str:
        return f"{APP_SOURCE_URL}/blob/{version}/CHANGELOG.md"
