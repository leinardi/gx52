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
from typing import Dict, Any

APP_PACKAGE_NAME = "gx52"
APP_NAME = "GX52"
APP_ID = "com.leinardi.gx52"
APP_VERSION = "0.7.0"
APP_ICON_NAME = APP_ID
APP_ICON_NAME_SYMBOLIC = APP_ID + "-symbolic"
APP_DB_NAME = APP_PACKAGE_NAME + ".db"
APP_MAIN_UI_NAME = "main.glade"
APP_PREFERENCES_UI_NAME = "preferences.glade"
APP_DESKTOP_ENTRY_NAME = APP_PACKAGE_NAME + ".desktop"
APP_DESCRIPTION = 'Provides control of LEDs and MFD for Logitech X52 and X52 Pro H.O.T.A.S.'
APP_SOURCE_URL = 'https://gitlab.com/leinardi/gx52'
APP_AUTHOR = 'Roberto Leinardi'
APP_AUTHOR_EMAIL = 'roberto@leinardi.com'

CLOCK_2_OFFSET_DEFAULT = -240
CLOCK_3_OFFSET_DEFAULT = 480

SETTINGS_DEFAULTS: Dict[str, Any] = {
    'settings_launch_on_login': False,
    'settings_load_last_profile': True,
    'settings_minimize_to_tray': True,
    'settings_refresh_interval': 3,
    'settings_show_app_indicator': True,
    'settings_app_indicator_show_gpu_temp': True,
}

DESKTOP_ENTRY: Dict[str, str] = {
    'Type': 'Application',
    'Encoding': 'UTF-8',
    'Name': APP_NAME,
    'Comment': APP_DESCRIPTION,
    'Terminal': 'false',
    'Categories': 'System;Settings;',
}
