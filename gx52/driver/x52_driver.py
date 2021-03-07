"""USB driver for third generation NZXT Kraken X and M liquid coolers.
"""
import datetime
import logging
from enum import Enum, unique, IntEnum
from typing import Any, List

import usb.util

from usb.core import Device

_LOG = logging.getLogger(__name__)

ID_VENDOR = 0x06a3
ID_PRODUCTS = [0x0762, 0x0255, 0x075c]

# X52 vendor API commands
# Vendor request - all commands must have this request ID
_X52_VENDOR_REQUEST = 0x91

# MFD Text commands
_X52_MFD_CLEAR_LINE = 0x08


# Brightness commands
@unique
class X52BrightnessCommand(Enum):
    MFD_BRIGHTNESS = 0xb1
    LED_BRIGHTNESS = 0xb2


X52_BRIGHTNESS_MIN = 0x00
X52_BRIGHTNESS_MAX = 0x80 / 4

# LED set commands
_X52_LED = 0xb8


# Time commands
@unique
class X52TimeCommand(Enum):
    TIME_CLOCK1 = 0xc0
    OFFS_CLOCK2 = 0xc1
    OFFS_CLOCK3 = 0xc2


# Date commands
@unique
class X52DateCommand(Enum):
    DDMM = 0xc4
    YEAR = 0xc8


@unique
class X52DateFormat(Enum):
    YYMMDD = 0
    DDMMYY = 1
    MMDDYY = 2


# Shift indicator on MFD
_X52_SHIFT_INDICATOR = 0xfd


@unique
class X52ShiftStatus(Enum):
    ON = 0x51
    OFF = 0x50


# Blink throttle & POV LED
_X52_BLINK_INDICATOR = 0xb4


@unique
class X52BlinkStatus(Enum):
    ON = 0x51
    OFF = 0x50


_X52_MFD_LINE_SIZE = 16

# Flag bits
_X52_FLAG_IS_PRO = 0

# Indicator bits for update mask
_X52_BIT_SHIFT = 0


@unique
class X52MfdLine(Enum):
    LINE1 = 0xd1
    LINE2 = 0xd2
    LINE3 = 0xd4


_WRITE_TIMEOUT = 5000


@unique
class X52DeviceType(Enum):
    X52_PRO = 'X52 Pro'
    X52 = 'X52'


class X52Device:
    def __init__(self, id_vendor: int,
                 id_product: int,
                 device_type: X52DeviceType,
                 description: str) -> None:
        self.id_vendor = id_vendor
        self.id_product = id_product
        self.device_type = device_type
        self.description = description


_SUPPORTED_DEVICES: List[X52Device] = [
    X52Device(ID_VENDOR, ID_PRODUCTS[0], X52DeviceType.X52_PRO, 'Saitek PLC Saitek X52 Pro Flight Control System'),
    X52Device(ID_VENDOR, ID_PRODUCTS[1], X52DeviceType.X52, 'Saitek PLC X52 Flight Controller'),
    X52Device(ID_VENDOR, ID_PRODUCTS[2], X52DeviceType.X52, 'Saitek PLC X52 Flight Controller')
]


@unique
class X52Led(IntEnum):
    X52_BIT_LED_FIRE = 1
    X52_BIT_LED_THROTTLE = 20


@unique
class X52LedRed(IntEnum):
    X52_BIT_LED_A_RED = 2
    X52_BIT_LED_B_RED = 4
    X52_BIT_LED_D_RED = 6
    X52_BIT_LED_E_RED = 8
    X52_BIT_LED_T1_RED = 10
    X52_BIT_LED_T2_RED = 12
    X52_BIT_LED_T3_RED = 14
    X52_BIT_LED_POV_RED = 16
    X52_BIT_LED_I_RED = 18


@unique
class X52LedGreen(IntEnum):
    X52_BIT_LED_A_GREEN = 3
    X52_BIT_LED_B_GREEN = 5
    X52_BIT_LED_D_GREEN = 7
    X52_BIT_LED_E_GREEN = 9
    X52_BIT_LED_T1_GREEN = 11
    X52_BIT_LED_T2_GREEN = 13
    X52_BIT_LED_T3_GREEN = 15
    X52_BIT_LED_POV_GREEN = 17
    X52_BIT_LED_I_GREEN = 19


@unique
class X52LedStatus(IntEnum):
    OFF = 0
    ON = 1


@unique
class X52ColoredLedStatus(IntEnum):
    OFF = 0
    GREEN = 1
    RED = 2
    AMBER = 3


