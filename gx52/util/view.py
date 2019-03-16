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

from typing import Optional, Any, Dict

from gi.repository import GLib, Gtk, Gdk
from gi.repository import Gio


def build_glib_option(long_name: str,
                      short_name: Optional[str] = None,
                      flags: int = 0,
                      arg: int = GLib.OptionArg.NONE,
                      arg_data: Optional[object] = None,
                      description: Optional[str] = None,
                      arg_description: Optional[str] = None) -> GLib.OptionEntry:
    option = GLib.OptionEntry()
    option.long_name = long_name
    option.short_name = 0 if not short_name else ord(short_name[0])
    option.flags = flags
    option.description = description
    option.arg = arg
    option.arg_description = arg_description
    option.arg_data = arg_data
    return option


def hide_on_delete(widget: Gtk.Widget, *_: Any) -> Any:
    widget.hide()
    return widget.hide_on_delete()


def rgba_to_hex(color: Gdk.RGBA) -> str:
    """Return hexadecimal string for :class:`Gdk.RGBA` `color`."""
    return "#{0:02x}{1:02x}{2:02x}{3:02x}".format(int(color.red * 255),
                                                  int(color.green * 255),
                                                  int(color.blue * 255),
                                                  int(color.alpha * 255))


def get_default_application() -> Gtk.Application:
    return Gtk.Application.get_default()


def show_notification(summary: str, body: str, iconname: str) -> None:
    application = get_default_application()
    notification = Gio.Notification.new(title=summary)
    notification.set_body(body)
    iconname = Gio.ThemedIcon.new(iconname=iconname)
    notification.set_icon(iconname)
    application.send_notification(None, notification)


def open_uri(uri: str, parent: Gtk.Window = None, timestamp: int = Gdk.CURRENT_TIME) -> None:
    Gtk.show_uri_on_window(parent, uri, timestamp)
