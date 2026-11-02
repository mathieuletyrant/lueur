use crate::color::Look;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    /// Folder holding the `.lueur` tracks.
    pub store: PathBuf,
    pub hue: HueConfig,
    pub jellyfin: JellyfinConfig,
    /// Jellyfin libraries lueur follows, with the folder where lueur sees them.
    pub libraries: Vec<Library>,
    /// Paths as seen by Jellyfin → paths as seen by lueur (on top of `libraries`).
    pub path_map: Vec<PathMap>,
    pub look: Look,
    /// Automatically analyze a film that is played without a track yet.
    pub auto_analyze: bool,
    /// Parallel analyses in the background queue (0 = 1).
    pub jobs: usize,
    /// Minutes between two automatic Scans (0 = off).
    pub auto_scan_minutes: u32,
    /// Videos whose path contains one of these (case-insensitive) are never analyzed.
    pub exclude: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            store: PathBuf::new(),
            hue: Default::default(),
            jellyfin: Default::default(),
            libraries: Vec::new(),
            path_map: Vec::new(),
            look: Look::default(),
            auto_analyze: true,
            jobs: 1,
            auto_scan_minutes: 15,
            // Jellyfin and Radarr/Sonarr conventions for extras.
            exclude: ["-trailer", "/trailers/", "-sample", "/extras/", "/featurettes/", "/behind the scenes/"]
                .map(String::from)
                .to_vec(),
        }
    }
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

/// One folder of a Jellyfin library: Jellyfin's path and lueur's path.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Library {
    pub name: String,
    pub jellyfin: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PathMap {
    pub from: String,
    pub to: String,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let mut c = if path.exists() {
            let s = std::fs::read_to_string(path)?;
            serde_json::from_str(&s).with_context(|| format!("reading {}", path.display()))?
        } else {
            Config::default()
        };
        if c.store.as_os_str().is_empty() {
            // Next to the config file, so a Docker volume holding the config keeps the tracks too.
            c.store = path.parent().unwrap_or(Path::new("")).join("tracks");
        }
        Ok(c)
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    pub fn local_path(&self, jellyfin_path: &str) -> PathBuf {
        let libs = self.libraries.iter().map(|l| (&l.jellyfin, &l.path));
        for (from, to) in libs.chain(self.path_map.iter().map(|m| (&m.from, &m.to))) {
            if let Some(rest) = jellyfin_path.strip_prefix(from.as_str()) {
                return PathBuf::from(format!("{to}{rest}"));
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
        assert_eq!(
            c.local_path("/media/movies/Dune (2021)/Dune.mkv"),
            PathBuf::from("/downloads/movies/Dune (2021)/Dune.mkv")
        );
        assert_eq!(c.local_path("/other/x.mkv"), PathBuf::from("/other/x.mkv"));
        let c = Config {
            libraries: vec![Library { name: "Shows".into(), jellyfin: "/media/tv".into(), path: "/data/tv".into() }],
            ..c
        };
        assert_eq!(c.local_path("/media/tv/Lost/s01e01.mkv"), PathBuf::from("/data/tv/Lost/s01e01.mkv"));
    }
}
