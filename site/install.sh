#!/bin/sh
# Installs the latest Open Task Manager for this computer:
#   curl -fsSL https://elad12390.github.io/open-task-manager/install.sh | sh
#
# macOS: the Homebrew cask when Homebrew is installed, else the DMG copied to Applications.
# Linux: the Flatpak when flatpak is installed, else the .deb through apt (Debian, Ubuntu and derivatives).
# Every download is checked against the .sha256 file published next to it.
#
# OTM_INSTALL=flatpak|deb picks the Linux format; OTM_DRY_RUN=1 prints what would run without running it.
set -eu

REPO="elad12390/open-task-manager"
TAP_CASK="elad12390/tap/open-task-manager"
APP_ID="io.github.elad12390.open_task_manager.OpenTaskManager"

say() { printf '%s\n' "$*"; }
fail() {
	printf 'open-task-manager: %s\n' "$*" >&2
	exit 1
}
run() {
	say "+ $*"
	if [ "${OTM_DRY_RUN:-}" != 1 ]; then "$@"; fi
}
need() { command -v "$1" >/dev/null 2>&1 || fail "this installer needs $1"; }

need curl
need uname

latest_version() {
	tag="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" |
		sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)"
	[ -n "$tag" ] || fail "could not find the latest release on GitHub"
	printf '%s' "$tag"
}

sha256_of() {
	if command -v sha256sum >/dev/null 2>&1; then
		sha256sum "$1" | cut -d' ' -f1
	else
		shasum -a 256 "$1" | cut -d' ' -f1
	fi
}

# fetch <asset>: downloads a release asset into $tmp and checks it against <asset>.sha256.
fetch() {
	url="https://github.com/$REPO/releases/download/v$version/$1"
	say "Downloading $1"
	if [ "${OTM_DRY_RUN:-}" = 1 ]; then
		say "+ curl -fL $url"
		return
	fi
	curl -fL --progress-bar -o "$tmp/$1" "$url" || fail "download failed: $url"
	expected="$(curl -fsSL "$url.sha256" | cut -d' ' -f1)"
	actual="$(sha256_of "$tmp/$1")"
	[ -n "$expected" ] && [ "$expected" = "$actual" ] || fail "checksum mismatch for $1"
	say "Checksum OK"
}

# sudo only when not already root.
as_root() {
	if [ "$(id -u)" = 0 ]; then run "$@"; else need sudo && run sudo "$@"; fi
}

install_macos() {
	if command -v brew >/dev/null 2>&1; then
		say "Installing with Homebrew"
		run brew install --cask "$TAP_CASK"
		return
	fi
	need hdiutil
	dmg="OpenTaskManager-$version.dmg"
	fetch "$dmg"
	dest=/Applications
	[ -w "$dest" ] || dest="$HOME/Applications"
	run mkdir -p "$dest"
	if [ "${OTM_DRY_RUN:-}" = 1 ]; then
		say "+ copy Open Task Manager.app from $dmg to $dest"
		return
	fi
	mount="$tmp/mount"
	mkdir -p "$mount"
	hdiutil attach -nobrowse -quiet -mountpoint "$mount" "$tmp/$dmg"
	app="$(find "$mount" -maxdepth 1 -name '*.app' | head -n 1)"
	[ -n "$app" ] || {
		hdiutil detach -quiet "$mount"
		fail "no app found in $dmg"
	}
	name="$(basename "$app")"
	rm -rf "${dest:?}/${name:?}"
	cp -R "$app" "$dest/"
	hdiutil detach -quiet "$mount"
	say "Installed $dest/$name"
}

install_flatpak() {
	bundle="OpenTaskManager-$version-$flatpak_arch.flatpak"
	fetch "$bundle"
	# The runtime comes from Flathub.
	run flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
	run flatpak install --user -y --noninteractive --bundle "$tmp/$bundle"
	say "Installed $APP_ID (flatpak run $APP_ID)"
}

install_deb() {
	command -v apt-get >/dev/null 2>&1 ||
		fail "no flatpak or apt here: install flatpak, or download a build from https://github.com/$REPO/releases/latest"
	deb="open-task-manager_$version-1_$deb_arch.deb"
	fetch "$deb"
	# apt reads the file as its own user: make it readable.
	if [ "${OTM_DRY_RUN:-}" != 1 ]; then chmod 0644 "$tmp/$deb" && chmod 0755 "$tmp"; fi
	as_root apt-get install -y "$tmp/$deb"
	say "Installed open-task-manager (run otm, or find Open Task Manager in your app menu)"
}

install_linux() {
	case "$(uname -m)" in
	x86_64 | amd64) deb_arch=amd64 flatpak_arch=x86_64 ;;
	aarch64 | arm64) deb_arch=arm64 flatpak_arch=aarch64 ;;
	*) fail "no Linux build for $(uname -m) (x86_64 and aarch64 only)" ;;
	esac
	case "${OTM_INSTALL:-}" in
	flatpak) need flatpak && install_flatpak ;;
	deb) install_deb ;;
	"") if command -v flatpak >/dev/null 2>&1; then install_flatpak; else install_deb; fi ;;
	*) fail "OTM_INSTALL must be flatpak or deb" ;;
	esac
}

version="$(latest_version)"
say "Open Task Manager $version"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

case "$(uname -s)" in
Darwin) install_macos ;;
Linux) install_linux ;;
*) fail "Open Task Manager runs on macOS and Linux; for $(uname -s), see https://github.com/$REPO" ;;
esac
