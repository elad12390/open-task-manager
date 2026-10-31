#!/usr/bin/env bash
# Record the README / website GIFs from the app's demo mode (fictional data only).
#
#   scripts/record_demos.sh                 # every scene
#   scripts/record_demos.sh overview keyboard
#
# Each scene opens a floating window (it does not take keyboard focus), plays its script and quits.
# Requires ffmpeg and the Screen Recording permission for your terminal.
set -euo pipefail

cd "$(dirname "$0")/.."
OUT="assets/demo"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$OUT"

cargo build --release
swiftc -O scripts/winid.swift -o "$WORK/winid"

SCENES=("$@")
if [[ ${#SCENES[@]} -eq 0 ]]; then
  SCENES=(overview process-tree memory columns context-menu keyboard pages)
fi

for scene in "${SCENES[@]}"; do
  frames="$WORK/$scene"
  mkdir -p "$frames"
  data="/tmp/OpenTaskManager-demo"
  rm -rf "$data"

  OTM_DATA_DIR="$data" OTM_BACKGROUND=1 ./target/release/otm --demo "$scene" >/dev/null 2>&1 &
  pid=$!

  win=""
  for _ in $(seq 1 50); do
    win="$("$WORK/winid" "$pid" 2>/dev/null || true)"
    [[ -n "$win" ]] && break
    sleep 0.1
  done
  if [[ -z "$win" ]]; then
    echo "$scene: window did not appear" >&2
    kill "$pid" 2>/dev/null || true
    exit 1
  fi

  n=0
  start=$(date +%s.%N)
  while kill -0 "$pid" 2>/dev/null; do
    screencapture -x -o -l"$win" -t png "$(printf '%s/f%05d.png' "$frames" "$n")" 2>/dev/null || break
    n=$((n + 1))
  done
  end=$(date +%s.%N)
  wait "$pid" 2>/dev/null || true
  # Drop the last frame (captured while the window was closing).
  last="$(printf '%s/f%05d.png' "$frames" $((n - 1)))"
  [[ -f "$last" ]] && rm -f "$last" && n=$((n - 1))

  fps=$(echo "$n / ($end - $start)" | bc -l)
  ffmpeg -loglevel error -y -framerate "$fps" -i "$frames/f%05d.png" \
    -vf "fps=8,scale=1240:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle" \
    "$OUT/$scene.gif"
  if command -v gifsicle >/dev/null; then
    gifsicle -O3 --lossy=60 --batch "$OUT/$scene.gif"
  fi
  printf '%-14s %4d frames at %4.1f fps -> %s (%s)\n' "$scene" "$n" "$fps" "$OUT/$scene.gif" "$(du -h "$OUT/$scene.gif" | cut -f1)"
done
rm -rf /tmp/OpenTaskManager-demo
