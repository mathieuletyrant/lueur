//! Main loop: follows Jellyfin, loads the track of the current film and
//! streams colors to the entertainment area at 50 frames per second.

use crate::color::{Look, Rgb, linear_to_srgb};
use crate::config::Config;
use crate::hue::{Area, Bridge, Channel, Credentials, Stream};
use crate::jellyfin::{Clock, ClockEvent, Jellyfin, NowPlaying};
use crate::queue::Queue;
use crate::track::{self, Track};
use anyhow::{Context, Result};
use serde::Serialize;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{info, warn};

const FRAME: Duration = Duration::from_millis(20);
const STOP_AFTER: Duration = Duration::from_millis(1500);
const PAUSE_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_TRACK: Duration = Duration::from_secs(5);

/// For each Hue channel, the grid cells that light it and their weights.
pub struct Mapping {
    channels: Vec<(u8, Vec<(usize, f32)>)>,
}

impl Mapping {
    /// Hue position: x from -1 (left) to 1 (right), z from -1 (floor) to 1 (ceiling).
    /// Positions are stretched to the area's extent: lights (or the segments
    /// of a gradient strip) grouped around the TV still reach the screen's
    /// edges. Each channel takes a Gaussian average of the screen near its edge.
    pub fn new(channels: &[Channel], cols: u8, rows: u8) -> Self {
        let (cols, rows) = (cols as usize, rows as usize);
        let fit = |vals: Vec<f32>| {
            let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            // All on one line (a single row of lights): keep Hue's own scale.
            move |v: f32| if hi - lo > 0.2 { (v - lo) / (hi - lo) } else { ((v + 1.0) / 2.0).clamp(0.0, 1.0) }
        };
        let fx = fit(channels.iter().map(|c| c.position.x).collect());
        let fz = fit(channels.iter().map(|c| c.position.z).collect());
        let channels = channels
            .iter()
            .map(|ch| {
                let u = fx(ch.position.x);
                let v = 1.0 - fz(ch.position.z);
                let mut w = Vec::new();
                let mut total = 0.0;
                for r in 0..rows {
                    for c in 0..cols {
                        let cu = (c as f32 + 0.5) / cols as f32;
                        let cv = (r as f32 + 0.5) / rows as f32;
                        // ponytail: fixed spread, a Look setting if some setups want wider/narrower.
                        let d = ((cu - u) / 0.15).powi(2) + ((cv - v) / 0.18).powi(2);
                        let k = (-0.5 * d).exp();
                        if k > 0.01 {
                            w.push((r * cols + c, k));
                            total += k;
                        }
                    }
                }
                w.iter_mut().for_each(|(_, k)| *k /= total);
                (ch.channel_id, w)
            })
            .collect();
        Self { channels }
    }

    pub fn colors(&self, grid: &[Rgb], out: &mut Vec<(u8, Rgb)>) {
        out.clear();
        for (id, weights) in &self.channels {
            let mut c = [0f32; 3];
            for (i, k) in weights {
                for ch in 0..3 {
                    c[ch] += grid[*i][ch] * k;
                }
            }
            out.push((*id, c));
        }
    }
}

/// Per-channel exponential smoothing (the bridge smooths too, but at 25 Hz).
struct Smoother {
    state: Vec<Rgb>,
}

impl Smoother {
    fn apply(&mut self, colors: &mut [(u8, Rgb)], look: &Look, dt: f32) {
        if self.state.len() != colors.len() || look.smoothing_ms <= 0.0 {
            self.state = colors.iter().map(|c| c.1).collect();
            return;
        }
        let a = 1.0 - (-dt * 1000.0 / look.smoothing_ms).exp();
        for (s, (_, c)) in self.state.iter_mut().zip(colors.iter_mut()) {
            for ch in 0..3 {
                s[ch] += (c[ch] - s[ch]) * a;
            }
            *c = *s;
        }
    }
}

struct Session {
    stream: Stream,
}

