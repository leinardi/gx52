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
from gx52.model.x52_pro_profile import X52ProProfile
from gx52.model.x52_profile import X52Profile


def load_profile_db_default_data() -> None:
    X52ProProfile.create(name="Default Profile", can_be_removed=False)
    X52Profile.create(name="Default Profile", can_be_removed=False)
