<p align="center">
  <img src="assets/AppIcon.png" width="128" height="128" alt="Open Task Manager icon">
</p>

<h1 align="center">Open Task Manager</h1>

<p align="center">
  A fast, native, open-source system monitor for macOS, written in Rust with <a href="https://gpui.rs">GPUI</a>.<br>
  Everything runs locally. Nothing is sent anywhere.
</p>

<p align="center">
  <a href="https://github.com/elad12390/open-task-manager/actions/workflows/ci.yml"><img src="https://img.shields.io/badge/CI-GitHub%20Actions-2088FF?logo=githubactions&logoColor=white" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-3E7BFA" alt="License: Apache-2.0"></a>
  <img src="https://img.shields.io/badge/macOS-13%2B-1D1D1F?logo=apple&logoColor=white" alt="macOS 13+">
  <a href="https://elad12390.github.io/open-task-manager/"><img src="https://img.shields.io/badge/website-online-5FF5E0" alt="Website"></a>
</p>

---

## Install

### Homebrew (recommended)

```sh
brew install --cask elad12390/tap/open-task-manager
```

Upgrade later with `brew upgrade --cask open-task-manager`.

### Download

Grab `OpenTaskManager-<version>.dmg` from the [latest release](https://github.com/elad12390/open-task-manager/releases/latest),
open it and drag **Open Task Manager** into **Applications**. The app is a universal binary (Apple silicon and Intel).

> **Gatekeeper:** releases are ad-hoc signed but not notarized (the project has no paid Apple Developer ID).
> The first time, right-click the app and choose **Open**, or clear the download flag:
> `xattr -dr com.apple.quarantine "/Applications/Open Task Manager.app"`

### Build from source

Requires Rust (stable) and the Xcode Command Line Tools. No full Xcode is needed: GPUI compiles its Metal shaders at runtime.

```sh
git clone https://github.com/elad12390/open-task-manager.git
cd open-task-manager
cargo run --release              # run it
./scripts/package.sh             # build dist/Open Task Manager.app and a DMG
```

## Features

- **Processes** as a flat list, a **process tree** (with the operating system's processes and yours in separate
  branches, grouped by subsystem and by owning app), by **owner app**, by **user account**, or by **category**.
- **Memory sums per branch** (Σ) so you can see what a whole app with all its helpers costs.
- **Configurable columns**: show/hide, reorder by dragging, resize, sort; scroll sideways to reach every column.
- **App icons** next to processes and owner groups.
- **Right-click actions**: Quit, Force Quit, Force Quit with children, Suspend/Resume, Show in Finder,
  Show Connections, Copy PID/path/info. Mass-quitting the operating system's own processes is disabled.
- **Keyboard navigation**: ↑↓, Page Up/Down, Home/End, ← → to fold and unfold, Space to toggle, ⌥→ to open a whole branch.
  Folding a branch folds everything inside it.
- **Overview** (CPU, memory, network, disk, per-core load, system info), **Users**, **Connections**, **Power & thermal**,
  **Startup items** (enable/disable your own launch agents), **Services** (restart/stop), **Installed apps**
  (uninstall to the Trash), **Disk space** (largest folders, scan history), **Benchmarks** (CPU single/all cores,
  memory bandwidth, uncached disk), **Flight recorder** (record, save, replay), **Settings**.
- **Popout** always-on-top mini monitor and a full-screen **screensaver** (F12).
- A complete macOS menu bar with an **Update** menu (every 1/2/5/10 s, pause, update now).
- Liquid-Glass styling that follows light/dark mode, with text size and high-contrast options.

## Privacy

Open Task Manager reads system information with macOS APIs and standard tools (`ps`, `lsof`, `launchctl`, `pmset`,
`du`). It has no telemetry, makes no network requests, and stores its settings, layouts, scans and recordings only
in `~/Library/Application Support/OpenTaskManager`.

## Development

```sh
cargo test                      # UI tests in GPUI's in-memory test window (no windows open)
cargo clippy --all-targets -- -D warnings
cargo fmt --check
cargo run -- --selftest          # end-to-end checks against the live system (see below)
```

`--selftest` exercises the real actions on disposable targets: it loads and unloads a throwaway launch agent,
moves a throwaway `.app` from `~/Applications` to the Trash, and quits throwaway `sleep` processes. Add `--bench`
to run the benchmarks and `--scan` to run a home-folder disk scan.

### Project layout

| Path | What it is |
|---|---|
| `src/app.rs` | App state, window, sampling loop, sidebar |
| `src/processes.rs` | Process table: rows, trees, grouping, folding, keyboard navigation |
| `src/categories.rs` | macOS vs. app vs. tool classification and owner apps |
| `src/context_menu.rs` | Right-click menu |
| `src/menus.rs` | Menu bar and shortcuts |
| `src/pages.rs`, `src/overview.rs` | The other pages |
| `src/metrics.rs`, `src/sysops.rs` | System sampling and guarded system actions |
| `src/icons.rs`, `src/app_icon.js` | App-icon rendering cache |
| `src/ui_tests.rs`, `src/selftest.rs` | Tests |
| `scripts/package.sh` | Builds the `.app` and DMG |
| `scripts/make_icon.py` | Rebuilds the app icon from `assets/AppIcon-raw.png` |
| `site/` | The landing page (GitHub Pages) |

### Releasing

Push a tag like `v0.2.0`. The release workflow builds a universal DMG, publishes a GitHub release and updates the
Homebrew cask in [`elad12390/homebrew-tap`](https://github.com/elad12390/homebrew-tap).

## Contributing

Issues and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) and the
[Code of Conduct](CODE_OF_CONDUCT.md). Report security issues privately as described in [SECURITY.md](SECURITY.md).

## License

[Apache License 2.0](LICENSE).
