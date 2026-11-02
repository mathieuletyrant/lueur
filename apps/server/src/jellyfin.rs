//! Tracks what is playing on Jellyfin, and the local playback clock.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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
    /// Estimated position at time `at`, in seconds.
    pub position: f64,
    pub paused: bool,
    pub at: Instant,
}

#[derive(Debug, Serialize)]
pub struct JellyfinLibrary {
    pub name: String,
    pub kind: String,
    pub locations: Vec<String>,
}

pub struct Jellyfin {
    url: String,
    key: String,
    device: Option<String>,
    http: reqwest::Client,
    paths: Mutex<HashMap<String, Option<String>>>,
    seen: Mutex<Option<Seen>>,
}

/// The last position report we saw change, and when we saw it.
#[derive(Debug, Clone)]
struct Seen {
    item: String,
    ticks: i64,
    at: Instant,
}

/// Position now, in seconds. Clients only report every few seconds, so we
/// extrapolate from the moment *we* saw the reported position change (one
/// poll late at most). `LastPlaybackCheckIn` is only used the first time:
/// clients update it on their own schedule, some seconds ahead of the position.
fn estimate(
    seen: &mut Option<Seen>,
    item: &str,
    ticks: i64,
    paused: bool,
    checkin_age: Option<f64>,
    now: Instant,
) -> f64 {
    let position = ticks as f64 / 1e7;
    match seen {
        // Paused: the position doesn't move; restart from here on resume.
        Some(s) if s.item == item && paused => {
            *s = Seen { item: item.into(), ticks, at: now };
            position
        }
        Some(s) if s.item == item && s.ticks == ticks => position + now.saturating_duration_since(s.at).as_secs_f64(),
        Some(s) if s.item == item => {
            *s = Seen { item: item.into(), ticks, at: now };
            position
        }
        _ => {
            let age = if paused { 0.0 } else { checkin_age.filter(|a| (0.0..6.0 * 3600.0).contains(a)).unwrap_or(0.0) };
            let at = now.checked_sub(Duration::from_secs_f64(age)).unwrap_or(now);
            *seen = Some(Seen { item: item.into(), ticks, at });
            position + age
        }
    }
}

