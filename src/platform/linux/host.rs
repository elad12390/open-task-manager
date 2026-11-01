//! Host access for the Linux platform layer. Inside a Flatpak sandbox (`/.flatpak-info` exists) the
//! process list and the host's services are not visible, so host commands run through
//! `flatpak-spawn --host` (the sandbox's `org.freedesktop.Flatpak` permission) and host files are read
//! from the `/run/host` mirror, or through a command on the host when the mirror lacks them. Outside a
//! sandbox both run directly, so callers never check where they are.
//!
//! Every path here names a host path. `run(argv)` returns a host command's stdout, `read(path)` a host
//! file's text, `sandboxed()` says which case applies. Every call starts a process, so sample many
//! values in one `run` (one `sh -c` script) rather than one call per value.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

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

/// The host's exit status and output of `argv`; an error only when the command cannot start.
pub fn output(argv: &[&str]) -> io::Result<Output> {
    let Some((program, args)) = argv.split_first() else {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "empty command"));
    };
    command_under(Path::new("/"), program, args).output()
}

/// The standard output of `argv` run on the host; an error with the command's stderr if it fails.
pub fn run(argv: &[&str]) -> io::Result<String> {
    let out = output(argv)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(io::Error::other(format!(
            "{} failed: {}",
            argv[0],
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// The standard output of `argv` on the host, whatever its exit status (empty when it cannot run).
pub fn stdout(argv: &[&str]) -> String {
    output(argv)
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default()
}

/// The host path under the `/run/host` mirror, which is the host's own tree when it is mirrored.
fn mirror(path: &Path) -> PathBuf {
    match path.strip_prefix(HOST_MIRROR) {
        Ok(_) => path.to_path_buf(),
        Err(_) => Path::new(HOST_MIRROR).join(path.strip_prefix("/").unwrap_or(path)),
    }
}

/// The host path without the mirror prefix, for commands that run on the host.
fn host_path(path: &Path) -> String {
    let path = path.strip_prefix(HOST_MIRROR).unwrap_or(path);
    path.to_string_lossy().into_owned()
}

/// Reads a host path: the mirror when it has the file, else the bytes of `cat` on the host.
pub fn read_bytes(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    if !sandboxed() {
        return fs::read(path);
    }
    fs::read(mirror(path)).or_else(|_| {
        let out = output(&["cat", &host_path(path)])?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                String::from_utf8_lossy(&out.stderr).trim().to_string(),
            ))
        }
    })
}

/// The text of a host file.
pub fn read(path: impl AsRef<Path>) -> io::Result<String> {
    read_bytes(path).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// The target of a host symbolic link.
pub fn read_link(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    let path = path.as_ref();
    if !sandboxed() {
        return fs::read_link(path);
    }
    let target = run(&["readlink", &host_path(path)])?;
    Ok(PathBuf::from(target.trim_end_matches('\n')))
}

/// The entries of a host directory, as full paths.
pub fn read_dir(path: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let path = path.as_ref();
    if !sandboxed() {
        return Ok(fs::read_dir(path)?.flatten().map(|e| e.path()).collect());
    }
    if let Ok(entries) = fs::read_dir(mirror(path)) {
        return Ok(entries.flatten().map(|e| e.path()).collect());
    }
    let out = output(&["ls", "-1A", "--", &host_path(path)])?;
    if !out.status.success() {
        return Err(io::Error::new(io::ErrorKind::NotFound, host_path(path)));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|name| path.join(name))
        .collect())
}

/// Whether a host path exists (the file itself or a directory).
pub fn exists(path: impl AsRef<Path>) -> bool {
    test(path.as_ref(), "-e")
}

pub fn is_file(path: impl AsRef<Path>) -> bool {
    test(path.as_ref(), "-f")
}

pub fn is_dir(path: impl AsRef<Path>) -> bool {
    test(path.as_ref(), "-d")
}

fn test(path: &Path, flag: &str) -> bool {
    if !sandboxed() {
        return match flag {
            "-d" => path.is_dir(),
            "-f" => path.is_file(),
            _ => path.exists(),
        };
    }
    if mirror(path).exists() {
        return match flag {
            "-d" => mirror(path).is_dir(),
            "-f" => mirror(path).is_file(),
            _ => true,
        };
    }
    output(&["test", flag, &host_path(path)]).is_ok_and(|o| o.status.success())
}

/// The size in bytes of a host file.
pub fn size(path: impl AsRef<Path>) -> Option<u64> {
    let path = path.as_ref();
    if !sandboxed() {
        return fs::metadata(path).ok().map(|m| m.len());
    }
    fs::metadata(mirror(path))
        .map(|m| m.len())
        .ok()
        .or_else(|| run(&["stat", "-L", "-c", "%s", &host_path(path)]).ok()?.trim().parse().ok())
}

/// Writes a host file, creating it or replacing it.
pub fn write(path: impl AsRef<Path>, data: &[u8]) -> io::Result<()> {
    let path = path.as_ref();
    if !sandboxed() {
        return fs::write(path, data);
    }
    let mut child = command_under(
        Path::new("/"),
        "sh",
        &["-c", "cat > \"$1\"", "sh", &host_path(path)],
    )
    .stdin(Stdio::piped())
    .spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("no stdin"))?
        .write_all(data)?;
    if child.wait()?.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("cannot write {}", path.display())))
    }
}

