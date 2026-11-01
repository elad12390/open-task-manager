#!/usr/bin/env bash
# Builds the Flatpak bundle, lints the manifest, the repo and the MetaInfo file, installs the bundle and
# runs its selftest. Runs in a privileged flatpak-builder container on the project's Colima profile `otm`:
#   packaging/flatpak/build.sh
# Runtimes, the build and the OSTree repo live in the named volume otm-flatpak (reused between runs);
# the bundle lands in dist/ in this checkout. Set OTM_FLATPAK_IMAGE to use another builder image.
set -euo pipefail

app_id=io.github.elad12390.open_task_manager.OpenTaskManager
lane=/work/linux-flatpak
manifest=$lane/packaging/flatpak/$app_id.yml
image=${OTM_FLATPAK_IMAGE:-ghcr.io/flathub-infra/flatpak-github-actions:freedesktop-26.08}

if [[ "${1:-}" != --in-container ]]; then
  root="$(cd "$(dirname "$0")/../.." && pwd)"
  mkdir -p "$root/dist"
  export DOCKER_HOST="unix://$HOME/.colima/otm/docker.sock"
  exec docker run --rm --privileged --name otm-flatpak-build-1 \
    -v "$root":$lane:ro \
    -v otm-flatpak:/state \
    -v "$root/dist":/out \
    -e OTM_FLATPAK_IMAGE="$image" \
    "$image" bash $lane/packaging/flatpak/build.sh --in-container
fi

# A user installation on the volume: a system installation on the container's overlay filesystem fails
# with "Invalid cross-device link" while updating runtimes.
export XDG_DATA_HOME=/state/data

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo

flatpak-builder --user --force-clean --disable-rofiles-fuse --install-deps-from=flathub \
  --state-dir=/state/state --repo=/state/repo /state/build "$manifest"

flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
  /state/repo "/out/$app_id.flatpak" "$app_id"

lint() {
  if command -v flatpak-builder-lint >/dev/null; then
    flatpak-builder-lint "$@"
  else
    flatpak install --user -y flathub org.flatpak.Builder
    flatpak run --command=flatpak-builder-lint org.flatpak.Builder "$@"
  fi
}

lint manifest "$manifest"
lint appstream "$lane/packaging/linux/$app_id.metainfo.xml"
(cd /state && lint repo repo)

flatpak install --user -y --bundle "/out/$app_id.flatpak"

dbus-run-session -- flatpak run --command=otm "$app_id" --selftest
