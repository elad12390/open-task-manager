# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org).

## [0.1.1]

Sortable, filterable tables everywhere, and a pass over the small details, checked against Apple's
Human Interface Guidelines, WCAG 2.2 and published table, filter and dashboard guidance.

- Every table sorts by clicking a header (numbers by value, biggest first; click again to reverse), and
  every table filters on all of its columns: Users, Startup Items, Services, Connections, Installed Apps,
  and the Disk Space folders and scan history. Users is now a sortable table.
- Force Quit asks first (Cancel is the default; Return and Esc cancel), single or bulk. Force Quit items sit
  apart from Quit in the right-click menu, and items that don't apply are hidden instead of dimmed.
- Quit and Force Quit stay in the toolbar, dimmed without a selection, so it no longer shifts. Both are in the
  Edit menu too (Quit Process ⌥⌘Q). Pause Updates moved to ⌥⌘P, freeing ⌘P.
- The filter field edits at a caret (←→, Home/End, ⌥⌫), Space types, ↓ opens suggestions, hovering moves the
  highlight, clicking a chip turns it back into text, and `cpu>` / `mem>` without a number says what to type.
- Shortened text shows its full value on hover; paths keep their end ("…" in the middle).
- Numbers use fixed-width digits so live values don't jitter; disk and file sizes use Finder's 1000-based
  units; per-process disk and network figures are true per-second rates; tiny values show "<1%".
- Better contrast for secondary text, borders and selected rows (white text on a darker blue); a 2 px focus
  ring; larger click targets for chip and clear buttons; equal-width grouping buttons.
- Notices carry ✓ or ⚠ as well as colour, errors stay 15 seconds, and copying says what was copied.
- Empty tables say so (with Clear filter when filtering), "12 of 40 shown" while filtering, headers stay
  visible while a page loads, and arrow keys only scroll as far as needed.
- The window title follows the page.

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
