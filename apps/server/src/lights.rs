//! The Area's lights, with one owner for their Entertainment stream.
//!
//! Every frame the player says what the lights should show, or nothing.
//! `Lights` opens the stream when needed, keeps it through short gaps,
//! retries after errors, gives the lights back once idle, and is the only
//! one to publish what they show.

use crate::color::Rgb;
use crate::hue::{Bridge, Stream};
use crate::player::Live;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Nothing to show for this long: the lights go back to their previous state.
const STOP_AFTER: Duration = Duration::from_millis(1500);
const RETRY_OPEN: Duration = Duration::from_secs(5);
const RETRY_SEND: Duration = Duration::from_secs(2);

/// Where the colors go: the bridge's Entertainment stream, or a fake in tests.
pub trait Sink {
    /// Takes the Area over (its lights stop following the Hue app).
    async fn open(&mut self) -> Result<()>;
    /// One frame: a color (linear light) per channel.
    fn send(&mut self, colors: &[(u8, Rgb)]) -> Result<()>;
    /// Gives the lights back to their previous state.
    async fn close(&mut self);
}

/// The bridge's Entertainment stream (DTLS) for one Area.
pub struct HueSink {
    bridge: Bridge,
    area_id: String,
    stream: Option<Stream>,
}

impl HueSink {
    pub fn new(bridge: Bridge, area_id: &str) -> Self {
        Self { bridge, area_id: area_id.into(), stream: None }
    }
}

impl Sink for HueSink {
    async fn open(&mut self) -> Result<()> {
        self.stream = Some(self.bridge.open_stream(&self.area_id).await?);
        Ok(())
    }

    fn send(&mut self, colors: &[(u8, Rgb)]) -> Result<()> {
        match &mut self.stream {
            Some(s) => s.send(colors),
            None => anyhow::bail!("stream not open"),
        }
    }

    async fn close(&mut self) {
        if let Some(s) = self.stream.take() {
            s.close();
        }
        if let Err(e) = self.bridge.set_streaming(&self.area_id, false).await {
            warn!("stopping the stream: {e:#}");
        }
    }
}

pub struct Lights<S: Sink> {
    sink: S,
    live: Arc<Live>,
    open: bool,
    retry_at: Option<Instant>,
    idle_since: Option<Instant>,
}

impl<S: Sink> Lights<S> {
    pub fn new(sink: S, live: Arc<Live>) -> Self {
        Self { sink, live, open: false, retry_at: None, idle_since: None }
    }

    /// What the lights show at `now`: `colors`, or nothing (`None`).
    pub async fn show(&mut self, now: Instant, colors: Option<&[(u8, Rgb)]>) {
        let Some(colors) = colors else {
            let since = *self.idle_since.get_or_insert(now);
            if self.open && now - since >= STOP_AFTER {
                self.close().await;
            }
            return;
        };
        self.idle_since = None;
        if !self.open {
            if self.retry_at.is_some_and(|r| now < r) {
                return;
            }
            if let Err(e) = self.sink.open().await {
                warn!("opening the stream: {e:#} (retrying in 5 s)");
                self.retry_at = Some(now + RETRY_OPEN);
                return;
            }
            self.open = true;
            info!("Entertainment stream open");
        }
        if let Err(e) = self.sink.send(colors) {
            warn!("sending to the bridge: {e:#}");
            self.close().await;
            self.retry_at = Some(now + RETRY_SEND);
            return;
        }
        self.live.set_streaming(true, colors);
    }

    /// Gives the lights back now.
    pub async fn close(&mut self) {
        if self.open {
            self.sink.close().await;
            self.open = false;
            info!("stream stopped, lights back to their previous state");
        }
        self.live.set_streaming(false, &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Fake {
        opens: usize,
        closes: usize,
        frames: usize,
        fail_open: bool,
        fail_send: bool,
    }

    impl Sink for Fake {
        async fn open(&mut self) -> Result<()> {
            self.opens += 1;
            if self.fail_open { anyhow::bail!("bridge unreachable") } else { Ok(()) }
        }
        fn send(&mut self, _: &[(u8, Rgb)]) -> Result<()> {
            self.frames += 1;
            if self.fail_send { anyhow::bail!("broken pipe") } else { Ok(()) }
        }
        async fn close(&mut self) {
            self.closes += 1;
        }
    }

    const RED: [(u8, Rgb); 1] = [(0, [1.0, 0.0, 0.0])];

    fn lights(fake: Fake) -> (Lights<Fake>, Arc<Live>) {
        let live = Arc::new(Live::default());
        live.status.lock().unwrap().channels =
            vec![crate::player::LiveChannel { id: 0, x: 0.0, y: 0.0, z: 0.0, color: [0; 3] }];
        (Lights::new(fake, live.clone()), live)
    }

    #[tokio::test]
    async fn opens_once_keeps_through_short_gaps_then_gives_back() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let (mut l, live) = lights(Fake::default());
        l.show(at(0), Some(&RED)).await;
        l.show(at(20), Some(&RED)).await;
        assert_eq!((l.sink.opens, l.sink.frames), (1, 2));
        assert!(live.status.lock().unwrap().streaming);
        assert_eq!(live.status.lock().unwrap().channels[0].color, [255, 0, 0]);
        // A short gap (a cut to nothing, a reload): stays open.
        l.show(at(40), None).await;
        l.show(at(1000), None).await;
        assert_eq!(l.sink.closes, 0);
        // Idle long enough: the lights go back, and the status says so.
        l.show(at(1600), None).await;
        assert_eq!(l.sink.closes, 1);
        assert!(!live.status.lock().unwrap().streaming);
    }

    #[tokio::test]
    async fn retries_a_failed_open_after_a_while_and_reopens_after_a_send_error() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let (mut l, _) = lights(Fake { fail_open: true, ..Fake::default() });
        l.show(at(0), Some(&RED)).await;
        l.show(at(1000), Some(&RED)).await;
        assert_eq!(l.sink.opens, 1, "no retry before 5 s");
        l.sink.fail_open = false;
        l.show(at(5000), Some(&RED)).await;
        assert_eq!((l.sink.opens, l.sink.frames), (2, 1));
        // The stream breaks: closed, then reopened 2 s later.
        l.sink.fail_send = true;
        l.show(at(5020), Some(&RED)).await;
        assert_eq!(l.sink.closes, 1);
        l.sink.fail_send = false;
        l.show(at(6000), Some(&RED)).await;
        assert_eq!(l.sink.opens, 2);
        l.show(at(7100), Some(&RED)).await;
        assert_eq!(l.sink.opens, 3);
    }
}
