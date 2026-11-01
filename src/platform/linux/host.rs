//! Host access for the Linux platform layer. Inside a Flatpak sandbox (`/.flatpak-info` exists) the
//! process list and the host's services are not visible, so host commands run through
//! `flatpak-spawn --host` (the sandbox's `org.freedesktop.Flatpak` permission) and host files are read
//! from the `/run/host` mirror, or through `cat` on the host when the mirror lacks them. Outside a
//! sandbox both run directly, so callers never check where they are.
//!
//! For the other Linux modules: `run(argv)` returns a host command's stdout, `read(path)` returns a
//! host file's text, `sandboxed()` says which case applies. Every call starts a process, so sample
//! many values in one `run` (one `sh -c` script) rather than one call per value.

use std::{fs, io, path::Path, process::Command};

const SANDBOX_MARK: &str = ".flatpak-info";
const HOST_MIRROR: &str = "/run/host";

pub fn sandboxed() -> bool {
    sandboxed_under(Path::new("/"))
}

/// Whether the sandbox marker exists under `root` (the filesystem root in production).
pub fn sandboxed_under(root: &Path) -> bool {
    root.join(SANDBOX_MARK).is_file()
}

/// The command that runs `program args` on the host, given where the sandbox marker lives.
pub fn command_under(root: &Path, program: &str, args: &[&str]) -> Command {
    let mut command = if sandboxed_under(root) {
        let mut spawn = Command::new("flatpak-spawn");
        spawn.arg("--host").arg(program);
        spawn
    } else {
        Command::new(program)
    };
    command.args(args);
    command
}

/// The standard output of `argv` run on the host; an error with the command's stderr if it fails.
pub fn run(argv: &[&str]) -> io::Result<String> {
    let Some((program, args)) = argv.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    };
    let output = command_under(Path::new("/"), program, args).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(io::Error::other(format!(
            "{program} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

/// The text of a host file.
pub fn read(path: &str) -> io::Result<String> {
    if !sandboxed() {
        return fs::read_to_string(path);
    }
    fs::read_to_string(Path::new(HOST_MIRROR).join(path.trim_start_matches('/')))
        .or_else(|_| run(&["cat", path]))
}
