#!/usr/bin/env python3

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
import signal
import locale
import gettext
import logging
import sys
from types import TracebackType
from typing import Type
from os.path import abspath, join, dirname
from peewee import SqliteDatabase
from rx.disposable import CompositeDisposable
from gi.repository import GLib
from gx52.conf import APP_PACKAGE_NAME
from gx52.model.x52_profile import X52Profile
from gx52.model.x52_pro_profile import X52ProProfile
from gx52.model.current_profile import CurrentProfile
from gx52.model.setting import Setting
from gx52.repository.x52_repository import X52Repository
from gx52.util.log import set_log_level
from gx52.di import INJECTOR
from gx52.app import Application

WHERE_AM_I = abspath(dirname(__file__))
LOCALE_DIR = join(WHERE_AM_I, 'mo')

set_log_level(logging.INFO)

_LOG = logging.getLogger(__name__)

# POSIX locale settings
locale.setlocale(locale.LC_ALL, locale.getlocale())
locale.bindtextdomain(APP_PACKAGE_NAME, LOCALE_DIR)

gettext.bindtextdomain(APP_PACKAGE_NAME, LOCALE_DIR)
gettext.textdomain(APP_PACKAGE_NAME)


def _cleanup() -> None:
    try:
        _LOG.debug("cleanup")
        INJECTOR.get(X52Repository).cleanup()
        composite_disposable = INJECTOR.get(CompositeDisposable)
        composite_disposable.dispose()
        database = INJECTOR.get(SqliteDatabase)
        database.close()
        # futures.thread._threads_queues.clear()
    except:
        _LOG.exception("Error during cleanup!")


def handle_exception(type_: Type[BaseException], value: BaseException, traceback: TracebackType) -> None:
    if issubclass(type_, KeyboardInterrupt):
        sys.__excepthook__(type_, value, traceback)
        return

    _LOG.critical("Uncaught exception", exc_info=(type_, value, traceback))
    _cleanup()
    sys.exit(1)


sys.excepthook = handle_exception


def _init_database() -> None:
    database = INJECTOR.get(SqliteDatabase)
    database.create_tables([
        X52Profile,
        X52ProProfile,
        CurrentProfile,
        Setting
    ])


def main() -> int:
    _LOG.debug("main")
    _init_database()
    application: Application = INJECTOR.get(Application)
    GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGINT, application.quit)
    exit_status = application.run(sys.argv)
    _cleanup()
    return sys.exit(exit_status)


if __name__ == "__main__":
    main()
