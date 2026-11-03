//! What the player saw while a film played, for the logs: sudden changes of
//! the lights next to the picture's own, the clock's corrections, how often
//! the app reports, and a summary every minute.
//!
//! Brightness here is perceptual (sRGB, 0 to 1), like the eye sees it.

use crate::clock::Correction;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// A change of the lights this big…
const STEP_SIZE: f32 = 0.3;
/// …within this long is a step.
const STEP_WINDOW: Duration = Duration::from_millis(300);
/// At most one step logged this often (a cut makes several frames qualify).
const STEP_QUIET: Duration = Duration::from_secs(2);
/// A clock jump this recent is named next to a step.
const JUMP_RECENT: Duration = Duration::from_secs(2);
/// A frame later than this is a stall (the loop runs every 20 ms).
const STALL: f32 = 0.06;
const SUMMARY_EVERY: Duration = Duration::from_secs(60);

/// One frame of a film on the lights.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub at: Instant,
    /// Position in the film the lights show (seconds, sync offset included).
    pub film: f64,
    /// The brightest light, perceptual.
    pub lights: f32,
    /// The whole picture's brightness at that position, before smoothing and the Look.
    pub picture: f32,
}

/// The lights changed a lot in a short time.
#[derive(Debug, PartialEq)]
pub struct Step {
    pub film: f64,
    pub lights_from: f32,
    pub lights_to: f32,
    pub picture_from: f32,
    pub picture_to: f32,
    pub seconds: f32,
    /// A clock jump just before (its error in seconds, and how long before).
    pub after_jump: Option<(f64, f32)>,
}

/// One minute of playback.
#[derive(Debug, Default, PartialEq)]
pub struct Summary {
    pub film_from: f64,
    pub film_to: f64,
    pub frames: u32,
    pub stalls: u32,
    pub longest_frame: f32,
    /// Reports where the app's position had moved.
    pub updates: u32,
    pub longest_between_updates: f32,
    pub jumps: u32,
    pub nudges: u32,
    /// The largest error of a jump or a nudge (seconds, signed).
    pub largest_error: f64,
    pub steps: u32,
    pub lights_mean: f32,
    pub lights_min: f32,
    pub lights_max: f32,
}

#[derive(Default)]
pub struct Diagnostics {
    recent: VecDeque<Frame>,
    last_step: Option<Instant>,
    last_jump: Option<(Instant, f64)>,
    last_update: Option<(i64, Instant)>,
    minute_start: Option<Instant>,
    lights_sum: f32,
    summary: Summary,
}

impl Diagnostics {
    /// A film starts, stops or another one plays: start over.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// A playback report: `ticks` is the app's position, unchanged between its updates.
    pub fn on_report(&mut self, at: Instant, ticks: i64, correction: &Correction) {
        match correction {
            Correction::Jumped { error } => {
                self.summary.jumps += 1;
                self.keep_largest(*error);
                self.last_jump = Some((at, *error));
            }
            Correction::Nudged { error } => {
                self.summary.nudges += 1;
                self.keep_largest(*error);
            }
            Correction::Restarted | Correction::Steady { .. } => {}
        }
        match self.last_update {
            Some((last_ticks, _)) if last_ticks == ticks => {}
            Some((_, since)) => {
                self.summary.updates += 1;
                let between = (at - since).as_secs_f32();
                self.summary.longest_between_updates = self.summary.longest_between_updates.max(between);
                self.last_update = Some((ticks, at));
            }
            None => self.last_update = Some((ticks, at)),
        }
    }

    /// A frame sent to the lights, `dt` seconds after the previous one.
    /// Returns a step when the lights just changed a lot.
    pub fn on_frame(&mut self, frame: Frame, dt: f32) -> Option<Step> {
        self.minute_start.get_or_insert(frame.at);
        if self.summary.frames == 0 {
            self.summary.film_from = frame.film;
            self.summary.lights_min = frame.lights;
        }
        self.summary.film_to = frame.film;
        self.summary.frames += 1;
        self.summary.longest_frame = self.summary.longest_frame.max(dt);
        if dt > STALL {
            self.summary.stalls += 1;
        }
        self.lights_sum += frame.lights;
        self.summary.lights_min = self.summary.lights_min.min(frame.lights);
        self.summary.lights_max = self.summary.lights_max.max(frame.lights);

        while self.recent.front().is_some_and(|old| frame.at - old.at > STEP_WINDOW) {
            self.recent.pop_front();
        }
        self.recent.push_back(frame);
        let oldest = *self.recent.front()?;
        let quiet = self.last_step.is_none_or(|last| frame.at - last >= STEP_QUIET);
        if (frame.lights - oldest.lights).abs() < STEP_SIZE || !quiet {
            return None;
        }
        self.last_step = Some(frame.at);
        self.summary.steps += 1;
        let after_jump = self
            .last_jump
            .filter(|(jumped_at, _)| frame.at - *jumped_at <= JUMP_RECENT)
            .map(|(jumped_at, error)| (error, (frame.at - jumped_at).as_secs_f32()));
        Some(Step {
            film: frame.film,
            lights_from: oldest.lights,
            lights_to: frame.lights,
            picture_from: oldest.picture,
            picture_to: frame.picture,
            seconds: (frame.at - oldest.at).as_secs_f32(),
            after_jump,
        })
    }

