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

import logging
from enum import Enum
from gettext import gettext as _
from typing import Any, Optional, List

from gi.repository import Gtk, Gio, GLib
from injector import inject, singleton

from gx52.conf import APP_NAME, APP_ID, APP_VERSION, APP_ICON_NAME
from gx52.di import MainBuilder
from gx52.interactor.udev_interactor import UdevInteractor
from gx52.model import load_profile_db_default_data
from gx52.model.x52_profile import X52Profile
from gx52.presenter.main_presenter import MainPresenter
from gx52.util.deployment import is_flatpak
from gx52.util.desktop_entry import set_autostart_entry
from gx52.util.log import LOG_DEBUG_FORMAT
from gx52.util.view import build_glib_option
from gx52.view.main_view import MainView

_LOG = logging.getLogger(__name__)


@singleton
class Application(Gtk.Application):
    @inject
    def __init__(self,
                 view: MainView,
                 presenter: MainPresenter,
                 builder: MainBuilder,
                 udev_interactor: UdevInteractor,
                 *args: Any,
                 **kwargs: Any) -> None:
        _LOG.debug("init Application")
        GLib.set_application_name(_(APP_NAME))
        super().__init__(*args, application_id=APP_ID,
                         flags=Gio.ApplicationFlags.HANDLES_COMMAND_LINE,
                         **kwargs)
        if X52Profile.select().count() == 0:
            load_profile_db_default_data()

        self.add_main_option_entries(self._get_main_option_entries())
        self._view = view
        self._presenter = presenter
        self._udev_interactor = udev_interactor
        self._window: Optional[Gtk.ApplicationWindow] = None
        self._builder: Gtk.Builder = builder
        self._start_hidden: bool = False

    def do_activate(self) -> None:
        if not self._window:
            self._builder.connect_signals(self._presenter)
            self._window: Gtk.ApplicationWindow = self._builder.get_object("application_window")
            self._window.set_icon_name(APP_ICON_NAME)
            self._window.set_application(self)
            self._window.show_all()
            self._view.show()
        self._window.present()
        if self._start_hidden:
            self._window.hide()
            self._start_hidden = False

    def do_startup(self) -> None:
        Gtk.Application.do_startup(self)

    def do_command_line(self, command_line: Gio.ApplicationCommandLine) -> int:

        start_app = True
        options = command_line.get_options_dict()
        # convert GVariantDict -> GVariant -> dict
        options = options.end().unpack()

        exit_value = 0
        if _Options.DEBUG.value in options:
            logging.getLogger().setLevel(logging.DEBUG)
            for handler in logging.getLogger().handlers:
                handler.formatter = logging.Formatter(LOG_DEBUG_FORMAT)
            _LOG.debug(f"Option {_Options.DEBUG.value} selected")

        if _Options.VERSION.value in options:
            _LOG.debug(f"Option {_Options.VERSION.value} selected")
            print(APP_VERSION)
            start_app = False

        if _Options.HIDE_WINDOW.value in options:
            _LOG.debug(f"Option {_Options.HIDE_WINDOW.value} selected")
            self._start_hidden = True

        if _Options.AUTOSTART_ON.value in options:
            _LOG.debug(f"Option {_Options.AUTOSTART_ON.value} selected")
            set_autostart_entry(True)
            start_app = False

        if _Options.AUTOSTART_OFF.value in options:
            _LOG.debug(f"Option {_Options.AUTOSTART_OFF.value} selected")
            set_autostart_entry(True)
            start_app = False

        if _Options.ADD_UDEV_RULE.value in options:
            _LOG.debug("Option %s selected", _Options.ADD_UDEV_RULE.value)
            exit_value += self._udev_interactor.add_udev_rule()
            start_app = False

        if _Options.REMOVE_UDEV_RULE.value in options:
            _LOG.debug("Option %s selected", _Options.REMOVE_UDEV_RULE.value)
            exit_value += self._udev_interactor.remove_udev_rule()
            start_app = False

        if start_app:
            self.activate()
        return exit_value

    @staticmethod
    def _get_main_option_entries() -> List[GLib.OptionEntry]:
        options = [
            build_glib_option(_Options.DEBUG.value,
                              description="Show debug messages"),
            build_glib_option(_Options.VERSION.value,
                              short_name='v',
                              description="Show the app version"),
            build_glib_option(_Options.HIDE_WINDOW.value,
                              description="Start with the main window hidden"),
            build_glib_option(_Options.ADD_UDEV_RULE.value,
                              description="Add udev rule to allow execution without root permission"),
            build_glib_option(_Options.REMOVE_UDEV_RULE.value,
                              description="Remove udev rule that allow execution without root permission"),
        ]
        if not is_flatpak():
            options.append(build_glib_option(_Options.AUTOSTART_ON.value,
                                             description="Enable automatic start of the app on login"))
            options.append(build_glib_option(_Options.AUTOSTART_OFF.value,
                                             description="Disable automatic start of the app on login"))
        return options


class _Options(Enum):
    VERSION = 'version'
    DEBUG = 'debug'
    HIDE_WINDOW = 'hide-window'
    AUTOSTART_ON = 'autostart-on'
    AUTOSTART_OFF = 'autostart-off'
    ADD_UDEV_RULE = 'add-udev-rule'
    REMOVE_UDEV_RULE = 'remove-udev-rule'
