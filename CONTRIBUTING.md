# Contributing

Thanks for helping improve Open Task Manager.

## Getting started

1. Install Rust (stable) and the Xcode Command Line Tools (`xcode-select --install`).
2. `cargo run` starts the app; `cargo test` runs the UI tests.

## Before opening a pull request

```sh
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs the same commands on macOS and builds the DMG.

## Guidelines

- Keep changes focused; one feature or fix per pull request.
- Add or update a test for behaviour you change. UI behaviour is tested in `src/ui_tests.rs` with GPUI's
  in-memory test window, so tests never open windows on screen. Checks that act on the live system belong in
  `src/selftest.rs` and must only touch disposable targets they create themselves.
- Never make an action able to harm the system by default: actions on macOS's own processes, system launch daemons
  or apps outside `/Applications` and `~/Applications` stay refused or disabled.
- Do not add telemetry or network access. The app is local-only by design.

## Reporting bugs and requesting features

Use the issue templates. For bugs, include your macOS version, Mac model (Apple silicon or Intel) and the app version
(**Open Task Manager → About**).
