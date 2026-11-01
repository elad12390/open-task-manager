#!/usr/bin/env bash
# Build the .deb for this machine's architecture inside the Ubuntu 22.04 build image.
#   ci/build-deb.sh            # output: dist/open-task-manager_<version>-1_<arch>.deb
set -euo pipefail

cd "$(dirname "$0")/.."
IMAGE="otm-linux-build-2204"

docker build -f ci/linux-build-2204.Dockerfile -t "$IMAGE" .
docker run --rm --name otm-linux-deb-build-2204 -v "$PWD":/work/deb -w /work/deb \
  -v otm-cargo:/usr/local/cargo/registry -v otm-linux-target:/target \
  -e CARGO_TARGET_DIR=/target/deb-2204 "$IMAGE" bash ci/build-deb-container.sh
