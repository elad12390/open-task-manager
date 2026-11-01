#!/usr/bin/env bash
# Captures the Linux demo frames (`otm --demo <scene>`) headless, inside the otm-linux-dev image, so
# nothing is shown on a real screen. Frames for review go to tmp/linux-frames/<scene>-<seconds>.png; the
# two MetaInfo screenshots (overview, process-tree) go to packaging/linux/screenshots/ from a second pass
# with the captions hidden (OTM_DEMO_CAPTIONS=0). Every capture must have more than 500 colours.
#
# Run it from the worktree, on the otm Colima profile:
#   DOCKER_HOST="unix://$HOME/.colima/otm/docker.sock" docker run --rm --name otm-capture-1 \
#     -v "$PWD":/work/demo-linux -w /work/demo-linux -v otm-cargo:/usr/local/cargo/registry \
#     -v otm-linux-target:/target -e CARGO_TARGET_DIR=/target/demo-linux otm-linux-dev \
#     ci/capture-linux-frames.sh
#
# X server quirks: Xvfb runs with the Composite extension disabled (`-extension Composite`), because with
# it on GPUI picks a 32-bit visual that `import` cannot see. GPUI's X11 backend also draws nothing without a
# window manager, so openbox manages the window; the client window is captured at its own size.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
out="$root/packaging/linux/screenshots"
frames="$root/tmp/linux-frames"
target="${CARGO_TARGET_DIR:-/target/demo-linux}"
work="$(mktemp -d)"
scenes=(overview process-tree memory columns filters context-menu keyboard pages)
store=(overview process-tree)

mkdir -p "$out" "$frames" "$work/home/.config" "$work/data"
printf '[Icons]\nTheme=Papirus\n' > "$work/home/.config/kdeglobals"

if ! command -v openbox >/dev/null || ! command -v xdotool >/dev/null || [ ! -d /usr/share/icons/Papirus ]; then
  apt-get update -qq
  DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
    openbox xdotool papirus-icon-theme adwaita-icon-theme >/dev/null
fi

cargo build --quiet --manifest-path "$root/Cargo.toml"
bin="$target/debug/otm"

Xvfb :8 -screen 0 1600x1000x24 -extension Composite >"$work/xvfb.log" 2>&1 &
xvfb=$!
export DISPLAY=:8
wm=""
cleanup() {
  [ -n "$wm" ] && kill "$wm" 2>/dev/null || true
  kill "$xvfb" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT
for _ in $(seq 1 40); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.25; done
openbox >"$work/openbox.log" 2>&1 &
wm=$!
sleep 1

mkdir -p -m 700 "$work/xdg"
export HOME="$work/home" OTM_DATA_DIR="$work/data" OTM_WINDOW_SIZE=1440x860 XDG_RUNTIME_DIR="$work/xdg"

# run_scene <scene> <captions: 1|0> <frame-prefix>: runs one tour, saves a frame every two seconds.
run_scene() {
  local scene="$1" captions="$2" prefix="$3" app win t=0
  # Each run gets a fresh data folder: a scene that changes the columns saves its layout.
  mkdir -p "$work/data-$scene-$captions"
  # No seasonal theme: the date a capture runs on shouldn't decide how the screenshots look.
  printf '{"holiday_theme": "Off"}\n' >"$work/data-$scene-$captions/native-settings.json"
  OTM_DATA_DIR="$work/data-$scene-$captions" OTM_DEMO_CAPTIONS="$captions" "$bin" --demo "$scene" >"$work/$scene.log" 2>&1 &
  app=$!
  for _ in $(seq 1 60); do
    win="$(xdotool search --onlyvisible --class 'OpenTaskManager' 2>/dev/null | tail -1 || true)"
    [ -n "$win" ] && break
    sleep 0.5
  done
  if [ -z "${win:-}" ]; then
    echo "no window for $scene; log:" >&2
    cat "$work/$scene.log" >&2
    kill "$app" 2>/dev/null || true
    return 1
  fi
  # A tour ends on its own; 45 s bounds a scene that keeps the app open.
  while kill -0 "$app" 2>/dev/null && ((t < 45)); do
    sleep 1
    t=$((t + 1))
    if ((t % 2 == 0)); then
      # import waits forever on a window that closes mid-capture.
      timeout 5 import -window "$win" "$prefix-$(printf '%02d' "$t").png" 2>/dev/null || true
    fi
  done
  kill "$app" 2>/dev/null || true
  wait "$app" || true
}

for scene in "${scenes[@]}"; do
  run_scene "$scene" 1 "$frames/$scene"
done

for scene in "${store[@]}"; do
  run_scene "$scene" 0 "$work/store-$scene"
  # The richest frame (most colours: the most rows and icons on screen).
  best="$(for f in "$work/store-$scene"-*.png; do echo "$(identify -format '%k' "$f") $f"; done | sort -n | tail -1 | cut -d' ' -f2)"
  cp "$best" "$out/$scene.png"
done

min=999999
for f in "$frames"/*.png "$out"/*.png; do
  colours="$(identify -format '%k' "$f")"
  if [ "$colours" -le 500 ]; then
    echo "BLANK: $f has $colours colours" >&2
    exit 1
  fi
  ((colours < min)) && min=$colours
  echo "$colours $f"
done
echo "all frames have more than 500 colours (lowest: $min)"
