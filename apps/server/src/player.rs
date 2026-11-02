//! Main loop: follows Jellyfin, loads the track of the current film and
//! streams colors to the entertainment area at 50 frames per second.

use crate::color::{Look, Rgb, linear_to_srgb};
use crate::config::Config;
use crate::hue::{Area, Bridge, Stream};
use crate::jellyfin::{Clock, ClockEvent, Jellyfin, NowPlaying};
use crate::queue::Queue;
use crate::render::Renderer;
use crate::track::{self, Track};
use anyhow::Result;
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
    /// How late the app's check-in runs, as learned (seconds).
    pub report_delay: Option<f64>,
    /// Brightness gain applied to this film (auto exposure).
    pub exposure: Option<f32>,
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

impl Live {
    /// Publishes what the lights show (black when `colors` misses a channel).
    pub fn set_streaming(&self, on: bool, colors: &[(u8, Rgb)]) {
        let mut st = self.status.lock().unwrap();
        st.streaming = on;
        for ch in st.channels.iter_mut() {
            let c = colors.iter().find(|c| c.0 == ch.id).map(|c| c.1).unwrap_or_default();
            ch.color = c.map(|v| (linear_to_srgb(v) * 255.0).round() as u8);
        }
    }
}

pub struct Player {
    cfg: Config,
    bridge: Bridge,
    area: Area,
    live: Arc<Live>,
    queue: Arc<Queue>,
}

impl Player {
    pub async fn new(cfg: Config, live: Arc<Live>, queue: Arc<Queue>) -> Result<Self> {
        let bridge = Bridge::new(&cfg.hue);
        let area = bridge.find_area(cfg.hue.area.as_deref()).await?;
        info!("area \"{}\": {} channel(s)", area.metadata.name, area.channels.len());
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
        Ok(Self { cfg, bridge, area, live, queue })
    }

    fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.live.status.lock().unwrap()
    }

    async fn open(&self) -> Result<Session> {
        let stream = self.bridge.open_stream(&self.area.id).await?;
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
        } else if self.cfg.auto_analyze
            && !crate::queue::excluded(&video, &self.cfg.exclude)
            && self.queue.push_front(video)
        {
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
            self.cfg.sync.learn_report_delay,
        ));
        let (tx, mut rx) = watch::channel::<Option<NowPlaying>>(None);
        let poller = {
            let jf = jf.clone();
            let live = self.live.clone();
            tokio::spawn(async move {
                let mut errors = 0u32;
                loop {
                    // Pushed by Jellyfin's WebSocket; polled for 30 s whenever it is down.
                    let pushed = jf
                        .follow(|np| {
                            live.status.lock().unwrap().jellyfin_error = None;
                            tx.send_replace(np);
                        })
                        .await;
                    if let Err(e) = pushed {
                        tracing::debug!("{e:#}, polling instead");
                    }
                    let mut tick = tokio::time::interval(Duration::from_millis(500));
                    let until = Instant::now() + Duration::from_secs(30);
                    while Instant::now() < until {
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
                                if errors == 1 || errors.is_multiple_of(20) {
                                    warn!("{e:#}");
                                }
                                if errors >= 4 {
                                    tx.send_replace(None);
                                }
                            }
                        }
                    }
                }
            })
        };

        let mut clock = Clock::new(self.cfg.sync.jump_ms / 1000.0, self.cfg.sync.drift_ms / 1000.0);
        let mut current: Option<(String, Option<Arc<Track>>)> = None;
        let mut session: Option<Session> = None;
        let mut retry_at = Instant::now();
        let mut idle_since: Option<Instant> = None;
        let mut paused_since: Option<Instant> = None;
        let mut retry_track_at = Instant::now();
        let mut renderer = Renderer::new(&self.area.channels);
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
                            current = Some((np.item_id.clone(), self.load_track(np)));
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
                    report_delay: np.report_delay,
                    exposure: None,
                });
            }

            // Played before its Track was ready: pick it up as soon as the Analysis ends.
            if let Some(c) = current.as_mut().filter(|c| c.1.is_none())
                && now >= retry_track_at
            {
                retry_track_at = now + RETRY_TRACK;
                let np = rx.borrow().clone().filter(|np| np.item_id == c.0);
                if let Some(t) = np.and_then(|np| self.find_track(&np)) {
                    if let Some(p) = self.status().playing.as_mut() {
                        p.has_track = true;
                        p.duration = Some(t.duration());
                    }
                    c.1 = Some(t);
                }
            }

            let playable = current.as_ref().and_then(|(_, t)| t.clone());
            let want = playable.is_some() && paused_since.is_none_or(|p| now - p < PAUSE_TIMEOUT);
            if !want {
                let since = *idle_since.get_or_insert(now);
                if now - since >= STOP_AFTER
                    && let Some(s) = session.take()
                {
                    self.close(s).await;
                    self.set_streaming(false, &[]);
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

            let track = playable.unwrap();
            let Some(pos) = clock.now() else { continue };
            let look = self.live.look.lock().unwrap().clone();
            let colors = renderer.colors(&track, pos + look.offset_ms / 1000.0, &look, dt);
            self.set_streaming(true, colors);
            if let Some(p) = self.status().playing.as_mut() {
                p.position = pos;
                p.exposure = look.auto_exposure.then_some(track.exposure);
            }
            if let Err(e) = session.as_mut().unwrap().stream.send(colors) {
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
        self.live.set_streaming(on, colors);
    }
}

/// Opens the area's stream outside of playback and sends `colors(area, t)`
/// at 50 Hz until `stop()` or `seconds` pass (test pattern, still frame).
pub async fn stream_for(
    cfg: &Config,
    seconds: f32,
    stop: impl Fn() -> bool,
    mut colors: impl FnMut(&Area, f32) -> Vec<(u8, Rgb)>,
) -> Result<()> {
    let bridge = Bridge::new(&cfg.hue);
    let area = bridge.find_area(cfg.hue.area.as_deref()).await?;
    let mut stream = bridge.open_stream(&area.id).await?;
    let start = Instant::now();
    let mut tick = tokio::time::interval(FRAME);
    let mut res = Ok(());
    while start.elapsed().as_secs_f32() < seconds && !stop() {
        tick.tick().await;
        if let Err(e) = stream.send(&colors(&area, start.elapsed().as_secs_f32())) {
            res = Err(e);
            break;
        }
    }
    stream.close();
    bridge.set_streaming(&area.id, false).await?;
    res
}

/// Test pattern: a rainbow rotating across the area's channels, or with
/// `only`, that channel blinking white and the others off (to find a light).
pub async fn test_pattern(cfg: &Config, seconds: f32, only: Option<u8>) -> Result<()> {
    stream_for(
        cfg,
        seconds,
        || false,
        |area, t| {
            let n = area.channels.len().max(1) as f32;
            area.channels
                .iter()
                .enumerate()
                .map(|(i, ch)| match only {
                    Some(id) if id == ch.channel_id && t.fract() < 0.6 => (id, [1.0; 3]),
                    Some(_) => (ch.channel_id, [0.0; 3]),
                    None => (ch.channel_id, hue_to_rgb(t * 0.25 + i as f32 / n)),
                })
                .collect()
        },
    )
    .await
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