class X52Driver:
    def __init__(self, usb_device: Device, x52_device: X52Device) -> None:
        self.usb_device = usb_device
        self.x52_device = x52_device

    @classmethod
    def find_supported_devices(cls) -> List['X52Driver']:
        """Find compatible devices and return corresponding driver instances.

        Returns a list of driver class instances.
        """
        devices: List['X52Driver'] = []
        for supported_device in _SUPPORTED_DEVICES:
            usb_devices = usb.core.find(
                idVendor=supported_device.id_vendor,
                idProduct=supported_device.id_product,
                find_all=True)
            for dev in usb_devices:
                devices.append(cls(dev, supported_device))
        return devices

    def set_led_a(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_A_RED, X52LedGreen.X52_BIT_LED_A_GREEN)

    def set_led_b(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_B_RED, X52LedGreen.X52_BIT_LED_B_GREEN)

    def set_led_d(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_D_RED, X52LedGreen.X52_BIT_LED_D_GREEN)

    def set_led_e(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_E_RED, X52LedGreen.X52_BIT_LED_E_GREEN)

    def set_led_t1_t2(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_T1_RED, X52LedGreen.X52_BIT_LED_T1_GREEN)

    def set_led_t3_t4(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_T2_RED, X52LedGreen.X52_BIT_LED_T2_GREEN)

    def set_led_t5_t6(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_T3_RED, X52LedGreen.X52_BIT_LED_T3_GREEN)

    def set_led_pov_2(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_POV_RED, X52LedGreen.X52_BIT_LED_POV_GREEN)

    def set_led_i(self, led: X52ColoredLedStatus) -> None:
        self._set_colored_led_status(led, X52LedRed.X52_BIT_LED_I_RED, X52LedGreen.X52_BIT_LED_I_GREEN)

    def set_led_fire(self, led_status: X52LedStatus) -> None:
        self._set_led_status(X52Led.X52_BIT_LED_FIRE.value, led_status)

    def set_led_throttle(self, led_status: X52LedStatus) -> None:
        self._set_led_status(X52Led.X52_BIT_LED_THROTTLE.value, led_status)

    def set_led_brightness(self, level: int) -> None:
        self._set_brightness(X52BrightnessCommand.LED_BRIGHTNESS, level)

    def set_mfd_brightness(self, level: int) -> None:
        self._set_brightness(X52BrightnessCommand.MFD_BRIGHTNESS, level)

    def set_shift_status(self, enabled: bool) -> None:
        self._vendor_command(_X52_SHIFT_INDICATOR,
                             X52ShiftStatus.ON.value if enabled else X52ShiftStatus.OFF.value)

    def set_blink_status(self, enabled: bool) -> None:
        self._vendor_command(_X52_BLINK_INDICATOR,
                             X52BlinkStatus.ON.value if enabled else X52BlinkStatus.OFF.value)

    def set_clock_1(self, time: datetime.time, use_24h: bool = True) -> None:
        value = (1 if use_24h else 0) << 15
        value += time.hour << 8
        value += time.minute
        self._vendor_command(X52TimeCommand.TIME_CLOCK1.value, value)

    def set_clock_2_offset(self, offset: datetime.timedelta, use_24h: bool = True) -> None:
        self._set_clock_offset(X52TimeCommand.OFFS_CLOCK2, int(offset.total_seconds() / 60), use_24h)

    def set_clock_3_offset(self, offset: datetime.timedelta, use_24h: bool = True) -> None:
        self._set_clock_offset(X52TimeCommand.OFFS_CLOCK3, int(offset.total_seconds() / 60), use_24h)

    def set_date(self, date: datetime.date, date_format: X52DateFormat = X52DateFormat.YYMMDD) -> None:
        year = int(str(date.year)[-2:])
        month = date.month
        day = date.day

        if date_format == X52DateFormat.YYMMDD:
            value1 = month << 8
            value1 += year
            value2 = day
        elif date_format == X52DateFormat.DDMMYY:
            value1 = day
            value1 += month << 8
            value2 = year
        elif date_format == X52DateFormat.MMDDYY:
            value1 = month
            value1 += day << 8
            value2 = year
        else:
            raise ValueError(f"Unsupported X52DateFormat: ${date_format.name}")
        self._vendor_command(X52DateCommand.DDMM.value, value1)
        self._vendor_command(X52DateCommand.YEAR.value, value2)

    def set_mfd_text(self, line: X52MfdLine, text: str) -> None:
        if len(text) > _X52_MFD_LINE_SIZE:
            raise ValueError(f"The text length must be less than 16: {len(text)}")
        text = f"{text:16s}"
        self._vendor_command(line.value | _X52_MFD_CLEAR_LINE, 0)
        for i in range(0, len(text), 2):
            value = int.from_bytes(text[i + 1].encode("ascii"), "big") << 8 \
                    | int.from_bytes(text[i].encode("ascii"), "big")
            self._vendor_command(line.value, value)

    def _vendor_command(self, index: int, value: int) -> Any:
        _LOG.debug(f'index = 0x{index:x} value = {value:016b}')
        return self.usb_device.ctrl_transfer(64, _X52_VENDOR_REQUEST, value, index, None, _WRITE_TIMEOUT)

    def _set_led_status(self, led: int, led_status: X52LedStatus) -> None:
        value = led << 8
        value += led_status.value
        self._vendor_command(_X52_LED, value)

    def _set_colored_led_status(self, led_status: X52ColoredLedStatus, red: X52LedRed, green: X52LedGreen) -> None:
        if led_status == X52ColoredLedStatus.RED:
            self._set_led_status(green.value, X52LedStatus.OFF)
            self._set_led_status(red.value, X52LedStatus.ON)
        elif led_status == X52ColoredLedStatus.GREEN:
            self._set_led_status(green.value, X52LedStatus.ON)
            self._set_led_status(red.value, X52LedStatus.OFF)
        elif led_status == X52ColoredLedStatus.AMBER:
            self._set_led_status(green.value, X52LedStatus.ON)
            self._set_led_status(red.value, X52LedStatus.ON)
        elif led_status == X52ColoredLedStatus.OFF:
            self._set_led_status(green.value, X52LedStatus.OFF)
            self._set_led_status(red.value, X52LedStatus.OFF)
        else:
            raise ValueError(f"Unsupported ColoredLedStatus: ${led_status.name}")

    def _set_brightness(self, command: X52BrightnessCommand, level: int) -> None:
        if level < X52_BRIGHTNESS_MIN or level > X52_BRIGHTNESS_MAX:
            raise ValueError(f"Level must be between {X52_BRIGHTNESS_MIN:d} and {X52_BRIGHTNESS_MAX:d}")
        self._vendor_command(command.value, level * 4)

    def _set_clock_offset(self, command: X52TimeCommand, offset_in_min: int, use_24h: bool = True) -> None:
        if offset_in_min < -1024 or offset_in_min > 1024:
            raise ValueError(f"Hours must be between -1024 and 1024")

        value = (1 if use_24h else 0) << 15
        if offset_in_min < 0:
            value += 1 << 10
            offset_in_min *= -1
        value += offset_in_min
        self._vendor_command(command.value, value)


