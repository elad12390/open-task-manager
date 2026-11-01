#!/usr/bin/env bash
# Writes the hicolor PNG icons of the Flatpak and .deb packages from assets/AppIcon.png.
# Needs ImageMagick (magick, or convert on ImageMagick 6); the dev container has it:
#   docker run --rm -v "$PWD":/src otm-linux-dev packaging/linux/make-icons.sh
set -euo pipefail

root="$(cd "$(dirname "$0")/../.." && pwd)"
source_png="$root/assets/AppIcon.png"
theme="$root/packaging/linux/icons/hicolor"
app_id=io.github.elad12390.open_task_manager.OpenTaskManager

if command -v magick >/dev/null; then
  convert=(magick)
else
  convert=(convert)
fi

for size in 16 24 32 48 64 128 256 512; do
  dir="$theme/${size}x${size}/apps"
  mkdir -p "$dir"
  "${convert[@]}" "$source_png" -filter Lanczos -resize "${size}x${size}" -strip \
    "PNG32:$dir/$app_id.png"
done