    /// The last minute of playback, once a minute has passed.
    pub fn summary(&mut self, now: Instant) -> Option<Summary> {
        let start = self.minute_start?;
        if now - start < SUMMARY_EVERY || self.summary.frames == 0 {
            return None;
        }
        let mut summary = std::mem::take(&mut self.summary);
        summary.lights_mean = self.lights_sum / summary.frames as f32;
        self.lights_sum = 0.0;
        self.minute_start = Some(now);
        Some(summary)
    }

    fn keep_largest(&mut self, error: f64) {
        if error.abs() > self.summary.largest_error.abs() {
            self.summary.largest_error = error;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(t0: Instant, ms: u64, lights: f32, picture: f32) -> Frame {
        Frame { at: t0 + Duration::from_millis(ms), film: 600.0 + ms as f64 / 1000.0, lights, picture }
    }

    #[test]
    fn a_sudden_change_is_a_step_a_fade_is_not() {
        let t0 = Instant::now();
        let mut d = Diagnostics::default();
        // A fade from 0.8 to 0.2 over 2 s: never 0.3 within 300 ms.
        for i in 0..=100 {
            let level = 0.8 - 0.6 * i as f32 / 100.0;
            assert_eq!(d.on_frame(frame(t0, i * 20, level, level), 0.02), None);
        }
        // Then a jump to 0.9 in one frame.
        let step = d.on_frame(frame(t0, 2020, 0.9, 0.2), 0.02).unwrap();
        // Measured against 0.3 s before.
        assert!((step.lights_from - 0.284).abs() < 0.01 && step.lights_to == 0.9, "{step:?}");
        assert_eq!(step.picture_to, 0.2);
        assert_eq!(step.after_jump, None);
        // The frames right after are the same step: not logged again.
        assert_eq!(d.on_frame(frame(t0, 2040, 0.1, 0.2), 0.02), None);
    }

    #[test]
    fn a_step_names_a_recent_clock_jump() {
        let t0 = Instant::now();
        let mut d = Diagnostics::default();
        d.on_frame(frame(t0, 0, 0.1, 0.1), 0.02);
        d.on_report(t0 + Duration::from_millis(10), 1, &Correction::Jumped { error: 0.9 });
        let step = d.on_frame(frame(t0, 20, 0.7, 0.1), 0.02).unwrap();
        let (error, ago) = step.after_jump.unwrap();
        assert_eq!(error, 0.9);
        assert!((ago - 0.01).abs() < 0.005, "{ago}");
    }

    #[test]
    fn a_minute_sums_up_frames_reports_and_corrections() {
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut d = Diagnostics::default();
        // The app reports every second but moves its position every 10 s.
        for s in 0..60u64 {
            let ticks = (s / 10) as i64;
            let correction = match s {
                30 => Correction::Nudged { error: -0.3 },
                40 => Correction::Jumped { error: 1.2 },
                _ => Correction::Steady { error: 0.0 },
            };
            d.on_report(at(s * 1000), ticks, &correction);
        }
        d.on_frame(frame(t0, 0, 0.5, 0.5), 0.02);
        d.on_frame(frame(t0, 20, 0.3, 0.5), 0.02);
        d.on_frame(frame(t0, 420, 0.4, 0.5), 0.4);
        assert_eq!(d.summary(at(59_000)), None);
        let summary = d.summary(at(60_000)).unwrap();
        assert_eq!((summary.frames, summary.stalls), (3, 1));
        assert_eq!((summary.jumps, summary.nudges, summary.largest_error), (1, 1, 1.2));
        assert!((summary.lights_mean - 0.4).abs() < 1e-6 && summary.lights_min == 0.3 && summary.lights_max == 0.5);
        assert_eq!((summary.updates, summary.longest_between_updates), (5, 10.0));
        assert_eq!(summary.film_from, 600.0);
        // The next minute starts empty.
        assert_eq!(d.summary(at(61_000)), None);
    }
}
