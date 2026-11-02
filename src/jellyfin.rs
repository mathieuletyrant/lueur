//! Suivi de ce qui joue sur Jellyfin, et horloge de lecture locale.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Session {
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    client: String,
    #[serde(default)]
    now_playing_item: Option<Item>,
    #[serde(default)]
    play_state: Option<PlayState>,
    #[serde(default)]
    last_playback_check_in: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Item {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    path: Option<String>,
    #[serde(default, rename = "Type")]
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlayState {
    #[serde(default)]
    position_ticks: Option<i64>,
    #[serde(default)]
    is_paused: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub item_id: String,
    pub name: String,
    pub path: Option<String>,
    pub device: String,
    /// Position estimée au moment `at`, en secondes.
    pub position: f64,
    pub paused: bool,
    pub at: Instant,
}

pub struct Jellyfin {
    url: String,
    key: String,
    device: Option<String>,
    http: reqwest::Client,
    paths: Mutex<HashMap<String, Option<String>>>,
}

impl Jellyfin {
    pub fn new(url: &str, key: &str, device: Option<String>) -> Self {
        Self {
            url: url.trim_end_matches('/').into(),
            key: key.into(),
            device: device.map(|d| d.to_lowercase()),
            http: reqwest::Client::builder().timeout(Duration::from_secs(4)).build().unwrap(),
            paths: Mutex::new(HashMap::new()),
        }
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.http
            .get(format!("{}{path}", self.url))
            .header("Authorization", format!("MediaBrowser Token=\"{}\"", self.key))
    }

    pub async fn now_playing(&self) -> Result<Option<NowPlaying>> {
        let sessions: Vec<Session> = self
            .get("/Sessions?activeWithinSeconds=120")
            .send()
            .await
            .context("Jellyfin injoignable")?
            .error_for_status()?
            .json()
            .await?;
        let now_wall = unix_now();
        let at = Instant::now();
        for s in sessions {
            let (Some(item), Some(ps)) = (s.now_playing_item, s.play_state) else { continue };
            if !matches!(item.kind.as_str(), "Movie" | "Episode" | "Video" | "MusicVideo") {
                continue;
            }
            if let Some(d) = &self.device {
                if !s.device_name.to_lowercase().contains(d) && !s.client.to_lowercase().contains(d) {
                    continue;
                }
            }
            let mut position = ps.position_ticks.unwrap_or(0) as f64 / 1e7;
            // Les clients ne rapportent leur position que toutes les quelques
            // secondes : on extrapole depuis le dernier rapport.
            if !ps.is_paused {
                if let Some(t) = s.last_playback_check_in.as_deref().and_then(parse_iso8601) {
                    let dt = now_wall - t;
                    if (0.0..6.0 * 3600.0).contains(&dt) {
                        position += dt;
                    }
                }
            }
            let path = match item.path {
                Some(p) => Some(p),
                None => self.item_path(&item.id).await,
            };
            return Ok(Some(NowPlaying {
                item_id: item.id,
                name: item.name,
                path,
                device: s.device_name,
                position,
                paused: ps.is_paused,
                at,
            }));
        }
        Ok(None)
    }

    /// Certaines versions n'incluent pas le chemin dans la session.
    async fn item_path(&self, id: &str) -> Option<String> {
        if let Some(p) = self.paths.lock().await.get(id) {
            return p.clone();
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Items {
            items: Vec<Item>,
        }
        let p = async {
            let r: Items = self.get(&format!("/Items?ids={id}&fields=Path")).send().await.ok()?.json().await.ok()?;
            r.items.into_iter().next()?.path
        }
        .await;
        self.paths.lock().await.insert(id.into(), p.clone());
        p
    }
}

fn unix_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()
}

/// `2026-11-02T20:15:30.1234567Z` → secondes Unix (UTC seulement, ce que renvoie Jellyfin).
pub fn parse_iso8601(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let time = time.split(['+']).next()?;
    let mut t = time.split(':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss: f64 = t.next()?.parse().ok()?;
    // Jours depuis 1970 (algorithme de Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hh * 3600 + mm * 60) as f64 + ss)
}

/// Horloge de lecture locale, recalée sur Jellyfin.
/// Une fois calée, elle ne suit plus chaque rapport (bruités) : elle ne se
/// recale que sur une vraie avance/retour, une pause ou un changement de
/// média, et corrige doucement les petites dérives. Les lampes restent fluides.
#[derive(Debug, Default)]
pub struct Clock {
    item: Option<String>,
    anchor: Option<(Instant, f64)>,
    paused: bool,
}

pub enum ClockEvent {
    None,
    Jump,
    Slew,
}

impl Clock {
    pub fn update(&mut self, np: &NowPlaying) -> ClockEvent {
        let expected = self.at(np.at);
        let changed = self.item.as_deref() != Some(&np.item_id) || self.paused != np.paused;
        let err = expected.map(|e| np.position - e).unwrap_or(f64::INFINITY);
        self.item = Some(np.item_id.clone());
        self.paused = np.paused;
        if changed || err.abs() > 1.5 {
            self.anchor = Some((np.at, np.position));
            ClockEvent::Jump
        } else if err.abs() > 0.25 {
            let (i, p) = self.anchor.unwrap();
            self.anchor = Some((i, p + err * 0.2));
            ClockEvent::Slew
        } else {
            ClockEvent::None
        }
    }

    pub fn at(&self, when: Instant) -> Option<f64> {
        let (i, p) = self.anchor?;
        if self.paused {
            return Some(p);
        }
        Some(p + when.saturating_duration_since(i).as_secs_f64())
    }

    pub fn now(&self) -> Option<f64> {
        self.at(Instant::now())
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso() {
        assert_eq!(parse_iso8601("1970-01-01T00:00:00Z"), Some(0.0));
        let t = parse_iso8601("2026-11-02T20:15:30.5000000Z").unwrap();
        assert!((t - 1_793_650_530.5).abs() < 1e-6, "{t}");
    }

    #[test]
    fn clock_locks_and_jumps() {
        let t0 = Instant::now();
        let np = |pos: f64, dt: f64, paused: bool| NowPlaying {
            item_id: "a".into(),
            name: String::new(),
            path: None,
            device: String::new(),
            position: pos,
            paused,
            at: t0 + Duration::from_secs_f64(dt),
        };
        let mut c = Clock::default();
        assert!(matches!(c.update(&np(100.0, 0.0, false)), ClockEvent::Jump));
        // Rapport légèrement bruité : ignoré.
        assert!(matches!(c.update(&np(110.1, 10.0, false)), ClockEvent::None));
        assert!((c.at(t0 + Duration::from_secs(10)).unwrap() - 110.0).abs() < 1e-9);
        // Avance rapide : recalage immédiat.
        assert!(matches!(c.update(&np(300.0, 11.0, false)), ClockEvent::Jump));
        // Pause : l'horloge s'arrête.
        c.update(&np(301.0, 12.0, true));
        assert_eq!(c.at(t0 + Duration::from_secs(60)), Some(301.0));
    }
}
