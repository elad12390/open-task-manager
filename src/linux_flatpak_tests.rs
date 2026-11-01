#![cfg(target_os = "linux")]

use std::{fs, path::PathBuf};

use crate::platform::host;

fn scratch_root(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("otm-{name}-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    root
}

fn args_of(command: &std::process::Command) -> Vec<String> {
    command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn sandbox_is_detected_from_the_flatpak_info_marker() {
    let root = scratch_root("flatpak-marker");
    assert!(!host::sandboxed_under(&root));
    fs::write(
        root.join(".flatpak-info"),
        "[Application]\nname=io.github.elad12390.open_task_manager\n",
    )
    .unwrap();
    assert!(host::sandboxed_under(&root));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn host_commands_inside_the_sandbox_go_through_flatpak_spawn() {
    let root = scratch_root("flatpak-spawn");
    fs::write(root.join(".flatpak-info"), "").unwrap();
    let command = host::command_under(&root, "ps", &["-eo", "pid"]);
    assert_eq!(command.get_program(), "flatpak-spawn");
    assert_eq!(args_of(&command), ["--host", "ps", "-eo", "pid"]);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn host_commands_outside_the_sandbox_run_directly() {
    let root = scratch_root("no-sandbox");
    let command = host::command_under(&root, "ps", &["-eo", "pid"]);
    assert_eq!(command.get_program(), "ps");
    assert_eq!(args_of(&command), ["-eo", "pid"]);
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn run_returns_the_standard_output_of_a_command() {
    assert_eq!(host::run(&["echo", "otm"]).unwrap().trim(), "otm");
}

#[test]
fn run_reports_a_failing_command_as_an_error() {
    assert!(host::run(&["false"]).is_err());
    assert!(host::run(&[]).is_err());
}

#[test]
fn read_returns_a_host_file_outside_the_sandbox() {
    assert!(host::read("/etc/os-release").unwrap().contains("ID="));
}
