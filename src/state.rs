use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::TelesyncError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncState {
    pub pages: HashMap<String, PageState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageState {
    pub publication: String,
    pub telegraph_path: String,
    pub telegraph_url: String,
    pub content_hash: String,
    pub title: String,
    pub last_synced: String,
    pub status: PageStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageStatus {
    Published,
    Deleted,
}

impl Serialize for PageStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            PageStatus::Published => serializer.serialize_str("published"),
            PageStatus::Deleted => serializer.serialize_str("deleted"),
        }
    }
}

impl<'de> Deserialize<'de> for PageStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "published" => Ok(PageStatus::Published),
            "deleted" => Ok(PageStatus::Deleted),
            other => Err(serde::de::Error::custom(format!(
                "unknown page status: '{}'",
                other
            ))),
        }
    }
}

impl SyncState {
    pub fn load(path: &Path) -> Result<SyncState, TelesyncError> {
        if !path.exists() {
            return Ok(SyncState {
                pages: HashMap::new(),
            });
        }

        let content = fs::read_to_string(path).map_err(|e| {
            TelesyncError::State(format!(
                "failed to read state file {}: {}",
                path.display(),
                e
            ))
        })?;

        let state: SyncState = serde_json::from_str(&content).map_err(|e| {
            TelesyncError::State(format!(
                "failed to parse state file {}: {}",
                path.display(),
                e
            ))
        })?;

        Ok(state)
    }

    pub fn save(&self, path: &Path) -> Result<(), TelesyncError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| TelesyncError::State(format!("failed to serialize state: {}", e)))?;

        let tmp_path = path.with_extension("json.tmp");

        fs::write(&tmp_path, json.as_bytes()).map_err(|e| {
            TelesyncError::State(format!(
                "failed to write temporary state file {}: {}",
                tmp_path.display(),
                e
            ))
        })?;

        fs::rename(&tmp_path, path).map_err(|e| {
            TelesyncError::State(format!(
                "failed to rename {} -> {}: {}",
                tmp_path.display(),
                path.display(),
                e
            ))
        })?;

        Ok(())
    }
}

pub struct Lockfile {
    path: PathBuf,
}

impl Lockfile {
    pub fn acquire(path: &Path) -> Result<Lockfile, TelesyncError> {
        if path.exists() {
            let content = fs::read_to_string(path).map_err(|e| {
                TelesyncError::State(format!("failed to read lockfile {}: {}", path.display(), e))
            })?;

            if let Ok(pid) = content.trim().parse::<u32>()
                && is_pid_alive(pid) {
                    return Err(TelesyncError::Locked { pid });
                }

            // Stale lockfile -- PID is not running, remove it
            fs::remove_file(path).map_err(|e| {
                TelesyncError::State(format!(
                    "failed to remove stale lockfile {}: {}",
                    path.display(),
                    e
                ))
            })?;
        }

        let pid = std::process::id();
        fs::write(path, pid.to_string()).map_err(|e| {
            TelesyncError::State(format!(
                "failed to write lockfile {}: {}",
                path.display(),
                e
            ))
        })?;

        Ok(Lockfile {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for Lockfile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn is_pid_alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{}", pid)).exists()
}
