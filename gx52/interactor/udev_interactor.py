import logging
from typing import Optional, Callable

from injector import singleton, inject
from pyudev import Context, Monitor, Device

from gx52.driver.x52_driver import ID_VENDOR, ID_PRODUCTS
from gx52.interactor import _run_and_get_stdout
from gx52.util.glib import MonitorObserver

_UDEV_RULE = "\n".join('SUBSYSTEMS=="usb", ATTRS{{idVendor}}=="{:04x}", ATTRS{{idProduct}}=="{:04x}", MODE="0666"'
                       .format(ID_VENDOR, s) for s in ID_PRODUCTS)
_UDEV_RULE_FILE_PATH = '/lib/udev/rules.d/60-gx52.rules'
_UDEV_RULE_RELOAD_COMMANDS = 'udevadm control --reload-rules ' \
                             '&& udevadm trigger --subsystem-match=usb --attr-match=idVendor=06a3 --action=add'

_LOG = logging.getLogger(__name__)

@singleton
class UdevInteractor:
    @inject
    def __init__(self) -> None:
        self._callback: Optional[Callable] = None

    def monitor_device_events(self, callback: Callable) -> None:
        self._callback = callback
        context = Context()
        monitor = Monitor.from_netlink(context)
        monitor.filter_by(subsystem='input')
        observer = MonitorObserver(monitor)
        observer.connect('device-event', self.device_event)
        monitor.start()

    def device_event(self, observer: MonitorObserver, device: Device) -> None:
        if device.device_node is None and (ID_VENDOR == int(device.get("ID_VENDOR_ID"), 16)):
            assert self._callback is not None
            self._callback()
            _LOG.debug(f'event {device.action} on device {device}')

    @staticmethod
    def add_udev_rule() -> int:
        cmd = ['pkexec',
               'sh',
               '-c',
               f'echo \'{_UDEV_RULE}\' > {_UDEV_RULE_FILE_PATH} && {_UDEV_RULE_RELOAD_COMMANDS}']
        result = _run_and_get_stdout(cmd)
        if result[0] != 0:
            _LOG.warning(f"Error while creating rule file. Exit code: {result[0]}. {result[1]}")
        return result[0]

    @staticmethod
    def remove_udev_rule() -> int:
        cmd = ['pkexec',
               'sh',
               '-c',
               f'rm {_UDEV_RULE_FILE_PATH} && {_UDEV_RULE_RELOAD_COMMANDS}']
        result = _run_and_get_stdout(cmd)
        if result[0] != 0:
            _LOG.warning(f"Error while removing rule file. Exit code: {result[0]}. {result[1]}")
        return result[0]