/// What the player is doing, for the UI.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Status {
    /// Why the player is not running (not paired, bridge unreachable…).
    pub error: Option<String>,
    pub jellyfin_error: Option<String>,
    pub area: Option<String>,
    pub playing: Option<Playing>,
    pub streaming: bool,
    /// Area channels with the color they show right now.
    pub channels: Vec<LiveChannel>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Playing {
    pub title: String,
    pub device: String,
    pub position: f64,
    pub duration: Option<f64>,
    pub paused: bool,
    pub has_track: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveChannel {
    pub id: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// sRGB, black when not streaming.
    pub color: [u8; 3],
}

/// State shared between the player and the API: the Look can change while
/// a film plays, the status is read by the UI.
#[derive(Default)]
pub struct Live {
    pub look: Mutex<Look>,
    pub status: Mutex<Status>,
}

pub struct Player {
    cfg: Config,
    bridge: Arc<Bridge>,
    area: Area,
    dtls_addr: String,
    live: Arc<Live>,
    queue: Arc<Queue>,
}

impl Player {
    pub async fn new(cfg: Config, live: Arc<Live>, queue: Arc<Queue>) -> Result<Self> {
        let creds = Credentials { username: cfg.hue.username.clone(), clientkey: cfg.hue.clientkey.clone() };
        let bridge = Bridge::new(&cfg.hue.bridge, creds, cfg.hue.base_url.as_deref());
        let area = bridge.find_area(cfg.hue.area.as_deref()).await?;
        info!("area \"{}\": {} channel(s)", area.metadata.name, area.channels.len());
        let dtls_addr = format!("{}:{}", cfg.hue.bridge, cfg.hue.dtls_port.unwrap_or(2100));
        *live.status.lock().unwrap() = Status {
            area: Some(area.metadata.name.clone()),
            channels: area
                .channels
                .iter()
                .map(|c| LiveChannel {
                    id: c.channel_id,
                    x: c.position.x,
                    y: c.position.y,
                    z: c.position.z,
                    color: [0; 3],
                })
                .collect(),
            ..Default::default()
        };
        Ok(Self { cfg, bridge: Arc::new(bridge), area, dtls_addr, live, queue })
    }

    fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.live.status.lock().unwrap()
    }

    async fn open(&self) -> Result<Session> {
        self.bridge.set_streaming(&self.area.id, true).await?;
        let (addr, creds, id) = (self.dtls_addr.clone(), self.bridge.credentials().clone(), self.area.id.clone());
        let stream = tokio::task::spawn_blocking(move || Stream::connect(&addr, &creds, &id)).await??;
        info!("Entertainment stream open");
        Ok(Session { stream })
    }

    async fn close(&self, s: Session) {
        s.stream.close();
        if let Err(e) = self.bridge.set_streaming(&self.area.id, false).await {
            warn!("stopping the stream: {e:#}");
        }
        info!("stream stopped, lights back to their previous state");
    }

    /// The Track of what is playing, if it exists already.
    fn find_track(&self, np: &NowPlaying) -> Option<Arc<Track>> {
        let video = self.cfg.local_path(np.path.as_ref()?);
        let t = Track::load(&track::track_path(&self.cfg.store, &video)).ok()?;
        info!("track loaded for \"{}\" ({:.0} min)", np.name, t.duration() / 60.0);
        Some(Arc::new(t))
    }

    fn load_track(&self, np: &NowPlaying) -> Option<Arc<Track>> {
        let Some(jpath) = &np.path else {
            warn!("\"{}\": unknown path (API key without admin rights?)", np.name);
            return None;
        };
        if let Some(t) = self.find_track(np) {
            return Some(t);
        }
        let video = self.cfg.local_path(jpath);
        warn!("no track for \"{}\" (Jellyfin path: {jpath}, local path: {})", np.name, video.display());
        if !video.exists() {
            warn!("lueur can't see {}: check the Library folders", video.display());
        } else if self.cfg.auto_analyze && self.queue.push_front(video) {
            info!("analysis queued, the lights will follow as soon as it is done");
        }
        None
    }

    /// Runs until `stop` completes, then closes the stream.
    pub async fn run(self, stop: impl Future<Output = ()>) -> Result<()> {
        let jf = Arc::new(Jellyfin::new(
            &self.cfg.jellyfin.url,
            &self.cfg.jellyfin.api_key,
            self.cfg.jellyfin.device.clone(),
        ));
        let (tx, mut rx) = watch::channel::<Option<NowPlaying>>(None);
        let poller = {
            let jf = jf.clone();
            let live = self.live.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(500));
                let mut errors = 0u32;
                loop {
                    tick.tick().await;
                    match jf.now_playing().await {
                        Ok(np) => {
                            errors = 0;
                            live.status.lock().unwrap().jellyfin_error = None;
                            tx.send_replace(np);
                        }
                        Err(e) => {
                            errors += 1;
                            live.status.lock().unwrap().jellyfin_error = Some(format!("{e:#}"));
                            if errors == 1 || errors % 20 == 0 {
                                warn!("{e:#}");
                            }
                            if errors >= 4 {
                                tx.send_replace(None);
                            }
                        }
                    }
                }
            })
        };

        let mut clock = Clock::default();
        let mut current: Option<(String, Option<Arc<Track>>, Mapping)> = None;
        let mut session: Option<Session> = None;
        let mut retry_at = Instant::now();
        let mut idle_since: Option<Instant> = None;
        let mut paused_since: Option<Instant> = None;
        let mut retry_track_at = Instant::now();
        let mut grid = Vec::new();
        let mut colors = Vec::new();
        let mut smoother = Smoother { state: Vec::new() };
        let mut tick = tokio::time::interval(FRAME);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last = Instant::now();
        tokio::pin!(stop);

        info!("waiting for playback on Jellyfin…");
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = &mut stop => break,
            }
            let now = Instant::now();
            let dt = (now - last).as_secs_f32();
            last = now;

            if rx.has_changed().unwrap_or(false) {
                let np = rx.borrow_and_update().clone();
                match &np {
                    Some(np) => {
                        if current.as_ref().map(|c| &c.0) != Some(&np.item_id) {
                            info!("playing: \"{}\" on {}", np.name, np.device);
                            let t = self.load_track(np);
                            let (cols, rows) = t.as_ref().map(|t| (t.cols, t.rows)).unwrap_or((16, 9));
                            current = Some((np.item_id.clone(), t, Mapping::new(&self.area.channels, cols, rows)));
                        }
                        if let ClockEvent::Jump = clock.update(np) {
                            tracing::debug!("clock re-synced to {:.2} s", np.position);
                        }
                        paused_since = if np.paused { paused_since.or(Some(now)) } else { None };
                    }
                    None => {
                        current = None;
                        clock.reset();
                        paused_since = None;
                    }
                }
                let mut st = self.status();
                st.playing = np.map(|np| Playing {
                    title: np.name,
                    device: np.device,
                    position: np.position,
                    duration: current.as_ref().and_then(|c| c.1.as_ref()).map(|t| t.duration()),
                    paused: np.paused,
                    has_track: current.as_ref().is_some_and(|c| c.1.is_some()),
                });
            }

            // Played before its Track was ready: pick it up as soon as the Analysis ends.
            if let Some(c) = current.as_mut().filter(|c| c.1.is_none())
                && now >= retry_track_at
            {
                retry_track_at = now + RETRY_TRACK;
                let np = rx.borrow().clone().filter(|np| np.item_id == c.0);
                if let Some(t) = np.and_then(|np| self.find_track(&np)) {
                    c.2 = Mapping::new(&self.area.channels, t.cols, t.rows);
                    if let Some(p) = self.status().playing.as_mut() {
                        p.has_track = true;
                        p.duration = Some(t.duration());
                    }
                    c.1 = Some(t);
                }
            }

            let playable = current.as_ref().and_then(|(_, t, m)| t.clone().map(|t| (t, m)));
            let want = playable.is_some() && paused_since.is_none_or(|p| now - p < PAUSE_TIMEOUT);
            if !want {
                let since = *idle_since.get_or_insert(now);
                if now - since >= STOP_AFTER {
                    if let Some(s) = session.take() {
                        self.close(s).await;
                        self.set_streaming(false, &[]);
                    }
                }
                continue;
            }
            idle_since = None;

            if session.is_none() {
                if now < retry_at {
                    continue;
                }
                match self.open().await {
                    Ok(s) => session = Some(s),
                    Err(e) => {
                        warn!("opening the stream: {e:#} (retrying in 5 s)");
                        retry_at = now + Duration::from_secs(5);
                        continue;
                    }
                }
            }

            let (track, mapping) = playable.unwrap();
            let Some(pos) = clock.now() else { continue };
            let look = &self.live.look.lock().unwrap().clone();
            track.sample(pos + look.offset_ms / 1000.0, &mut grid);
            mapping.colors(&grid, &mut colors);
            for c in colors.iter_mut() {
                c.1 = look.apply(c.1);
            }
            smoother.apply(&mut colors, look, dt);
            self.set_streaming(true, &colors);
            if let Some(p) = self.status().playing.as_mut() {
                p.position = pos;
            }
            if let Err(e) = session.as_mut().unwrap().stream.send(&colors) {
                warn!("sending to the bridge: {e:#}");
                if let Some(s) = session.take() {
                    self.close(s).await;
                }
                self.set_streaming(false, &[]);
                retry_at = now + Duration::from_secs(2);
            }
        }

        poller.abort();
        if let Some(s) = session.take() {
            self.close(s).await;
        }
        self.set_streaming(false, &[]);
        Ok(())
    }

    fn set_streaming(&self, on: bool, colors: &[(u8, Rgb)]) {
        let mut st = self.status();
        st.streaming = on;
        for ch in st.channels.iter_mut() {
            let c = colors.iter().find(|c| c.0 == ch.id).map(|c| c.1).unwrap_or_default();
            ch.color = c.map(|v| (linear_to_srgb(v) * 255.0).round() as u8);
        }
    }
}

