use crate::color::Look;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Folder holding the `.lueur` tracks.
    pub store: PathBuf,
    pub hue: HueConfig,
    pub jellyfin: JellyfinConfig,
    /// Paths as seen by Jellyfin → paths as seen by lueur.
    pub path_map: Vec<PathMap>,
    pub look: Look,
    /// Automatically analyze a film that is played without a track yet.
    pub auto_analyze: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct HueConfig {
    pub bridge: String,
    pub username: String,
    pub clientkey: String,
    /// Entertainment area name or id.
    pub area: Option<String>,
    /// DTLS port (2100 on a real bridge).
    pub dtls_port: Option<u16>,
    /// Replacement REST URL (tests).
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct JellyfinConfig {
    pub url: String,
    pub api_key: String,
    /// Only follow one device or client (substring, e.g. "Apple TV").
    pub device: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathMap {
    pub from: String,
    pub to: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self { store: PathBuf::from("tracks"), ..Default::default() });
        }
        let s = std::fs::read_to_string(path)?;
        let mut c: Config = serde_json::from_str(&s).with_context(|| format!("reading {}", path.display()))?;
        if c.store.as_os_str().is_empty() {
            c.store = PathBuf::from("tracks");
        }
        Ok(c)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    pub fn local_path(&self, jellyfin_path: &str) -> PathBuf {
        for m in &self.path_map {
            if let Some(rest) = jellyfin_path.strip_prefix(&m.from) {
                return PathBuf::from(format!("{}{rest}", m.to));
            }
        }
        PathBuf::from(jellyfin_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_mapping() {
        let c = Config {
            path_map: vec![PathMap { from: "/media/movies".into(), to: "/downloads/movies".into() }],
            ..Default::default()
        };
        assert_eq!(c.local_path("/media/movies/Dune (2021)/Dune.mkv"), PathBuf::from("/downloads/movies/Dune (2021)/Dune.mkv"));
        assert_eq!(c.local_path("/other/x.mkv"), PathBuf::from("/other/x.mkv"));
    }
}
