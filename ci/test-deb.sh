#!/usr/bin/env bash
# Install the .deb in clean Ubuntu 22.04, Ubuntu 24.04 and Debian 12 containers (native arch).
#   ci/test-deb.sh [dir-with-deb]     # default: dist; screenshots land in dist/deb-test/<distro>/
set -uo pipefail

cd "$(dirname "$0")/.."
DEB_DIR="$(cd "${1:-dist}" && pwd)"
ls "$DEB_DIR"/open-task-manager_*.deb >/dev/null
ARTIFACTS="$PWD/dist/deb-test"
rm -rf "$ARTIFACTS"

failed=0
for image in ubuntu:22.04 ubuntu:24.04 debian:12; do
  name="otm-deb-test-$(echo "$image" | tr ':.' '--')"
  out="$ARTIFACTS/$(echo "$image" | tr ':.' '--')"
  mkdir -p "$out"
  echo "### $image"
  if docker run --rm --name "$name" -v "$DEB_DIR":/debs:ro -v "$out":/artifacts \
    -v "$PWD/ci/install-check.sh":/install-check.sh:ro "$image" bash /install-check.sh; then
    echo "### $image: PASS"
  else
    echo "### $image: FAIL"
    failed=1
  fi
done
exit "$failed"
