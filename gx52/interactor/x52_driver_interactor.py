# This file is part of gx52
#
# Copyright (c) 2020 Roberto Leinardi
#
# gst is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# gst is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with gst.  If not, see <http://www.gnu.org/licenses/>.
import datetime
import logging
from typing import Union, Tuple

import reactivex
from injector import singleton, inject
from reactivex import Observable

from gx52.driver.x52_driver import X52Driver, X52ColoredLedStatus, X52LedStatus, X52DateFormat, _X52_MFD_LINE_SIZE
from gx52.repository.x52_repository import X52Repository

_LOG = logging.getLogger(__name__)


@singleton
class X52DriverInteractor:
    @inject
    def __init__(self, x52_repository: X52Repository, ) -> None:
        self._x52_repository = x52_repository

    def get_devices(self) -> Observable:
        _LOG.debug("X52DriverInteractor.get_devices()")
        return reactivex.defer(lambda _: reactivex.just(self._x52_repository.get_devices()))

    def set_mfd_mode_line(self,
                          driver: X52Driver,
                          mode: str) -> Observable:
        _LOG.debug("X52DriverInteractor.set_mfd_brightness()")
        return reactivex.defer(lambda _: reactivex.just(self._x52_repository.set_mfd_line1(driver, mode[:_X52_MFD_LINE_SIZE])))

    def set_mfd_button_line(self,
                            driver: X52Driver,
                            button: str) -> Observable:
        _LOG.debug("X52DriverInteractor.set_mfd_brightness()")
        return reactivex.defer(lambda _: reactivex.just(self._x52_repository.set_mfd_line2(driver, button[:_X52_MFD_LINE_SIZE])))

    def set_mfd_profile_name_line(self,
                                  driver: X52Driver,
                                  name: str,
                                  clear_mfd: bool = False) -> Observable:
        _LOG.debug("X52DriverInteractor.set_mfd_line3()")
        return reactivex.defer(
            lambda _: reactivex.just(self._x52_repository.set_mfd_line3(driver, name[:_X52_MFD_LINE_SIZE], clear_mfd)))

    def set_led_status(self,
                       driver: X52Driver,
                       led_status: Union[X52ColoredLedStatus, X52LedStatus],
                       attr_name: str) -> Observable:
        _LOG.debug("X52DriverInteractor.set_led_status()")
        return reactivex.defer(
            lambda _: reactivex.just(self._x52_repository.set_led_status(driver, led_status, attr_name)))

    def set_led_brightness(self,
                           driver: X52Driver,
                           brightness: int) -> Observable:
        _LOG.debug("X52DriverInteractor.set_led_brightness()")
        return reactivex.defer(lambda _: reactivex.just(self._x52_repository.set_led_brightness(driver, brightness)))

    def set_mfd_brightness(self,
                           driver: X52Driver,
                           brightness: int) -> Observable:
        _LOG.debug("X52DriverInteractor.set_mfd_brightness()")
        return reactivex.defer(lambda _: reactivex.just(self._x52_repository.set_mfd_brightness(driver, brightness)))

    def set_date_time(self,
                      driver: X52Driver,
                      use_local_time: bool,
                      use_24h: Tuple[bool, bool, bool],
                      clock2_offset: datetime.timedelta,
                      clock3_offset: datetime.timedelta,
                      date_format: X52DateFormat) -> Observable:
        _LOG.debug("X52DriverInteractor.set_date_time()")
        return reactivex.defer(lambda _: reactivex.just(self._x52_repository.set_date_time(
            driver,
            use_local_time,
            use_24h,
            clock2_offset,
            clock3_offset,
            date_format)))

    def get_evdev_events(self,
                         driver: X52Driver) -> Observable:
        _LOG.debug("X52DriverInteractor.get_evdev_events()")
        return self._x52_repository.get_evdev_events(driver)

    def set_send_ev_abs_events(self, should_send: bool) -> None:
        self._x52_repository.should_send_ev_abs_events = should_send
