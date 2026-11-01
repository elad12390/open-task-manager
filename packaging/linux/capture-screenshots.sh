#!/usr/bin/env bash
# Captures the MetaInfo screenshots from demo mode (a fictional computer) under a virtual X server, so
# nothing is shown on a real screen. Run it in the dev container:
#   docker run --rm -v "$PWD":/src -v otm-cargo:/usr/local/cargo/registry -v otm-linux-target:/target \
#     -e CARGO_TARGET_DIR=/target otm-linux-dev packaging/linux/capture-screenshots.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
out="$root/packaging/linux/screenshots"
mkdir -p "$out"
cargo build --manifest-path "$root/Cargo.toml" --quiet

for scene in overview process-tree; do
  xvfb-run -a -s "-screen 0 1240x800x24" bash -c '
    /target/debug/otm --demo "$1" & app=$!
    sleep 8
    import -window root "$2"
    kill "$app" 2>/dev/null || true
    wait "$app" 2>/dev/null || true
  ' _ "$scene" "$out/$scene.png"
done
