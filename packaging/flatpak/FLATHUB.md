# Flathub submission kit

App ID `io.github.elad12390.OpenTaskManager`. The files for Flathub are:

- `io.github.elad12390.OpenTaskManager.yml`: the manifest (runtime `org.freedesktop.Platform` 26.08, Rust SDK extension).
- `cargo-sources.json`: every crate in `Cargo.lock`, vendored so the build runs offline. Regenerate it with `generate-cargo-sources.sh` after any `Cargo.lock` change.
- `flathub.json`: the architectures (`x86_64`, `aarch64`).
- `../linux/`: the desktop file, the MetaInfo file, the icons and the release notes script.

## Local build, lint and install

`build.sh` runs in a privileged flatpak-builder container (`ghcr.io/flathub-infra/flatpak-github-actions:freedesktop-26.08`) on Colima or Docker. It:

1. builds the manifest from this checkout (`type: dir`) into the named volume `otm-flatpak`;
2. writes the bundle to `dist/io.github.elad12390.OpenTaskManager.flatpak`;
3. runs `flatpak-builder-lint manifest`, `appstream` and `repo`;
4. installs the bundle and runs `flatpak run --command=otm io.github.elad12390.OpenTaskManager --selftest`.

```sh
packaging/flatpak/build.sh
```

The selftest inside the sandbox exercises the host route: host commands run through `flatpak-spawn --host`, which needs the `org.freedesktop.Flatpak` permission and the session helper on the bus.

## Before the submission PR

The local manifest points at this checkout. Flathub builds from a pinned source, so before submitting:

1. Release a tag (for example `v0.2.2`) that contains the Flatpak files.
2. In the manifest, replace the `dir` source with the git source of that tag:

   ```yaml
   sources:
     - type: git
       url: https://github.com/elad12390/open-task-manager.git
       tag: v0.2.2
       commit: <the full commit SHA of the tag>
     - cargo-sources.json
   ```

   Keep the `skip` of `dir` out of the git version. The build-commands stay the same.
3. Run `build.sh` once more against the git source, then `flatpak-builder-lint` as above.

Read the [Generative AI policy](https://docs.flathub.org/docs/for-app-authors/requirements#generative-ai-policy) in the Flathub requirements before submitting, and write the PR description yourself.

## Submission steps

Follow the [Flathub submission guide](https://docs.flathub.org/docs/for-app-authors/submission):

1. Fork `flathub/flathub` with **"Copy the master branch only" unchecked**.
2. Clone the fork on the `new-pr` branch and create a branch for the submission:

   ```sh
   git clone --branch=new-pr https://github.com/<your-user>/flathub.git && cd flathub
   git checkout -b io.github.elad12390.OpenTaskManager-submission new-pr
   ```

3. Copy into the root of that branch: `io.github.elad12390.OpenTaskManager.yml` (with the git source), `cargo-sources.json` and `flathub.json`. Fill in the PR template that the branch contains; do not delete it.
4. Commit and push, then open a pull request **against the `new-pr` base branch** (not `master`) titled `Add io.github.elad12390.OpenTaskManager`.
5. Reviewers may ask for changes. Answer them on the same PR; do not close it. Once the review comments are resolved, a maintainer can start a test build with `bot, build`.

The GitHub account that owns the `io.github.elad12390` ID must be `elad12390`. Flathub checks ownership against that account.

## PR description

Paste this into the PR description and adjust the details:

```markdown
## What

Open Task Manager is a native system monitor (Rust and GPUI, Apache-2.0): process list and tree, CPU, memory,
network, disk, services and startup items. Source: https://github.com/elad12390/open-task-manager

## Permissions

- `--talk-name=org.freedesktop.Flatpak`: a system monitor has to see and act on the host's processes, and the
  sandbox sees only its own. Host commands (process data, `systemctl`, `du`, `dpkg-query`, and the actions the
  user chooses such as quit or restart) run through `flatpak-spawn --host`. The app has no other way to read the
  host's process table. Mission Center (io.missioncenter.MissionCenter) requests the same permission for the same
  reason.
- `--system-talk-name=org.freedesktop.systemd1`: list and control system services.
- `--system-talk-name=org.freedesktop.PolicyKit1`: ask for authorization when an action needs root, so the app itself never runs as root.
- `--system-talk-name=org.freedesktop.UPower`: battery and power state.
- `--talk-name=org.freedesktop.FileManager1`: show a file in the file manager.
- `--socket=wayland`, `--socket=fallback-x11`, `--device=dri`, `--share=ipc`: the window and GPU.

No network permission and no filesystem permission: the app makes no network requests, and disk usage comes
from the host through `flatpak-spawn --host`.

## Testing

- Built and installed with flatpak-builder and org.flatpak.Builder; `flatpak run --command=otm <id> --selftest`
  passes in the sandbox.
- flatpak-builder-lint manifest, repo and appstream: <paste the output and any exceptions>.
```

## Known review risks

- Flathub does not accept system utilities that are generally used on the host, or host-dependent applications
  that cannot work without host components. This app runs on its own sandboxed data and reads the host through
  `flatpak-spawn --host`, which is the same route Mission Center takes. Expect questions about this; the
  permission justification above is the answer.
- The linter may report `finish-args-flatpak-spawn-access` for `org.freedesktop.Flatpak`. Flathub grants
  exceptions case by case in the flatpak-builder-lint exceptions file; the exception request has to explain
  this permission.
- Only the aarch64 build has been run locally. The x86_64 build is untested until a Flathub test build runs.

## Maintenance

On each release tag, update the git source (`tag` and `commit`) in the Flathub repository, and `cargo-sources.json`
if `Cargo.lock` changed. Flathub maintenance is described in the
[maintenance guide](https://docs.flathub.org/docs/for-app-authors/maintenance).
