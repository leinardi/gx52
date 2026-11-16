#!/usr/bin/env bash
#
# Copyright 2026 Roberto Leinardi.
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
# GNU General Public License for more details.
#
# You should have received a copy of the GNU General Public License
# along with this program.  If not, see <https://www.gnu.org/licenses/>.
#
# Captures the metainfo screenshots with the installed Devel Flatpak (`make screenshots` installs
# it first): the app's screenshot mode (src/screenshots/) runs on a private headless weston, in an
# environment built from scratch, so the host's desktop, locale, theme and scale never reach the
# shots. Every PNG the metainfo names must come out of the run.
#
# Usage: scripts/screenshots.sh [OUTPUT_DIR]   (default: data/screenshots)

set -euo pipefail

readonly APP_ID=com.leinardi.gx52.Devel
readonly METAINFO=data/com.leinardi.gx52.metainfo.xml.in.in
# How long weston gets to accept a client, polled every 100 ms.
readonly WESTON_TRIES=100
# The output is larger than the widest shot (src/screenshots/mod.rs, WIDE), at scale 1.
readonly WESTON_WIDTH=1600
readonly WESTON_HEIGHT=1200

die() {
    printf 'screenshots: %s\n' "$*" >&2
    exit 1
}

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
# A relative OUTPUT_DIR is the caller's; the default is the repository's.
out=$(realpath -m "${1:-$repo/data/screenshots}")
cd "$repo"

: "${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR must be set}"
[[ -d $XDG_RUNTIME_DIR ]] || die "XDG_RUNTIME_DIR $XDG_RUNTIME_DIR is not a directory"
command -v weston >/dev/null || die "weston is not installed"
command -v flatpak >/dev/null || die "flatpak is not installed"
command -v python3 >/dev/null || die "python3 is not installed"

mapfile -t shots < <(grep -oE 'data/screenshots/[a-z0-9-]+\.png' "$METAINFO" | sed 's|.*/||')
((${#shots[@]} > 0)) || die "$METAINFO names no screenshot"

# The one name of the compositor's socket: weston, the probe and the app all use it.
readonly SOCKET="gx52-shots-$$"
readonly SOCKET_PATH="$XDG_RUNTIME_DIR/$SOCKET"
[[ -e $SOCKET_PATH ]] && die "$SOCKET_PATH already exists"

# The environment the compositor and the app start from: what flatpak run needs, nothing else.
base_env=(
    "HOME=$HOME"
    "PATH=$PATH"
    "XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR"
)
if [[ -n ${DBUS_SESSION_BUS_ADDRESS:-} ]]; then
    base_env+=("DBUS_SESSION_BUS_ADDRESS=$DBUS_SESSION_BUS_ADDRESS")
fi

mkdir -p "$out"
capture=$(mktemp -d "$out/.capture.XXXXXX")
weston_log=$(mktemp)
weston_pid=
cleanup() {
    if [[ -n $weston_pid ]]; then
        kill "$weston_pid" 2>/dev/null || true
        wait "$weston_pid" 2>/dev/null || true
    fi
    rm -rf "$capture" "$weston_log"
}
trap cleanup EXIT

env -i "${base_env[@]}" weston --no-config --backend=headless --socket="$SOCKET" \
    --width="$WESTON_WIDTH" --height="$WESTON_HEIGHT" --scale=1 --idle-time=0 \
    >"$weston_log" 2>&1 &
weston_pid=$!

weston_failed() {
    printf 'screenshots: %s; weston log:\n' "$1" >&2
    cat "$weston_log" >&2
    exit 1
}

# Ready once weston is alive and a client connection to the socket succeeds.
ready=false
for ((try = 0; try < WESTON_TRIES; try++)); do
    kill -0 "$weston_pid" 2>/dev/null || weston_failed "weston exited"
    if python3 -I -c 'import socket, sys; socket.socket(socket.AF_UNIX).connect(sys.argv[1])' \
        "$SOCKET_PATH" 2>/dev/null; then
        ready=true
        break
    fi
    sleep 0.1
done
$ready || weston_failed "weston did not accept a client in $((WESTON_TRIES / 10)) s"

# Every variable the app reads is set here, over anything the sandbox would pass on.
env -i "${base_env[@]}" WAYLAND_DISPLAY="$SOCKET" \
    timeout --kill-after=10 180 flatpak run \
    --env=LC_ALL=C.UTF-8 --env=LANG=C.UTF-8 --env=LANGUAGE= \
    --env=GSETTINGS_BACKEND=memory --env=GDK_BACKEND=wayland --env=GSK_RENDERER=cairo \
    --env=GTK_THEME= --env=GDK_SCALE= --env=GDK_DPI_SCALE= --env=GTK_DEBUG= \
    --env=ADW_DEBUG_COLOR_SCHEME=prefer-light --env=ADW_DEBUG_HIGH_CONTRAST=0 \
    --env=ADW_DEBUG_ACCENT_COLOR=blue \
    --nosocket=x11 --nosocket=fallback-x11 --nodevice=all --filesystem="$capture" \
    "$APP_ID" --screenshots "$capture" ||
    die "the screenshot run failed"

for shot in "${shots[@]}"; do
    [[ -f $capture/$shot ]] || die "the run did not save $shot"
done
for shot in "${shots[@]}"; do
    mv -f "$capture/$shot" "$out/$shot"
done
printf 'screenshots: saved %s into %s\n' "${shots[*]}" "$out"
