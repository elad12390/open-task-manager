#!/usr/bin/env bash
# Regenerate the hicolor PNG icon set from assets/AppIcon.png (ImageMagick 7 `magick` or 6 `convert`).
set -euo pipefail

cd "$(dirname "$0")/../.."
APP_ID="io.github.elad12390.OpenTaskManager"
SRC="assets/AppIcon.png"
OUT="packaging/linux/icons/hicolor"
IM="$(command -v magick || command -v convert)"

for size in 16 24 32 48 64 128 256 512; do
  dir="$OUT/${size}x${size}/apps"
  mkdir -p "$dir"
  "$IM" "$SRC" -filter Lanczos -resize "${size}x${size}" -strip "$dir/$APP_ID.png"
done