/// Test pattern: a rainbow rotating across the area's channels, or with
/// `only`, that channel blinking white and the others off (to find a light).
pub async fn test_pattern(cfg: &Config, seconds: f32, only: Option<u8>) -> Result<()> {
    let creds = Credentials { username: cfg.hue.username.clone(), clientkey: cfg.hue.clientkey.clone() };
    let bridge = Bridge::new(&cfg.hue.bridge, creds.clone(), cfg.hue.base_url.as_deref());
    let area = bridge.find_area(cfg.hue.area.as_deref()).await?;
    bridge.set_streaming(&area.id, true).await?;
    let addr = format!("{}:{}", cfg.hue.bridge, cfg.hue.dtls_port.unwrap_or(2100));
    let id = area.id.clone();
    let mut stream =
        tokio::task::spawn_blocking(move || Stream::connect(&addr, &creds, &id)).await?.context("DTLS connection")?;
    let start = Instant::now();
    let mut tick = tokio::time::interval(FRAME);
    let n = area.channels.len().max(1) as f32;
    while start.elapsed().as_secs_f32() < seconds {
        tick.tick().await;
        let t = start.elapsed().as_secs_f32();
        let colors: Vec<(u8, Rgb)> = area
            .channels
            .iter()
            .enumerate()
            .map(|(i, ch)| match only {
                Some(id) if id == ch.channel_id && t.fract() < 0.6 => (id, [1.0; 3]),
                Some(_) => (ch.channel_id, [0.0; 3]),
                None => (ch.channel_id, hue_to_rgb(t * 0.25 + i as f32 / n)),
            })
            .collect();
        stream.send(&colors)?;
    }
    stream.close();
    bridge.set_streaming(&area.id, false).await?;
    Ok(())
}

