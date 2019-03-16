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
import logging

from peewee import CharField, IntegerField, BooleanField, DateTimeField, SQL, SqliteDatabase
from playhouse.signals import Model
from playhouse.sqlite_ext import AutoIncrementField

from gx52.conf import CLOCK_2_OFFSET_DEFAULT, CLOCK_3_OFFSET_DEFAULT
from gx52.di import INJECTOR
from gx52.driver.x52_driver import X52LedStatus, X52_BRIGHTNESS_MAX, X52DateFormat
from gx52.model.enum_field import EnumField

_LOG = logging.getLogger(__name__)


class X52Profile(Model):
    id = AutoIncrementField()
    name = CharField()
    led_fire = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_a = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_b = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_d = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_e = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_t1_t2 = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_t3_t4 = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_t5_t6 = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_pov_2 = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_i = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_throttle = EnumField(default=X52LedStatus.ON, choices=X52LedStatus)
    led_brightness = IntegerField(default=X52_BRIGHTNESS_MAX)
    mfd_brightness = IntegerField(default=X52_BRIGHTNESS_MAX)
    clock_1_use_local_time = BooleanField(default=False)
    clock_1_use_24h = BooleanField(default=True)
    clock_2_offset = IntegerField(default=CLOCK_2_OFFSET_DEFAULT)
    clock_2_use_24h = BooleanField(default=True)
    clock_3_offset = IntegerField(default=CLOCK_3_OFFSET_DEFAULT)
    clock_3_use_24h = BooleanField(default=True)
    date_format = EnumField(default=X52DateFormat.YYMMDD, choices=X52DateFormat)
    can_be_removed = BooleanField(default=True)
    timestamp = DateTimeField(constraints=[SQL('DEFAULT CURRENT_TIMESTAMP')])

    @classmethod
    def get_empty_profile(cls) -> 'X52Profile':
        return cls(id=None,
                   name=None,
                   led_fire=None,
                   led_a=None,
                   led_b=None,
                   led_d=None,
                   led_e=None,
                   led_t1_t2=None,
                   led_t3_t4=None,
                   led_t5_t6=None,
                   led_pov_2=None,
                   led_i=None,
                   led_throttle=None,
                   led_brightness=None,
                   mfd_brightness=None,
                   clock_1_use_local_time=None,
                   clock_1_use_24h=None,
                   clock_2_offset=None,
                   clock_2_use_24h=None,
                   clock_3_offset=None,
                   clock_3_use_24h=None,
                   date_format=None,
                   can_be_removed=None)

    class Meta:
        legacy_table_names = False
        database = INJECTOR.get(SqliteDatabase)
