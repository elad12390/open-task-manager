#!/usr/bin/env bash
# Install the .deb in a clean distribution container and check it works. Runs as root inside
# ci/test-deb.sh containers: /debs holds the package, /artifacts receives the screenshot.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
DESKTOP=/usr/share/applications/io.github.elad12390.open_task_manager.OpenTaskManager.desktop
ICON=/usr/share/icons/hicolor/512x512/apps/io.github.elad12390.open_task_manager.OpenTaskManager.png

apt-get update -qq
apt-get install -y -qq /debs/open-task-manager_*.deb
apt-get install -y -qq --no-install-recommends desktop-file-utils xvfb xauth imagemagick mesa-vulkan-drivers \
  openbox >/dev/null

echo "== version"
otm --version
otm --version | grep -q "^otm [0-9]"

echo "== selftest"
set +e
otm --selftest
selftest_rc=$?
set -e

echo "== desktop entry"
desktop-file-validate "$DESKTOP"
grep -qx 'Exec=otm' "$DESKTOP"
test -f "$ICON"
test -f /usr/share/metainfo/io.github.elad12390.open_task_manager.OpenTaskManager.metainfo.xml

echo "== launch from the menu entry (must draw its window and still run after 15 s)"
export XDG_RUNTIME_DIR=/tmp/xdg-runtime
mkdir -p -m 700 "$XDG_RUNTIME_DIR"
# The command the app menu runs: the entry's Exec line without field codes.
launch="$(sed -n 's/^Exec=//p' "$DESKTOP" | sed 's/ %[fFuUick]//g')"
# GPUI's X11 backend draws only under a window manager, and its 32-bit window stays invisible without a
# compositor: Composite off forces a 24-bit window (as ci/capture-linux-frames.sh does).
Xvfb :7 -screen 0 1600x1000x24 -extension Composite >/dev/null 2>&1 &
xvfb=$!
sleep 2
DISPLAY=:7 openbox >/dev/null 2>&1 &
sleep 1
set +e
DISPLAY=:7 timeout 15 $launch >/artifacts/otm.log 2>&1 &
pid=$!
sleep 10
DISPLAY=:7 import -window root /artifacts/screenshot.png
wait "$pid"
rc=$?
set -e
kill "$xvfb" 2>/dev/null || true
echo "launch exit code: $rc (124 = still running at timeout)"
test "$rc" -eq 124
# Colours inside the window (centred, 1240x800), so the window manager's frame alone doesn't count.
colours="$(convert /artifacts/screenshot.png -crop 800x400+400+300 +repage -format '%k' info:)"
echo "window content colours: $colours"
test "$colours" -gt 500
echo "selftest exit code: $selftest_rc"
test "$selftest_rc" -eq 0
