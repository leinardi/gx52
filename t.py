#!/usr/bin/env python3
import multiprocessing
from datetime import datetime
from enum import Enum
from time import sleep

import evdev as evdev
import rx
from evdev import ecodes, categorize
from rx import operators
from rx.scheduler import ThreadPoolScheduler

from gx52.driver.x52_driver import X52Driver, X52ColoredLedStatus, X52LedStatus, X52DateFormat


# class OverclockProfileType(Enum):
#     DEFAULT = 'default'
#     OFFSET = 'offset'
#
#
# def push_five_strings(s: str):
#     def observe_poba(observer, scheduler):
#         observer.on_next("Alpha" + s)
#         sleep(1)
#         observer.on_next("Beta")
#         observer.on_next("Gamma")
#         observer.on_next("Delta")
#         observer.on_next("Epsilon")
#         observer.on_completed()
#
#     return rx.create(observe_poba)
#
#
# source = push_five_strings("pobaaa")
#
# print("aa")
# source.pipe(
#     operators.subscribe_on(ThreadPoolScheduler(multiprocessing.cpu_count())),
# ).subscribe(
#     on_next=lambda i: print("Received {0}".format(i)),
#     on_error=lambda e: print("Error Occurred: {0}".format(e)),
#     on_completed=lambda: print("Done!"),
# )
# print("bb")
# sleep(2)

if __name__ == "__main__":
    devices = X52Driver.find_supported_devices()
    for device in devices:
        # device.set_mfd_text(X52MfdLine.LINE2, "ciao e' un piace")

        device.set_led_a(X52LedStatus.ON)
        device.set_led_b(X52LedStatus.OFF)
        device.set_led_d(X52LedStatus.ON)
        device.set_led_e(X52LedStatus.OFF)
    #     device.set_led_t1_t2(X52ColoredLedStatus.RED)
    #     device.set_led_t3_t4(X52ColoredLedStatus.RED)
    #     device.set_led_t5_t6(X52ColoredLedStatus.RED)
    #     device.set_led_pov_2(X52ColoredLedStatus.RED)
    #     device.set_led_i(X52ColoredLedStatus.RED)
    #
    #     device.set_led_fire(X52LedStatus.OFF)
    #     device.set_led_throttle(X52LedStatus.OFF)
    #
    #     sleep(2)
    #
    #     device.set_led_a(X52ColoredLedStatus.AMBER)
    #     device.set_led_b(X52ColoredLedStatus.AMBER)
    #     device.set_led_d(X52ColoredLedStatus.AMBER)
    #     device.set_led_e(X52ColoredLedStatus.AMBER)
    #     device.set_led_t1_t2(X52ColoredLedStatus.AMBER)
    #     device.set_led_t3_t4(X52ColoredLedStatus.AMBER)
    #     device.set_led_t5_t6(X52ColoredLedStatus.AMBER)
    #     device.set_led_pov_2(X52ColoredLedStatus.AMBER)
    #     device.set_led_i(X52ColoredLedStatus.AMBER)
    #
    #     sleep(2)
    #
    #     device.set_led_brightness(2)
    #     device.set_mfd_brightness(0)
    #
    #     device.set_led_a(X52ColoredLedStatus.GREEN)
    #     device.set_led_b(X52ColoredLedStatus.GREEN)
    #     device.set_led_d(X52ColoredLedStatus.GREEN)
    #     device.set_led_e(X52ColoredLedStatus.AMBER)
    #     device.set_led_t1_t2(X52ColoredLedStatus.GREEN)
    #     device.set_led_t3_t4(X52ColoredLedStatus.GREEN)
    #     device.set_led_t5_t6(X52ColoredLedStatus.GREEN)
    #     device.set_led_pov_2(X52ColoredLedStatus.GREEN)
    #     device.set_led_i(X52ColoredLedStatus.RED)
    #     device.set_led_fire(X52LedStatus.ON)
    #     device.set_led_throttle(X52LedStatus.ON)
    #
    #     device.set_clock_1(datetime.today().time(), True)
    #     device.set_clock_2_offset(120, False)
    #     device.set_clock_3_offset(-120, True)
    #
    #     device.set_date(datetime.today().date(), X52DateFormat.YYMMDD)

    # devices = [evdev.InputDevice(path) for path in evdev.list_devices()]
    # device = devices[0]
    # print(device.capabilities(verbose=True))
    # print(device.leds(verbose=True))
    # print(device.active_keys(verbose=True))
    # for event in device.read_loop():
    #     if event.type == ecodes.EV_REL:
    #         print(event)
    #     elif event.type == ecodes.EV_ABS:
    #         absevent = categorize(event)
    #         print(ecodes.bytype[absevent.event.type][absevent.event.code])
