//! Demo mode's fictional machines: each OS's data is complete, internally consistent and names the
//! processes the scene scripts point at.

use std::collections::HashSet;

use crate::platform::demo_data;

#[test]
fn demo_processes_have_unique_ids_and_parents_in_the_demo() {
    let machine = demo_data::machine();
    let pids: HashSet<u32> = machine.seeds.iter().map(|s| s.pid).collect();
    assert_eq!(pids.len(), machine.seeds.len(), "demo pids must be unique");
    for s in machine.seeds {
        assert!(
            s.ppid == 0 || pids.contains(&s.ppid),
            "{} (pid {}) has a parent that is not in the demo",
            s.name,
            s.pid
        );
    }
}

#[test]
fn scene_scripts_find_the_processes_they_point_at() {
    let machine = demo_data::machine();
    for pid in [1, 1200, 1604] {
        assert!(
            machine.seeds.iter().any(|s| s.pid == pid),
            "the scene scripts use pid {pid}, which the demo machine must have"
        );
    }
}

#[test]
fn demo_machine_names_its_own_operating_system() {
    let machine = demo_data::machine();
    let intro = machine.captions.intro;
    if cfg!(target_os = "linux") {
        assert_eq!(machine.host, "Demo PC");
        assert!(intro.contains("Linux") && !intro.contains("macOS"));
    } else {
        assert_eq!(machine.host, "Demo Mac");
        assert!(intro.contains("macOS") && !intro.contains("Linux"));
    }
}

#[test]
fn demo_memory_and_disk_are_consistent() {
    let machine = demo_data::machine();
    assert!(machine.memory_base < machine.memory_total);
    assert!(machine.disk_free <= machine.disk_total);
    assert!(
        machine.folders.iter().map(|(_, gb)| gb).sum::<u64>()
            <= (machine.disk_total - machine.disk_free) / (1024 * 1024 * 1024)
    );
}