/// Copies a host file to a path in this process's own space (the app's cache, for instance).
pub fn copy(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::write(dst, read_bytes(src)?)
}

/// Creates a host directory and its parents.
pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if !sandboxed() {
        return fs::create_dir_all(path);
    }
    run(&["mkdir", "-p", "--", &host_path(path)]).map(|_| ())
}

/// Deletes a host file, if there is one.
pub fn remove_file(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if !sandboxed() {
        let _ = fs::remove_file(path);
        return;
    }
    let _ = output(&["rm", "-f", "--", &host_path(path)]);
}

/// Deletes a host folder and everything in it.
pub fn remove_dir_all(path: impl AsRef<Path>) {
    let path = path.as_ref();
    if !sandboxed() {
        let _ = fs::remove_dir_all(path);
        return;
    }
    let _ = output(&["rm", "-rf", "--", &host_path(path)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_commands_go_through_flatpak_spawn_only_in_a_sandbox() {
        let sandbox = tempfile_dir("sandboxed");
        fs::write(sandbox.join(SANDBOX_MARK), "").unwrap();
        let cmd = command_under(&sandbox, "ss", &["-tunapH"]);
        assert_eq!(cmd.get_program(), "flatpak-spawn");
        assert_eq!(
            cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>(),
            ["--host", "ss", "-tunapH"]
        );
        let native = tempfile_dir("native");
        assert_eq!(command_under(&native, "ss", &[]).get_program(), "ss");
        assert!(!sandboxed_under(&native));
        assert!(sandboxed_under(&sandbox));
        fs::remove_dir_all(sandbox).ok();
        fs::remove_dir_all(native).ok();
    }

    #[test]
    fn mirror_paths_and_host_paths_round_trip() {
        assert_eq!(
            mirror(Path::new("/usr/share/applications")),
            PathBuf::from("/run/host/usr/share/applications")
        );
        assert_eq!(
            mirror(Path::new("/run/host/usr/share")),
            PathBuf::from("/run/host/usr/share")
        );
        assert_eq!(host_path(Path::new("/run/host/etc/os-release")), "/etc/os-release");
        assert_eq!(host_path(Path::new("/sys/class/thermal")), "/sys/class/thermal");
    }

    #[test]
    fn native_host_helpers_match_the_filesystem() {
        let dir = tempfile_dir("native-helpers");
        let file = dir.join("note.txt");
        write(&file, b"hello").unwrap();
        assert_eq!(read(&file).unwrap(), "hello");
        assert_eq!(size(&file), Some(5));
        assert!(is_file(&file) && !is_dir(&file) && exists(&file));
        assert!(is_dir(&dir));
        let names: Vec<_> = read_dir(&dir).unwrap();
        assert_eq!(names, vec![file.clone()]);
        remove_file(&file);
        assert!(!exists(&file));
        remove_dir_all(&dir);
        assert!(!is_dir(&dir));
    }

    #[test]
    fn host_source_files_never_touch_the_filesystem_directly() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/platform/linux");
        let forbidden = [
            "Command::new(",
            "fs::read",
            "fs::write",
            "fs::copy",
            "fs::metadata",
            "fs::create",
            "fs::remove",
            "read_dir(",
            "read_link(",
            ".exists()",
            ".is_file()",
            ".is_dir()",
        ];
        for entry in fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("rs")
                || path.file_name().and_then(|n| n.to_str()) == Some("host.rs")
            {
                continue;
            }
            let text = fs::read_to_string(&path).unwrap();
            let code = text.split("#[cfg(test)]").next().unwrap_or("");
            for pattern in forbidden {
                assert!(
                    !code.contains(pattern),
                    "{} uses `{pattern}` outside host.rs",
                    path.display()
                );
            }
        }
    }

    fn tempfile_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("otm-host-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
