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
from gx52.driver.x52_driver import X52EvdevKeyMapping, X52ProEvdevKeyMapping


def get_button_name(key) -> str:
    if isinstance(key, X52ProEvdevKeyMapping):
        return get_x52_pro_button_name(key)
    elif isinstance(key, X52EvdevKeyMapping):
        return get_x52_button_name(key)
    else:
        raise ValueError(f"Wrong key mapping type")


def get_x52_pro_button_name(key: X52ProEvdevKeyMapping) -> str:
    if key == X52ProEvdevKeyMapping.TRIGGER:
        return "Trigger (B1)"
    if key == X52ProEvdevKeyMapping.FIRE:
        return "Fire (B2)"
    if key == X52ProEvdevKeyMapping.FIRE_A:
        return "Fire A (B3)"
    if key == X52ProEvdevKeyMapping.FIRE_B:
        return "Fire B (B4)"
    if key == X52ProEvdevKeyMapping.FIRE_C:
        return "Fire C (B5)"
    if key == X52ProEvdevKeyMapping.PINKIE:
        return "Pinkie (B6)"
    if key == X52ProEvdevKeyMapping.FIRE_D:
        return "Fire D (B7)"
    if key == X52ProEvdevKeyMapping.FIRE_E:
        return "Fire E (B8)"
    if key == X52ProEvdevKeyMapping.TOGGLE_1:
        return "Toggle 1 (B9)"
    if key == X52ProEvdevKeyMapping.TOGGLE_2:
        return "Toggle 2 (B10)"
    if key == X52ProEvdevKeyMapping.TOGGLE_3:
        return "Toggle 3 (B11)"
    if key == X52ProEvdevKeyMapping.TOGGLE_4:
        return "Toggle 4 (B12)"
    if key == X52ProEvdevKeyMapping.TOGGLE_5:
        return "Toggle 5 (B13)"
    if key == X52ProEvdevKeyMapping.TOGGLE_6:
        return "Toggle 6 (B14)"
    if key == X52ProEvdevKeyMapping.SECONDARY_TRIGGER:
        return "Trigger 2 (B15)"
    if key == X52ProEvdevKeyMapping.LEFT_MOUSE_BUTTON:
        return "Button 16"
    if key == X52ProEvdevKeyMapping.SCROLL_DOWN:
        return "Scroll D (B17)"
    if key == X52ProEvdevKeyMapping.SCROLL_UP:
        return "Scroll U (B18)"
    if key == X52ProEvdevKeyMapping.SCROLL_CLICK:
        return "Scroll Clk (B19)"
    if key == X52ProEvdevKeyMapping.POV_2_UP:
        return "POV 2 U (B20)"
    if key == X52ProEvdevKeyMapping.POV_2_RIGHT:
        return "POV 2 R (B21)"
    if key == X52ProEvdevKeyMapping.POV_2_DOWN:
        return "POV 2 D (B22)"
    if key == X52ProEvdevKeyMapping.POV_2_LEFT:
        return "POV 2 L (B23)"
    if key == X52ProEvdevKeyMapping.THROTTLE_HAT_UP:
        return "Thr. hat U (B24)"
    if key == X52ProEvdevKeyMapping.THROTTLE_HAT_RIGHT:
        return "Thr. hat R (B25)"
    if key == X52ProEvdevKeyMapping.THROTTLE_HAT_DOWN:
        return "Thr. hat D (B26)"
    if key == X52ProEvdevKeyMapping.THROTTLE_HAT_LEFT:
        return "Thr. hat L (B27)"
    if key == X52ProEvdevKeyMapping.MODE_1:
        return "Mode 1 (B28)"
    if key == X52ProEvdevKeyMapping.MODE_2:
        return "Mode 2 (B29)"
    if key == X52ProEvdevKeyMapping.MODE_3:
        return "Mode 3 (B30)"
    if key == X52ProEvdevKeyMapping.FIRE_I:
        return "Fire i (B31)"
    if key == X52ProEvdevKeyMapping.BUTTON_32:
        return "Button 32"
    if key == X52ProEvdevKeyMapping.BUTTON_33:
        return "Button 33"
    if key == X52ProEvdevKeyMapping.BUTTON_34:
        return "Button 34"
    if key == X52ProEvdevKeyMapping.BUTTON_35:
        return "Button 35"
    if key == X52ProEvdevKeyMapping.BUTTON_36:
        return "Button 36"
    if key == X52ProEvdevKeyMapping.BUTTON_37:
        return "Button 37"
    if key == X52ProEvdevKeyMapping.BUTTON_38:
        return "Button 38"
    if key == X52ProEvdevKeyMapping.BUTTON_39:
        return "Button 39"
    raise ValueError(f"Unknown key {key}")


