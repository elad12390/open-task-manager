#!/usr/bin/env bash
# Install the .deb in a clean distribution container and check it works. Runs as root inside
# ci/test-deb.sh containers: /debs holds the package, /artifacts receives the screenshot.
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
DESKTOP=/usr/share/applications/io.github.elad12390.OpenTaskManager.desktop
ICON=/usr/share/icons/hicolor/512x512/apps/io.github.elad12390.OpenTaskManager.png

apt-get update -qq
apt-get install -y -qq /debs/open-task-manager_*.deb
apt-get install -y -qq --no-install-recommends desktop-file-utils xvfb xauth imagemagick mesa-vulkan-drivers >/dev/null

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
test -f /usr/share/metainfo/io.github.elad12390.OpenTaskManager.metainfo.xml

echo "== launch (must still be running after 12 s, screenshot at 8 s)"
export XDG_RUNTIME_DIR=/tmp/xdg-runtime
mkdir -p -m 700 "$XDG_RUNTIME_DIR"
set +e
xvfb-run -a sh -c '
  timeout 12 otm >/artifacts/otm.log 2>&1 &
  pid=$!
  sleep 8
  import -window root /artifacts/screenshot.png
  wait "$pid"
  exit $?'
rc=$?
set -e
echo "launch exit code: $rc (124 = still running at timeout)"
test "$rc" -eq 124
echo "selftest exit code: $selftest_rc"
test "$selftest_rc" -eq 0