impl Jellyfin {
    pub fn new(url: &str, key: &str, device: Option<String>) -> Self {
        Self {
            url: url.trim_end_matches('/').into(),
            key: key.into(),
            device: device.map(|d| d.to_lowercase()),
            http: reqwest::Client::builder().timeout(Duration::from_secs(4)).build().unwrap(),
            paths: Mutex::new(HashMap::new()),
            seen: Mutex::new(None),
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
            .context("Jellyfin unreachable")?
            .error_for_status()?
            .json()
            .await?;
        Ok(self.pick(sessions).await)
    }

    /// Follows Jellyfin's WebSocket, which pushes the sessions on every
    /// playback report (pause, seek, progress) instead of us polling for
    /// them. Calls `on` for each update; returns when the socket closes.
    pub async fn follow(&self, mut on: impl FnMut(Option<NowPlaying>)) -> Result<()> {
        use futures_util::{SinkExt, StreamExt};
        use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};
        let url = format!("{}/socket?api_key={}&deviceId=lueur", self.url.replacen("http", "ws", 1), self.key);
        let mut req = url.into_client_request()?;
        req.headers_mut().insert("Authorization", format!("MediaBrowser Token=\"{}\"", self.key).parse()?);
        let (mut ws, _) = tokio_tungstenite::connect_async(req).await.context("Jellyfin websocket")?;
        // Sessions right away, then at least every second.
        ws.send(Message::text(r#"{"MessageType":"SessionsStart","Data":"0,1000"}"#)).await?;
        let mut keepalive = tokio::time::interval(Duration::from_secs(30));
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Envelope {
            message_type: String,
            #[serde(default)]
            data: Value,
        }
        loop {
            tokio::select! {
                _ = keepalive.tick() => ws.send(Message::text(r#"{"MessageType":"KeepAlive"}"#)).await?,
                msg = ws.next() => match msg.context("Jellyfin websocket closed")?? {
                    Message::Text(t) => {
                        let e: Envelope = serde_json::from_str(&t)?;
                        if e.message_type == "Sessions" {
                            on(self.pick(serde_json::from_value(e.data)?).await);
                        }
                    }
                    Message::Close(_) => anyhow::bail!("Jellyfin websocket closed"),
                    _ => {}
                },
            }
        }
    }

    /// What the followed device plays, among `sessions`.
    async fn pick(&self, sessions: Vec<Session>) -> Option<NowPlaying> {
        let now_wall = unix_now();
        let at = Instant::now();
        for s in sessions {
            let (Some(item), Some(ps)) = (s.now_playing_item, s.play_state) else {
                continue;
            };
            if !matches!(item.kind.as_str(), "Movie" | "Episode" | "Video" | "MusicVideo") {
                continue;
            }
            if let Some(d) = &self.device {
                if !s.device_name.to_lowercase().contains(d) && !s.client.to_lowercase().contains(d) {
                    continue;
                }
            }
            let checkin_age = s.last_playback_check_in.as_deref().and_then(parse_iso8601).map(|t| now_wall - t);
            let ticks = ps.position_ticks.unwrap_or(0);
            let position = estimate(&mut *self.seen.lock().await, &item.id, ticks, ps.is_paused, checkin_age, at);
            let path = match item.path {
                Some(p) => Some(p),
                None => self.item_path(&item.id).await,
            };
            return Some(NowPlaying {
                item_id: item.id,
                name: item.name,
                path,
                device: s.device_name,
                position,
                paused: ps.is_paused,
                at,
            });
        }
        None
    }

    /// Server name and version: checks the URL and the key.
    pub async fn info(&self) -> Result<Value> {
        Ok(self.get("/System/Info").send().await.context("Jellyfin unreachable")?.error_for_status()?.json().await?)
    }

    /// Names of the devices known to Jellyfin, most recently used first, for the device filter.
    /// (`/Sessions` only lists clients connected right now.)
    pub async fn devices(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Device {
            #[serde(default)]
            name: String,
            #[serde(default)]
            date_last_activity: String,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Devices {
            items: Vec<Device>,
        }
        let mut d: Devices = self.get("/Devices").send().await?.error_for_status()?.json().await?;
        // ISO 8601 dates sort as strings.
        d.items.sort_by(|a, b| b.date_last_activity.cmp(&a.date_last_activity));
        let mut names: Vec<String> = Vec::new();
        for n in d.items.into_iter().map(|d| d.name) {
            if !n.is_empty() && !names.contains(&n) {
                names.push(n);
            }
        }
        Ok(names)
    }

    /// Video libraries (movies, shows, mixed) and their folders.
    pub async fn libraries(&self) -> Result<Vec<JellyfinLibrary>> {
        #[derive(Deserialize)]
        #[serde(rename_all = "PascalCase")]
        struct Folder {
            name: String,
            #[serde(default)]
            collection_type: Option<String>,
            #[serde(default)]
            locations: Vec<String>,
        }
        let folders: Vec<Folder> = self.get("/Library/VirtualFolders").send().await?.error_for_status()?.json().await?;
        Ok(folders
            .into_iter()
            .filter_map(|f| {
                let kind = f.collection_type.unwrap_or_else(|| "mixed".into());
                matches!(kind.as_str(), "movies" | "tvshows" | "mixed").then_some(JellyfinLibrary {
                    name: f.name,
                    kind,
                    locations: f.locations,
                })
            })
            .collect())
    }

    /// Some versions do not include the path in the session.
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

/// `2026-11-02T20:15:30.1234567Z` → Unix seconds (UTC only, which is what Jellyfin returns).
pub fn parse_iso8601(s: &str) -> Option<f64> {
    let s = s.trim().trim_end_matches('Z');
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-').map(|x| x.parse::<i64>());
    let (y, m, day) = (d.next()?.ok()?, d.next()?.ok()?, d.next()?.ok()?);
    let time = time.split(['+']).next()?;
    let mut t = time.split(':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss: f64 = t.next()?.parse().ok()?;
    // Days since 1970 (Howard Hinnant's algorithm).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hh * 3600 + mm * 60) as f64 + ss)
}

/// Local playback clock, kept in step with Jellyfin.
/// Once locked it no longer follows every (noisy) report: it only re-syncs
/// on a real seek, a pause or a media change, and gently corrects small
/// drifts. The lights stay smooth.
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
    fn position_extrapolates_from_when_the_report_changed() {
        let t0 = Instant::now();
        let at = |s: f64| t0 + Duration::from_secs_f64(s);
        let mut seen = None;
        // First sight: trust the check-in age once.
        assert_eq!(estimate(&mut seen, "a", 100 * 10_000_000, false, Some(2.0), at(0.0)), 102.0);
        // Same report 3 s later: extrapolate.
        assert_eq!(estimate(&mut seen, "a", 100 * 10_000_000, false, Some(2.0), at(3.0)), 105.0);
        // New report: it is the position now, whatever the check-in says.
        assert_eq!(estimate(&mut seen, "a", 106 * 10_000_000, false, Some(9.0), at(6.0)), 106.0);
        assert_eq!(estimate(&mut seen, "a", 106 * 10_000_000, false, None, at(7.5)), 107.5);
        // Paused for a minute, then resumed with the same report: no jump.
        assert_eq!(estimate(&mut seen, "a", 107 * 10_000_000, true, None, at(8.0)), 107.0);
        assert_eq!(estimate(&mut seen, "a", 107 * 10_000_000, true, None, at(68.0)), 107.0);
        assert_eq!(estimate(&mut seen, "a", 107 * 10_000_000, false, None, at(69.0)), 108.0);
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
        // Slightly noisy report: ignored.
        assert!(matches!(c.update(&np(110.1, 10.0, false)), ClockEvent::None));
        assert!((c.at(t0 + Duration::from_secs(10)).unwrap() - 110.0).abs() < 1e-9);
        // Seek: immediate re-sync.
        assert!(matches!(c.update(&np(300.0, 11.0, false)), ClockEvent::Jump));
        // Pause: the clock stops.
        c.update(&np(301.0, 12.0, true));
        assert_eq!(c.at(t0 + Duration::from_secs(60)), Some(301.0));
    }
}
