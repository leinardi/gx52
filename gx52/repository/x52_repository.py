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
import threading
from typing import List, Union, Tuple, Optional

import evdev
import reactivex
from evdev import ecodes, InputDevice
from injector import singleton, inject
from reactivex import Observable, Observer
from reactivex.scheduler.scheduler import Scheduler

from gx52.driver.x52_driver import X52Driver, X52ColoredLedStatus, X52LedStatus, X52DateFormat, X52MfdLine
from gx52.util.concurrency import synchronized_with_attr

_LOG = logging.getLogger(__name__)

_EMPTY_MFD_LINE = ""


@singleton
class X52Repository:
    @inject
    def __init__(self) -> None:
        self.should_send_ev_abs_events = False
        self._lock = threading.RLock()
        self._should_monitor_evdev_events = False

    @synchronized_with_attr("_lock")
    def get_devices(self) -> List[X52Driver]:
        return X52Driver.find_supported_devices()

    def cleanup(self) -> None:
        _LOG.debug("X52Repository cleanup")
        if self._should_monitor_evdev_events:
            self._should_monitor_evdev_events = False

    @synchronized_with_attr("_lock")
    def set_mfd_line1(self, driver: X52Driver, name: str, clear_mfd: bool = False) -> None:
        self.set_mfd_line(X52MfdLine.LINE1, driver, name, clear_mfd)

    @synchronized_with_attr("_lock")
    def set_mfd_line2(self, driver: X52Driver, name: str, clear_mfd: bool = False) -> None:
        self.set_mfd_line(X52MfdLine.LINE2, driver, name, clear_mfd)

    @synchronized_with_attr("_lock")
    def set_mfd_line3(self, driver: X52Driver, name: str, clear_mfd: bool = False) -> None:
        self.set_mfd_line(X52MfdLine.LINE3, driver, name, clear_mfd)

    @synchronized_with_attr("_lock")
    def set_mfd_line(self, mfd_line: X52MfdLine, driver: X52Driver, name: str, clear_mfd: bool) -> None:
        if clear_mfd:
            driver.set_mfd_text(X52MfdLine.LINE1, _EMPTY_MFD_LINE)
            driver.set_mfd_text(X52MfdLine.LINE2, _EMPTY_MFD_LINE)
            driver.set_mfd_text(X52MfdLine.LINE3, _EMPTY_MFD_LINE)
        driver.set_mfd_text(mfd_line, name)

    @synchronized_with_attr("_lock")
    def set_led_status(self,
                       driver: X52Driver,
                       led_status: Union[X52ColoredLedStatus, X52LedStatus],
                       attr_name: str) -> None:
        getattr(driver, f"set_{attr_name}")(led_status)

    @synchronized_with_attr("_lock")
    def set_led_brightness(self, driver: X52Driver, brightness: int) -> None:
        driver.set_led_brightness(brightness)

    @synchronized_with_attr("_lock")
    def set_mfd_brightness(self, driver: X52Driver, brightness: int) -> None:
        driver.set_mfd_brightness(brightness)

    @synchronized_with_attr("_lock")
    def set_date_time(self, driver: X52Driver,
                      use_local_time: bool,
                      use_24h: Tuple[bool, bool, bool],
                      clock2_offset: datetime.timedelta,
                      clock3_offset: datetime.timedelta,
                      date_format: X52DateFormat) -> None:
        now = datetime.datetime.utcnow()
        if use_local_time:
            tmp_now = datetime.datetime.now()
            offset_from_utc = datetime.datetime.now() - now
            offset_from_utc = offset_from_utc - datetime.timedelta(microseconds=offset_from_utc.microseconds)
            clock2_offset -= offset_from_utc
            clock3_offset -= offset_from_utc
            now = tmp_now
        driver.set_date(now.date(), date_format)
        driver.set_clock_1(now.time(), use_24h[0])
        driver.set_clock_2_offset(clock2_offset, use_24h[1])
        driver.set_clock_3_offset(clock3_offset, use_24h[2])

    @synchronized_with_attr("_lock")
    def get_evdev_events(self, driver: X52Driver) -> Observable:
        devices = [InputDevice(path) for path in evdev.list_devices()]

        device: Optional[InputDevice] = None
        for d in devices:
            if d.info.product == driver.usb_device.idProduct and d.info.vendor == driver.usb_device.idVendor:
                device = d
                break

        def observe(observer: Observer, _: Optional[Scheduler]) -> None:
            assert device is not None
            self._should_monitor_evdev_events = True
            for event in device.read_loop():
                if not self._should_monitor_evdev_events:
                    break
                if event.type == ecodes.EV_KEY or \
                        (event.type == ecodes.EV_ABS and self.should_send_ev_abs_events):
                    observer.on_next(event)
            device.close()
            observer.on_completed()

        return reactivex.create(observe)
