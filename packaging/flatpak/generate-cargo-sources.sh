#!/usr/bin/env bash
# Regenerates cargo-sources.json from Cargo.lock: every crate the build needs, vendored for an offline build.
# Run it after every Cargo.lock change and commit the result. The generator is flatpak-builder-tools'
# flatpak-cargo-generator.py at a pinned commit; uv runs it with its own inline dependencies.
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
generator_commit=9d86d336a39b9af38b86f114b6ce5820fe260f91
generator="https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/$generator_commit/cargo/flatpak-cargo-generator.py"

cd "$root"
uv run "$generator" Cargo.lock -o packaging/flatpak/cargo-sources.json
