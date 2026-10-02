//! Tiny persisted preferences (`$XDG_CONFIG_HOME/infra-plot/settings.toml`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
}

fn path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("infra-plot").join("settings.toml"))
}

impl Settings {
    pub fn load() -> Self {
        path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// Best effort: preferences are not worth an error dialog.
    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let (Some(dir), Ok(s)) = (p.parent(), toml::to_string(self))
            && std::fs::create_dir_all(dir).is_ok()
        {
            let _ = std::fs::write(p, s);
        }
    }
}
