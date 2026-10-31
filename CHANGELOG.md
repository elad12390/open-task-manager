# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org).

## [0.1.0]

First public release.

- Native macOS system monitor in Rust and GPUI with Liquid-Glass styling.
- Processes: flat list, process tree (operating system vs. your processes, grouped by subsystem and owner app),
  owner, user and category grouping; per-branch memory sums; batch folding; keyboard navigation.
- Dynamic filters: autocomplete chips for app, user, category and state, plus `cpu>` and `mem>` thresholds.
- Configurable, reorderable, resizable columns with horizontal scrolling; app icons.
- Right-click actions: quit, force quit (with children), suspend/resume, show in Finder, show connections, copy.
- Overview, Users, Connections, Power, Startup items, Services, Installed apps, Disk space, Benchmarks,
  Flight recorder, Settings, popout and screensaver.
- Menu bar with update-speed controls and shortcuts.
- A filter field with a caret and focus ring (⌘F), match counts while filtering, and an empty state with
  Clear filters; a two-row toolbar that never overflows; long names end in "…".
- `otm --demo <scene>`: a fictional Mac for screenshots and recordings; nothing real is read or signalled.
