//! Main loop: follows Jellyfin, loads the track of the current film and
//! streams colors to the entertainment area at 50 frames per second.

use crate::analyze;
use crate::color::{Look, Rgb};
use crate::config::Config;
use crate::hue::{Area, Bridge, Channel, Credentials, Stream};
use crate::jellyfin::{Clock, ClockEvent, Jellyfin, NowPlaying};
use crate::track::{self, Track};
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{info, warn};

const FRAME: Duration = Duration::from_millis(20);
const STOP_AFTER: Duration = Duration::from_millis(1500);
const PAUSE_TIMEOUT: Duration = Duration::from_secs(30);

/// For each Hue channel, the grid cells that light it and their weights.
pub struct Mapping {
    channels: Vec<(u8, Vec<(usize, f32)>)>,
}

impl Mapping {
    /// Hue position: x from -1 (left) to 1 (right), z from -1 (floor) to 1 (ceiling).
    /// Each channel takes a Gaussian average around the matching screen point.
    pub fn new(channels: &[Channel], cols: u8, rows: u8) -> Self {
        let (cols, rows) = (cols as usize, rows as usize);
        let channels = channels
            .iter()
            .map(|ch| {
                let u = ((ch.position.x + 1.0) / 2.0).clamp(0.0, 1.0);
                let v = ((1.0 - ch.position.z) / 2.0).clamp(0.0, 1.0);
                let mut w = Vec::new();
                let mut total = 0.0;
                for r in 0..rows {
                    for c in 0..cols {
                        let cu = (c as f32 + 0.5) / cols as f32;
                        let cv = (r as f32 + 0.5) / rows as f32;
                        let d = ((cu - u) / 0.2).powi(2) + ((cv - v) / 0.35).powi(2);
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

pub struct Player {
    cfg: Config,
    bridge: Arc<Bridge>,
    area: Area,
    dtls_addr: String,
    analyzing: Arc<Mutex<HashSet<PathBuf>>>,
}

impl Player {
    pub async fn new(cfg: Config) -> Result<Self> {
        let creds = Credentials { username: cfg.hue.username.clone(), clientkey: cfg.hue.clientkey.clone() };
        let bridge = Bridge::new(&cfg.hue.bridge, creds, cfg.hue.base_url.as_deref());
        let area = bridge.find_area(cfg.hue.area.as_deref()).await?;
        info!("area \"{}\": {} channel(s)", area.metadata.name, area.channels.len());
        let dtls_addr = format!("{}:{}", cfg.hue.bridge, cfg.hue.dtls_port.unwrap_or(2100));
        Ok(Self { cfg, bridge: Arc::new(bridge), area, dtls_addr, analyzing: Default::default() })
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

    fn load_track(&self, np: &NowPlaying) -> Option<Arc<Track>> {
        let Some(jpath) = &np.path else {
            warn!("\"{}\": unknown path (API key without admin rights?)", np.name);
            return None;
        };
        let video = self.cfg.local_path(jpath);
        let tp = track::track_path(&self.cfg.store, &video);
        match Track::load(&tp) {
            Ok(t) => {
                info!("track loaded for \"{}\" ({:.0} min)", np.name, t.duration() / 60.0);
                return Some(Arc::new(t));
            }
            Err(_) => warn!("no track for \"{}\" (Jellyfin path: {jpath}, local path: {})", np.name, video.display()),
        }
        if self.cfg.auto_analyze && self.analyzing.lock().unwrap().insert(video.clone()) {
            let (store, set) = (self.cfg.store.clone(), self.analyzing.clone());
            info!("analysis started in the background, the lights will follow on the next playback");
            std::thread::spawn(move || {
                let opts = analyze::Options { fps: 12.0, cols: 16, rows: 9, hwaccel: analyze::HwAccel::Auto, threads: 2 };
                match analyze::analyze(&video, &opts) {
                    Ok((t, _)) => {
                        if let Err(e) = t.save(&track::track_path(&store, &video)) {
                            warn!("saving the track: {e:#}");
                        }
                    }
                    Err(e) => warn!("analyzing {}: {e:#}", video.display()),
                }
                set.lock().unwrap().remove(&video);
            });
        }
        None
    }

    pub async fn run(self) -> Result<()> {
        let jf = Arc::new(Jellyfin::new(&self.cfg.jellyfin.url, &self.cfg.jellyfin.api_key, self.cfg.jellyfin.device.clone()));
        let (tx, mut rx) = watch::channel::<Option<NowPlaying>>(None);
        let poller = {
            let jf = jf.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_millis(500));
                let mut errors = 0u32;
                loop {
                    tick.tick().await;
                    match jf.now_playing().await {
                        Ok(np) => {
                            errors = 0;
                            tx.send_replace(np);
                        }
                        Err(e) => {
                            errors += 1;
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
        let mut grid = Vec::new();
        let mut colors = Vec::new();
        let mut smoother = Smoother { state: Vec::new() };
        let mut tick = tokio::time::interval(FRAME);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut last = Instant::now();
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::pin!(ctrl_c);

        info!("waiting for playback on Jellyfin…");
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = &mut ctrl_c => break,
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
            }

            let playable = current.as_ref().and_then(|(_, t, m)| t.clone().map(|t| (t, m)));
            let want = playable.is_some() && paused_since.is_none_or(|p| now - p < PAUSE_TIMEOUT);
            if !want {
                let since = *idle_since.get_or_insert(now);
                if now - since >= STOP_AFTER {
                    if let Some(s) = session.take() {
                        self.close(s).await;
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
            let look = &self.cfg.look;
            track.sample(pos + look.offset_ms / 1000.0, &mut grid);
            mapping.colors(&grid, &mut colors);
            for c in colors.iter_mut() {
                c.1 = look.apply(c.1);
            }
            smoother.apply(&mut colors, look, dt);
            if let Err(e) = session.as_mut().unwrap().stream.send(&colors) {
                warn!("sending to the bridge: {e:#}");
                if let Some(s) = session.take() {
                    self.close(s).await;
                }
                retry_at = now + Duration::from_secs(2);
            }
        }

        poller.abort();
        if let Some(s) = session.take() {
            self.close(s).await;
        }
        Ok(())
    }
}

/// Test pattern: a rainbow rotating across the area's channels.
pub async fn test_pattern(cfg: &Config, seconds: f32) -> Result<()> {
    let creds = Credentials { username: cfg.hue.username.clone(), clientkey: cfg.hue.clientkey.clone() };
    let bridge = Bridge::new(&cfg.hue.bridge, creds.clone(), cfg.hue.base_url.as_deref());
    let area = bridge.find_area(cfg.hue.area.as_deref()).await?;
    bridge.set_streaming(&area.id, true).await?;
    let addr = format!("{}:{}", cfg.hue.bridge, cfg.hue.dtls_port.unwrap_or(2100));
    let id = area.id.clone();
    let mut stream = tokio::task::spawn_blocking(move || Stream::connect(&addr, &creds, &id))
        .await?
        .context("DTLS connection")?;
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
            .map(|(i, ch)| (ch.channel_id, hue_to_rgb(t * 0.25 + i as f32 / n)))
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