fn hue_to_rgb(h: f32) -> Rgb {
    let h = h.rem_euclid(1.0) * 6.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    let (r, g, b) = match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    };
    // sRGB → linear values, to stay consistent with the rest of the pipeline.
    [r, g, b].map(|v: f32| v.powf(2.2))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hue::Position;

    #[test]
    fn strip_segments_around_the_tv_see_their_own_edge() {
        // A gradient strip's segments, as the Hue app places them: close to the center.
        let ch = |id, x, z| Channel { channel_id: id, position: Position { x, y: 0.0, z } };
        let m = Mapping::new(
            &[ch(0, -0.37, -0.47), ch(1, -0.37, -0.1), ch(2, -0.37, 0.27), ch(3, 0.0, 0.27), ch(6, 0.37, -0.47)],
            16,
            9,
        );
        // Screen: top third red, the rest blue on the left half and green on the right half.
        let grid: Vec<Rgb> = (0..16 * 9)
            .map(|i| match (i / 16, i % 16) {
                (0..3, _) => [1.0, 0.0, 0.0],
                (_, 0..8) => [0.0, 0.0, 1.0],
                _ => [0.0, 1.0, 0.0],
            })
            .collect();
        let mut out = Vec::new();
        m.colors(&grid, &mut out);
        let c = |id: u8| out.iter().find(|c| c.0 == id).unwrap().1;
        assert!(c(0)[2] > 0.9, "bottom left is blue: {:?}", c(0));
        assert!(c(2)[0] > 0.6, "top left is mostly red: {:?}", c(2));
        assert!(c(3)[0] > 0.8, "top center is red: {:?}", c(3));
        assert!(c(6)[1] > 0.9, "bottom right is green: {:?}", c(6));
    }

    #[test]
    fn left_light_sees_left_of_screen() {
        let ch = |id, x| Channel { channel_id: id, position: Position { x, y: 0.8, z: 0.0 } };
        let m = Mapping::new(&[ch(0, -1.0), ch(1, 1.0)], 16, 9);
        // Grid: left half red, right half blue.
        let grid: Vec<Rgb> = (0..16 * 9).map(|i| if i % 16 < 8 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] }).collect();
        let mut out = Vec::new();
        m.colors(&grid, &mut out);
        assert!(out[0].1[0] > 0.95 && out[0].1[2] < 0.05, "{:?}", out[0]);
        assert!(out[1].1[2] > 0.95 && out[1].1[0] < 0.05, "{:?}", out[1]);
    }
}