@unique
class X52ProEvdevKeyMapping(IntEnum):
    TRIGGER = 288
    FIRE = 289
    FIRE_A = 290
    FIRE_B = 291
    FIRE_C = 292
    PINKIE = 293
    FIRE_D = 294
    FIRE_E = 295
    TOGGLE_1 = 296
    TOGGLE_2 = 297
    TOGGLE_3 = 298
    TOGGLE_4 = 299
    TOGGLE_5 = 300
    TOGGLE_6 = 301
    SECONDARY_TRIGGER = 302
    LEFT_MOUSE_BUTTON = 303
    SCROLL_DOWN = 704
    SCROLL_UP = 705
    SCROLL_CLICK = 706
    POV_2_UP = 707
    POV_2_RIGHT = 708
    POV_2_DOWN = 709
    POV_2_LEFT = 710
    THROTTLE_HAT_UP = 711
    THROTTLE_HAT_RIGHT = 712
    THROTTLE_HAT_DOWN = 713
    THROTTLE_HAT_LEFT = 714
    MODE_1 = 715
    MODE_2 = 716
    MODE_3 = 717
    FIRE_I = 718
    MFD_FUNCTION = 719
    MFD_START_STOP = 720
    MFD_RESET = 721
    MFD_PAGE_UP = 722
    MFD_PAGE_DOWN = 723
    MFD_UP = 724
    MFD_DOWN = 725
    MFD_SELECT = 726


@unique
class X52EvdevKeyMapping(IntEnum):
    TRIGGER = 288
    FIRE = 289
    FIRE_A = 290
    FIRE_B = 291
    FIRE_C = 292
    PINKIE = 293
    FIRE_D = 294
    FIRE_E = 295
    TOGGLE_1 = 296
    TOGGLE_2 = 297
    TOGGLE_3 = 298
    TOGGLE_4 = 299
    TOGGLE_5 = 300
    TOGGLE_6 = 301
    SECONDARY_TRIGGER = 302
    POV_2_UP = 303
    POV_2_RIGHT = 704
    POV_2_DOWN = 705
    POV_2_LEFT = 706
    THROTTLE_HAT_UP = 707
    THROTTLE_HAT_RIGHT = 708
    THROTTLE_HAT_DOWN = 709
    THROTTLE_HAT_LEFT = 710
    MODE_1 = 711
    MODE_2 = 712
    MODE_3 = 713
    FIRE_I = 717
    LEFT_MOUSE_BUTTON = 718
    SCROLL_CLICK = 719
    SCROLL_DOWN = 720
    SCROLL_UP = 721
    MFD_FUNCTION = 714
    MFD_START_STOP = 715
    MFD_RESET = 716
