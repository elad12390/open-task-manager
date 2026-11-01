#!/usr/bin/env bash
# Install a .flatpak bundle in a clean Fedora container (flathub remote for the runtime) and run the
# app's selftest inside the sandbox.
#   ci/test-flatpak-install.sh <bundle.flatpak>
set -euo pipefail

BUNDLE="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
APP_ID="io.github.elad12390.open_task_manager"
cd "$(dirname "$0")/.."

docker run --rm --privileged --name "otm-flatpak-test-$$" \
  -v "$BUNDLE":/bundle/app.flatpak:ro \
  fedora:latest bash -c "
    set -euo pipefail
    dnf install -y -q flatpak
    flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
    flatpak install --user -y --noninteractive --bundle /bundle/app.flatpak
    flatpak run --user --command=otm $APP_ID --selftest
    flatpak run --user --command=otm $APP_ID --version
  "
