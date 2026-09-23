//! Flat-file diagram storage: one pretty-printed `<id>.json` per diagram.

use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use infraplot_model::Diagram;
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

#[derive(Debug, Serialize)]
pub struct Summary {
    pub id: String,
    pub title: String,
    /// Seconds since the Unix epoch.
    pub updated_at: u64,
}

/// Ids double as file names, so keep them boring.
pub fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    (1..=64).contains(&id.len())
        && chars
            .next()
            .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

impl Store {
    pub async fn open(dir: &Path) -> std::io::Result<Self> {
        tokio::fs::create_dir_all(dir).await?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    fn path(&self, id: &str) -> PathBuf {
        debug_assert!(valid_id(id));
        self.dir.join(format!("{id}.json"))
    }

    pub async fn list(&self) -> std::io::Result<Vec<Summary>> {
        let mut out = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .filter(|_| path.extension().is_some_and(|e| e == "json"))
                .filter(|id| valid_id(id))
            else {
                continue;
            };
            let id = id.to_owned();
            match self.get(&id).await {
                Ok(Some((diagram, updated_at))) => out.push(Summary {
                    id,
                    title: diagram.title,
                    updated_at,
                }),
                Ok(None) => {}
                Err(err) => tracing::warn!(%id, %err, "skipping unreadable diagram"),
            }
        }
        out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.id.cmp(&b.id)));
        Ok(out)
    }

    pub async fn get(&self, id: &str) -> std::io::Result<Option<(Diagram, u64)>> {
        let path = self.path(id);
        let raw = match tokio::fs::read_to_string(&path).await {
            Ok(raw) => raw,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let diagram =
            Diagram::from_json(&raw).map_err(|e| std::io::Error::new(ErrorKind::InvalidData, e))?;
        let modified = tokio::fs::metadata(&path).await?.modified()?;
        let updated_at = modified
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Ok(Some((diagram, updated_at)))
    }

    /// Atomically replaces the stored diagram.
    pub async fn put(&self, id: &str, diagram: &Diagram) -> std::io::Result<()> {
        let path = self.path(id);
        let tmp = self.dir.join(format!(".{id}.json.tmp"));
        tokio::fs::write(&tmp, diagram.to_json()).await?;
        tokio::fs::rename(&tmp, &path).await
    }

    pub async fn delete(&self, id: &str) -> std::io::Result<bool> {
        match tokio::fs::remove_file(self.path(id)).await {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(false),
            Err(e) => Err(e),
        }
    }
}
