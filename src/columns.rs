use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::metrics::{ProcessInfo, fmt_bytes, fmt_duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ColumnId {
    Name,
    Owner,
    Pid,
    ParentPid,
    User,
    Cpu,
    Memory,
    OwnMemory,
    DiskRead,
    DiskWrite,
    State,
    RunTime,
    Path,
}

impl ColumnId {
    pub const ALL: [ColumnId; 13] = [
        ColumnId::Name,
        ColumnId::Owner,
        ColumnId::Pid,
        ColumnId::ParentPid,
        ColumnId::User,
        ColumnId::Cpu,
        ColumnId::Memory,
        ColumnId::OwnMemory,
        ColumnId::DiskRead,
        ColumnId::DiskWrite,
        ColumnId::State,
        ColumnId::RunTime,
        ColumnId::Path,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ColumnId::Name => "Name",
            ColumnId::Owner => "Owner",
            ColumnId::Pid => "PID",
            ColumnId::ParentPid => "Parent PID",
            ColumnId::User => "User",
            ColumnId::Cpu => "CPU",
            ColumnId::Memory => "Memory",
            ColumnId::OwnMemory => "Own Memory",
            ColumnId::DiskRead => "Disk Read",
            ColumnId::DiskWrite => "Disk Write",
            ColumnId::State => "State",
            ColumnId::RunTime => "Running",
            ColumnId::Path => "Path",
        }
    }

    pub fn default_width(self) -> f32 {
        match self {
            ColumnId::Name => 240.,
            ColumnId::Path => 360.,
            ColumnId::Owner => 150.,
            ColumnId::User => 120.,
            ColumnId::ParentPid => 90.,
            ColumnId::Memory | ColumnId::OwnMemory => 100.,
            _ => 86.,
        }
    }

    pub fn numeric(self) -> bool {
        !matches!(
            self,
            ColumnId::Name | ColumnId::Owner | ColumnId::User | ColumnId::State | ColumnId::Path
        )
    }

    pub fn cell(self, p: &ProcessInfo) -> String {
        match self {
            ColumnId::Name => p.name.clone(),
            ColumnId::Owner => p.owner_app.clone(),
            ColumnId::Pid => p.pid.to_string(),
            ColumnId::ParentPid => p.ppid.map_or_else(|| "—".into(), |x| x.to_string()),
            ColumnId::User => p.user.clone(),
            ColumnId::Cpu => format!("{:.1}%", p.cpu),
            ColumnId::Memory | ColumnId::OwnMemory => fmt_bytes(p.memory),
            ColumnId::DiskRead => format!("{}/s", fmt_bytes(p.disk_read)),
            ColumnId::DiskWrite => format!("{}/s", fmt_bytes(p.disk_write)),
            ColumnId::State => p.state.clone(),
            ColumnId::RunTime => fmt_duration(p.run_time),
            ColumnId::Path => p.path.clone(),
        }
    }

    pub fn compare(self, a: &ProcessInfo, b: &ProcessInfo) -> std::cmp::Ordering {
        match self {
            ColumnId::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            ColumnId::Owner => a.owner_app.to_lowercase().cmp(&b.owner_app.to_lowercase()),
            ColumnId::Pid => a.pid.cmp(&b.pid),
            ColumnId::ParentPid => a.ppid.cmp(&b.ppid),
            ColumnId::User => a.user.cmp(&b.user),
            ColumnId::Cpu => a.cpu.total_cmp(&b.cpu),
            ColumnId::Memory | ColumnId::OwnMemory => a.memory.cmp(&b.memory),
            ColumnId::DiskRead => a.disk_read.cmp(&b.disk_read),
            ColumnId::DiskWrite => a.disk_write.cmp(&b.disk_write),
            ColumnId::State => a.state.cmp(&b.state),
            ColumnId::RunTime => a.run_time.cmp(&b.run_time),
            ColumnId::Path => a.path.cmp(&b.path),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Column {
    pub id: ColumnId,
    pub width: f32,
    pub visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupBy {
    None,
    User,
    Parent,
    Category,
    App,
}

impl GroupBy {
    pub const ALL: [GroupBy; 5] = [
        GroupBy::None,
        GroupBy::App,
        GroupBy::User,
        GroupBy::Parent,
        GroupBy::Category,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GroupBy::None => "None",
            GroupBy::User => "User",
            GroupBy::App => "Owner",
            GroupBy::Parent => "Tree",
            GroupBy::Category => "Category",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TableLayout {
    pub columns: Vec<Column>,
    pub sort: ColumnId,
    pub descending: bool,
    pub group_by: GroupBy,
}

impl Default for TableLayout {
    fn default() -> Self {
        let visible = [
            ColumnId::Name,
            ColumnId::Owner,
            ColumnId::Pid,
            ColumnId::User,
            ColumnId::Cpu,
            ColumnId::Memory,
            ColumnId::DiskRead,
            ColumnId::DiskWrite,
            ColumnId::State,
        ];
        Self {
            columns: ColumnId::ALL
                .iter()
                .map(|&id| Column {
                    id,
                    width: id.default_width(),
                    visible: visible.contains(&id),
                })
                .collect(),
            sort: ColumnId::Cpu,
            descending: true,
            group_by: GroupBy::None,
        }
    }
}

impl TableLayout {
    fn path() -> PathBuf {
        crate::sysops::data_dir().join("native-layout.json")
    }

    pub fn load() -> Self {
        let mut layout: Self = fs::read(Self::path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        for id in ColumnId::ALL {
            if !layout.columns.iter().any(|c| c.id == id) {
                let column = Column {
                    id,
                    width: id.default_width(),
                    visible: id == ColumnId::Owner,
                };
                match (
                    id,
                    layout.columns.iter().position(|c| c.id == ColumnId::Name),
                ) {
                    (ColumnId::Owner, Some(i)) => layout.columns.insert(i + 1, column),
                    _ => layout.columns.push(column),
                }
            }
        }
        layout
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(path, json);
        }
    }

    pub fn visible(&self) -> impl Iterator<Item = (usize, &Column)> {
        self.columns.iter().enumerate().filter(|(_, c)| c.visible)
    }

    pub fn toggle(&mut self, id: ColumnId) {
        let shown = self.columns.iter().filter(|c| c.visible).count();
        if let Some(c) = self.columns.iter_mut().find(|c| c.id == id)
            && (!c.visible || shown > 1)
        {
            c.visible = !c.visible;
        }
    }

    pub fn move_column(&mut self, from: ColumnId, to: ColumnId) {
        let (Some(a), Some(b)) = (
            self.columns.iter().position(|c| c.id == from),
            self.columns.iter().position(|c| c.id == to),
        ) else {
            return;
        };
        if a != b {
            let col = self.columns.remove(a);
            self.columns.insert(b, col);
        }
    }

    pub fn set_width(&mut self, index: usize, width: f32) {
        if let Some(c) = self.columns.get_mut(index) {
            c.width = width.clamp(48., 800.);
        }
    }

    pub fn sort_by(&mut self, id: ColumnId) {
        if self.sort == id {
            self.descending = !self.descending;
        } else {
            self.sort = id;
            self.descending = id.numeric();
        }
    }
}
