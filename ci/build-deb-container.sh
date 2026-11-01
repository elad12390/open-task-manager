#!/usr/bin/env bash
# Build the release binary and a .deb for this machine's architecture. Runs inside
# ci/linux-build-2204.Dockerfile through ci/build-deb.sh; writes to $OUT_DIR (default ./dist).
set -euo pipefail

cd "$(dirname "$0")/.."
OUT_DIR="${OUT_DIR:-$PWD/dist}"
CARGO_DEB="$CARGO_HOME/registry/tools/bin/cargo-deb"

if [[ ! -x "$CARGO_DEB" ]]; then
  cargo install cargo-deb --locked --root "$CARGO_HOME/registry/tools"
fi

cargo build --release --locked
"$CARGO_DEB" --no-build
mkdir -p "$OUT_DIR"
cp "$CARGO_TARGET_DIR"/debian/open-task-manager_*.deb "$OUT_DIR/"
ls -l "$OUT_DIR"/open-task-manager_*.deb
