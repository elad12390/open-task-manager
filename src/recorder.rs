use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{metrics::Snapshot, sysops};

pub const FORMAT: &str = "otm-trace-1";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub t: u64,
    pub cpu: f32,
    pub memory: f32,
    pub swap: u64,
    pub down: u64,
    pub up: u64,
    pub cores: Vec<f32>,
    pub top: Vec<(String, f32)>,
}

impl Frame {
    pub fn from_snapshot(s: &Snapshot) -> Self {
        let mut top: Vec<(String, f32)> = s
            .processes
            .iter()
            .map(|p| (p.name.clone(), p.cpu))
            .collect();
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(5);
        Self {
            t: sysops::now_secs(),
            cpu: s.cpu,
            memory: if s.memory_total == 0 {
                0.
            } else {
                s.memory_used as f32 / s.memory_total as f32 * 100.
            },
            swap: s.swap_used,
            down: s.net_down,
            up: s.net_up,
            cores: s.cores.clone(),
            top,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Recording {
    pub format: String,
    pub started: u64,
    pub frames: Vec<Frame>,
}

pub fn dir() -> PathBuf {
    sysops::data_dir().join("Recordings")
}

pub fn save(frames: Vec<Frame>) -> Result<PathBuf, String> {
    let started = frames.first().map(|f| f.t).unwrap_or_else(sysops::now_secs);
    let rec = Recording {
        format: FORMAT.into(),
        started,
        frames,
    };
    fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let path = dir().join(format!("recording-{started}.otmtrace"));
    let json = serde_json::to_vec(&rec).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn list() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir())
        .map(|r| {
            r.flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("otmtrace"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files.reverse();
    files
}

pub fn load(path: &PathBuf) -> Result<Recording, String> {
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let rec: Recording = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if rec.format != FORMAT {
        return Err(format!("Unsupported recording format {}", rec.format));
    }
    Ok(rec)
}
