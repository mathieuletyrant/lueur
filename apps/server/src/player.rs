//! Main loop: follows Jellyfin, loads the track of the current film and
//! streams colors to the entertainment area at 50 frames per second.

use crate::clock::PlaybackClock;
use crate::color::{Look, Rgb, linear_to_srgb};
use crate::config::Config;
use crate::hue::{Area, Bridge, Channel};
use crate::jellyfin::{Jellyfin, NowPlaying};
use crate::lights::{HueSink, Lights};
use crate::queue::Queue;
use crate::render::Renderer;
use crate::track::{self, Track};
use anyhow::Result;
use serde::Serialize;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tracing::{info, warn};

const FRAME: Duration = Duration::from_millis(20);
const PAUSE_TIMEOUT: Duration = Duration::from_secs(30);
const RETRY_TRACK: Duration = Duration::from_secs(5);

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
/// a film plays, the status is read by the UI, and the UI can ask for a
/// still frame or the test pattern, which the player shows when no film does.
#[derive(Default)]
pub struct Live {
    pub look: Mutex<Look>,
    pub status: Mutex<Status>,
    pub hold: Mutex<Option<Hold>>,
    pub test: Mutex<Option<TestPattern>>,
}

/// A moment of a video held on the lights, to tune the Look in front of them.
#[derive(Debug, Clone, Serialize)]
pub struct Hold {
    pub path: PathBuf,
    pub t: f64,
    #[serde(skip)]
    pub until: Instant,
}

/// The rainbow, or one channel blinking (to find a light), for a few seconds.
#[derive(Debug, Clone)]
pub struct TestPattern {
    pub only: Option<u8>,
    pub start: Instant,
    pub until: Instant,
}

impl TestPattern {
    fn colors(&self, channels: &[Channel], now: Instant) -> Vec<(u8, Rgb)> {
        let t = (now - self.start).as_secs_f32();
        let n = channels.len().max(1) as f32;
        channels
            .iter()
            .enumerate()
            .map(|(i, ch)| match self.only {
                Some(id) if id == ch.channel_id && t.fract() < 0.6 => (id, [1.0; 3]),
                Some(_) => (ch.channel_id, [0.0; 3]),
                None => (ch.channel_id, hue_to_rgb(t * 0.25 + i as f32 / n)),
            })
            .collect()
    }
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
    lights: Lights<HueSink>,
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
        let lights = Lights::new(HueSink::new(bridge, &area.id), live.clone());
        Ok(Self { cfg, lights, area, live, queue })
    }

    fn status(&self) -> std::sync::MutexGuard<'_, Status> {
        self.live.status.lock().unwrap()
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

    /// Runs until `stop` completes, then gives the lights back.
    pub async fn run(mut self, stop: impl Future<Output = ()>) -> Result<()> {
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

        let mut clock = PlaybackClock::new(&self.cfg.sync);
        let mut current: Option<(String, Option<Arc<Track>>)> = None;
        let mut paused_since: Option<Instant> = None;
        let mut retry_track_at = Instant::now();
        let mut renderer = Renderer::new(&self.area.channels);
        let mut still_renderer = Renderer::new(&self.area.channels);
        let mut still: Option<(PathBuf, Track)> = None;
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
                        if clock.report(np) {
                            tracing::debug!("clock re-synced to {:.2} s", clock.position(np.at).unwrap_or_default());
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
                    position: clock.position(np.at).unwrap_or_default(),
                    duration: current.as_ref().and_then(|c| c.1.as_ref()).map(|t| t.duration()),
                    paused: np.paused,
                    has_track: current.as_ref().is_some_and(|c| c.1.is_some()),
                    report_delay: clock.report_delay(),
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

            // What the lights show, by priority: the film, a still frame, the test pattern.
            let film = current
                .as_ref()
                .and_then(|(_, t)| t.clone())
                .filter(|_| paused_since.is_none_or(|p| now - p < PAUSE_TIMEOUT));
            let look = self.live.look.lock().unwrap().clone();
            let colors = if let Some(track) = film
                && let Some(pos) = clock.position(now)
            {
                // A film takes the lights back from a still frame or a test.
                self.live.hold.lock().unwrap().take();
                self.live.test.lock().unwrap().take();
                if let Some(p) = self.status().playing.as_mut() {
                    p.position = pos;
                    p.exposure = look.auto_exposure.then_some(track.exposure);
                }
                Some(renderer.colors(&track, pos + look.offset_ms / 1000.0, &look, dt).to_vec())
            } else if let Some(h) = self.live_hold(now) {
                if still.as_ref().is_none_or(|s| s.0 != h.path) {
                    // ponytail: loads on the frame loop (~50 ms once per film), spawn_blocking if it stutters.
                    still = Track::load(&track::track_path(&self.cfg.store, &h.path)).ok().map(|t| (h.path.clone(), t));
                }
                // Same rendering as a film: moving the slider fades like a cut does.
                still.as_ref().map(|(_, t)| still_renderer.colors(t, h.t, &look, dt).to_vec())
            } else {
                self.live_test(now).map(|t| t.colors(&self.area.channels, now))
            };
            self.lights.show(now, colors.as_deref()).await;
        }

        poller.abort();
        self.lights.close().await;
        Ok(())
    }

    /// The still frame asked for, unless it expired.
    fn live_hold(&self, now: Instant) -> Option<Hold> {
        let mut h = self.live.hold.lock().unwrap();
        if h.as_ref().is_some_and(|h| now >= h.until) {
            *h = None;
        }
        h.clone()
    }

    /// The test pattern asked for, unless it is over.
    fn live_test(&self, now: Instant) -> Option<TestPattern> {
        let mut t = self.live.test.lock().unwrap();
        if t.as_ref().is_some_and(|t| now >= t.until) {
            *t = None;
        }
        t.clone()
    }
}

/// `lueur test`: the test pattern, without the player.
pub async fn test_pattern(cfg: &Config, seconds: f32, only: Option<u8>) -> Result<()> {
    let bridge = Bridge::new(&cfg.hue);
    let area = bridge.find_area(cfg.hue.area.as_deref()).await?;
    let mut lights = Lights::new(HueSink::new(bridge, &area.id), Arc::new(Live::default()));
    let start = Instant::now();
    let pattern = TestPattern { only, start, until: start + Duration::from_secs_f32(seconds) };
    let mut tick = tokio::time::interval(FRAME);
    while Instant::now() < pattern.until {
        tick.tick().await;
        let now = Instant::now();
        lights.show(now, Some(&pattern.colors(&area.channels, now))).await;
    }
    lights.close().await;
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