def get_x52_button_name(key: X52EvdevKeyMapping) -> str:
    if key == X52EvdevKeyMapping.TRIGGER:
        return "Trigger (B1)"
    if key == X52EvdevKeyMapping.FIRE:
        return "Fire (B2)"
    if key == X52EvdevKeyMapping.FIRE_A:
        return "Fire A (B3)"
    if key == X52EvdevKeyMapping.FIRE_B:
        return "Fire B (B4)"
    if key == X52EvdevKeyMapping.FIRE_C:
        return "Fire C (B5)"
    if key == X52EvdevKeyMapping.PINKIE:
        return "Pinkie (B6)"
    if key == X52EvdevKeyMapping.FIRE_D:
        return "Fire D (B7)"
    if key == X52EvdevKeyMapping.FIRE_E:
        return "Fire E (B8)"
    if key == X52EvdevKeyMapping.TOGGLE_1:
        return "Toggle 1 (B9)"
    if key == X52EvdevKeyMapping.TOGGLE_2:
        return "Toggle 2 (B10)"
    if key == X52EvdevKeyMapping.TOGGLE_3:
        return "Toggle 3 (B11)"
    if key == X52EvdevKeyMapping.TOGGLE_4:
        return "Toggle 4 (B12)"
    if key == X52EvdevKeyMapping.TOGGLE_5:
        return "Toggle 5 (B13)"
    if key == X52EvdevKeyMapping.TOGGLE_6:
        return "Toggle 6 (B14)"
    if key == X52EvdevKeyMapping.SECONDARY_TRIGGER:
        return "Trigger 2 (B15)"
    if key == X52EvdevKeyMapping.POV_2_UP:
        return "POV 2 U (B16)"
    if key == X52EvdevKeyMapping.POV_2_RIGHT:
        return "POV 2 R (B17)"
    if key == X52EvdevKeyMapping.POV_2_DOWN:
        return "POV 2 D (B18)"
    if key == X52EvdevKeyMapping.POV_2_LEFT:
        return "POV 2 L (B19)"
    if key == X52EvdevKeyMapping.THROTTLE_HAT_UP:
        return "Thr. hat U (B20)"
    if key == X52EvdevKeyMapping.THROTTLE_HAT_RIGHT:
        return "Thr. hat R (B21)"
    if key == X52EvdevKeyMapping.THROTTLE_HAT_DOWN:
        return "Thr. hat D (B22)"
    if key == X52EvdevKeyMapping.THROTTLE_HAT_LEFT:
        return "Thr. hat L (B23)"
    if key == X52EvdevKeyMapping.MODE_1:
        return "Mode 1 (B24)"
    if key == X52EvdevKeyMapping.MODE_2:
        return "Mode 2 (B25)"
    if key == X52EvdevKeyMapping.MODE_3:
        return "Mode 3 (B26)"
    if key == X52EvdevKeyMapping.FUNCTION:
        return "Function (B27)"
    if key == X52EvdevKeyMapping.START_STOP:
        return "Start/Stop (B28)"
    if key == X52EvdevKeyMapping.BUTTON_34:
        return "Reset (B29)"
    if key == X52EvdevKeyMapping.FIRE_I:
        return "Fire i (B30)"
    if key == X52EvdevKeyMapping.LEFT_MOUSE_BUTTON:
        return "Button 31"
    if key == X52EvdevKeyMapping.SCROLL_CLICK:
        return "Scroll Clk (B32)"
    if key == X52EvdevKeyMapping.SCROLL_DOWN:
        return "Scroll D (B33)"
    if key == X52EvdevKeyMapping.SCROLL_UP:
        return "Scroll U (B34)"
    raise ValueError(f"Unknown key {key}")
