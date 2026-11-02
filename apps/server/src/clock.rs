//! Where the film is, from Jellyfin's playback reports.
//!
//! Apps report their position every few seconds, each on its own schedule.
//! The clock times the reports itself, learns how late each app's check-in
//! runs, and once locked only re-syncs on a seek, a pause or a new film,
//! correcting small drifts gently so the lights stay smooth.

use crate::config::SyncConfig;
use crate::jellyfin::NowPlaying;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// What a playback report did to the clock.
#[derive(Debug, PartialEq)]
pub enum Correction {
    /// Started again from the report: a new film, a pause or a resume.
    Restarted,
    /// The report disagreed by more than `jump`: the clock jumped by `error`
    /// seconds (positive: the app was ahead of the clock).
    Jumped { error: f64 },
    /// The report disagreed by more than `drift`: a fifth of `error` corrected.
    Nudged { error: f64 },
    /// Close enough: nothing changed.
    Steady { error: f64 },
}

pub struct PlaybackClock {
    /// Learn each app's report delay.
    learn: bool,
    /// Errors above this re-sync at once (seconds).
    jump: f64,
    /// Errors above this are corrected gently (seconds).
    drift: f64,
    /// The last report whose position changed, when we saw it, and whether it was paused.
    seen: Option<(String, i64, Instant, bool)>,
    /// Learned check-in delay of each device's app (seconds).
    delays: HashMap<String, f64>,
    device: String,
    item: Option<String>,
    paused: bool,
    /// Film position at an instant; the clock runs from there.
    anchor: Option<(Instant, f64)>,
}

impl PlaybackClock {
    pub fn new(cfg: &SyncConfig) -> Self {
        Self {
            learn: cfg.learn_report_delay,
            jump: cfg.jump_ms / 1000.0,
            drift: cfg.drift_ms / 1000.0,
            seen: None,
            delays: HashMap::new(),
            device: String::new(),
            item: None,
            paused: false,
            anchor: None,
        }
    }

    /// Takes a report seen at `np.at` and says what it changed.
    pub fn report(&mut self, np: &NowPlaying) -> Correction {
        let reported = self.estimate(np);
        let expected = self.position(np.at);
        let changed = self.item.as_deref() != Some(&np.item_id) || self.paused != np.paused;
        self.item = Some(np.item_id.clone());
        self.paused = np.paused;
        let (Some(expected), Some((i, p)), false) = (expected, self.anchor, changed) else {
            self.anchor = Some((np.at, reported));
            return Correction::Restarted;
        };
        let error = reported - expected;
        if error.abs() > self.jump {
            self.anchor = Some((np.at, reported));
            Correction::Jumped { error }
        } else if error.abs() > self.drift {
            self.anchor = Some((i, p + error * 0.2));
            Correction::Nudged { error }
        } else {
            Correction::Steady { error }
        }
    }

    /// Where the film is at `when`, once a report came in.
    pub fn position(&self, when: Instant) -> Option<f64> {
        let (i, p) = self.anchor?;
        Some(if self.paused { p } else { p + when.saturating_duration_since(i).as_secs_f64() })
    }

    /// How late the current app's check-in runs, as learned (seconds).
    pub fn report_delay(&self) -> Option<f64> {
        self.delays.get(&self.device).copied()
    }

    /// Nothing plays any more.
    pub fn reset(&mut self) {
        (self.seen, self.item, self.anchor, self.paused) = (None, None, None, false);
    }

    /// The position this report stands for at `np.at`. We extrapolate from
    /// the moment *we* saw the reported position change (one poll late at
    /// most); `LastPlaybackCheckIn` is only used the first time, minus the
    /// app's delay. When a new report arrives, the check-in's age at that
    /// moment *is* that delay, which is how it is learned.
    fn estimate(&mut self, np: &NowPlaying) -> f64 {
        let position = np.ticks as f64 / 1e7;
        self.device.clone_from(&np.device);
        let delay = self.delays.get(&np.device).copied();
        match &mut self.seen {
            // Paused, or just resumed: the position hasn't moved; run from here.
            Some((item, ticks, at, was_paused)) if *item == np.item_id && (np.paused || *was_paused) => {
                (*ticks, *at, *was_paused) = (np.ticks, np.at, np.paused);
                position
            }
            Some((item, ticks, at, _)) if *item == np.item_id && *ticks == np.ticks => {
                position + np.at.saturating_duration_since(*at).as_secs_f64()
            }
            Some((item, ticks, at, _)) if *item == np.item_id => {
                (*ticks, *at) = (np.ticks, np.at);
                if let Some(a) = np.checkin_age.filter(|a| self.learn && (-30.0..30.0).contains(a)) {
                    self.delays.insert(np.device.clone(), delay.map_or(a, |d| d * 0.7 + a * 0.3));
                }
                position
            }
            _ => {
                let age = np.checkin_age.filter(|a| !np.paused && (0.0..6.0 * 3600.0).contains(a)).unwrap_or(0.0);
                let age = (age - if self.learn { delay.unwrap_or(0.0) } else { 0.0 }).max(0.0);
                let at = np.at.checked_sub(Duration::from_secs_f64(age)).unwrap_or(np.at);
                self.seen = Some((np.item_id.clone(), np.ticks, at, np.paused));
                position + age
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync(learn: bool, jump: f64, drift: f64) -> SyncConfig {
        SyncConfig { learn_report_delay: learn, jump_ms: jump * 1000.0, drift_ms: drift * 1000.0 }
    }

    /// A report of `item` at `pos` seconds, seen `dt` seconds after `t0`.
    fn np(t0: Instant, item: &str, pos: f64, dt: f64, paused: bool, checkin_age: Option<f64>) -> NowPlaying {
        NowPlaying {
            item_id: item.into(),
            name: String::new(),
            path: None,
            device: "Apple TV".into(),
            ticks: (pos * 1e7) as i64,
            paused,
            checkin_age,
            at: t0 + Duration::from_secs_f64(dt),
        }
    }

    #[test]
    fn extrapolates_from_when_the_report_changed() {
        let t0 = Instant::now();
        let at = |s: f64| t0 + Duration::from_secs_f64(s);
        let mut c = PlaybackClock::new(&sync(false, 0.75, 0.1));
        // First sight: trust the check-in age once.
        assert_eq!(c.report(&np(t0, "a", 100.0, 0.0, false, Some(2.0))), Correction::Restarted);
        assert_eq!(c.position(at(0.0)), Some(102.0));
        // The same report again 3 s later: the clock just runs.
        assert!(matches!(c.report(&np(t0, "a", 100.0, 3.0, false, Some(2.0))), Correction::Steady { .. }));
        assert_eq!(c.position(at(3.0)), Some(105.0));
        // A new report is the position now, whatever the check-in says.
        c.report(&np(t0, "a", 106.0, 6.0, false, Some(9.0)));
        assert!((c.position(at(6.0)).unwrap() - 106.0).abs() < 0.25);
        // Paused for a minute, then resumed with the same report (no report in
        // between): no jump forward.
        c.report(&np(t0, "a", 107.0, 8.0, true, None));
        assert_eq!(c.position(at(68.0)), Some(107.0));
        assert_eq!(c.report(&np(t0, "a", 107.0, 69.0, false, None)), Correction::Restarted);
        assert_eq!(c.position(at(70.0)), Some(108.0));
    }

    #[test]
    fn learns_the_app_report_delay_and_corrects_the_first_sight() {
        let t0 = Instant::now();
        let at = |s: f64| t0 + Duration::from_secs_f64(s);
        let mut c = PlaybackClock::new(&sync(true, 0.75, 0.1));
        // An app whose check-in is 4 s older than its reports.
        c.report(&np(t0, "a", 100.0, 0.0, false, Some(4.0)));
        c.report(&np(t0, "a", 110.0, 10.0, false, Some(4.0)));
        assert_eq!(c.report_delay(), Some(4.0));
        // Next film, first sight 2 s after a report: 6 s of check-in age, 2 s really.
        c.report(&np(t0, "b", 50.0, 20.0, false, Some(6.0)));
        assert_eq!(c.position(at(20.0)), Some(52.0));
        // Learning off: the check-in is taken as is.
        let mut c = PlaybackClock::new(&sync(false, 0.75, 0.1));
        c.report(&np(t0, "c", 50.0, 20.0, false, Some(6.0)));
        assert_eq!(c.position(at(20.0)), Some(56.0));
    }

    #[test]
    fn pause_seek_while_paused_then_resume() {
        let t0 = Instant::now();
        let at = |s: f64| t0 + Duration::from_secs_f64(s);
        let mut c = PlaybackClock::new(&sync(true, 0.75, 0.1));
        c.report(&np(t0, "a", 600.0, 0.0, false, None));
        // Pause at 10:00: frozen.
        c.report(&np(t0, "a", 600.0, 1.0, true, None));
        assert_eq!(c.position(at(20.0)), Some(600.0));
        // Seek to 25:00 while paused: the still moves there at once.
        assert_eq!(c.report(&np(t0, "a", 1500.0, 20.0, true, None)), Correction::Jumped { error: 900.0 });
        assert_eq!(c.position(at(40.0)), Some(1500.0));
        // Resume a minute later: runs from 25:00, not 25:00 plus the pause.
        assert_eq!(c.report(&np(t0, "a", 1500.0, 80.0, false, None)), Correction::Restarted);
        assert_eq!(c.position(at(82.0)), Some(1502.0));
        // The app reports the seek only on resume: same result.
        let mut c = PlaybackClock::new(&sync(true, 0.75, 0.1));
        c.report(&np(t0, "a", 600.0, 0.0, false, None));
        c.report(&np(t0, "a", 600.0, 1.0, true, None));
        c.report(&np(t0, "a", 1500.0, 80.0, false, None));
        assert_eq!(c.position(at(82.0)), Some(1502.0));
    }

    #[test]
    fn locks_ignores_noise_and_jumps_on_a_seek() {
        let t0 = Instant::now();
        let at = |s: f64| t0 + Duration::from_secs_f64(s);
        let mut c = PlaybackClock::new(&sync(false, 1.5, 0.25));
        assert_eq!(c.report(&np(t0, "a", 100.0, 0.0, false, None)), Correction::Restarted);
        // A slightly noisy report: ignored.
        assert!(matches!(c.report(&np(t0, "a", 110.1, 10.0, false, None)), Correction::Steady { .. }));
        assert!((c.position(at(10.0)).unwrap() - 110.0).abs() < 1e-9);
        // Off by 0.5 s: a fifth of it corrected.
        let Correction::Nudged { error } = c.report(&np(t0, "a", 120.5, 20.0, false, None)) else { panic!() };
        assert!((error - 0.5).abs() < 1e-6, "{error}");
        assert!((c.position(at(20.0)).unwrap() - 120.1).abs() < 1e-6);
        // The app 2 s ahead: the clock jumps there, and says by how much.
        let Correction::Jumped { error } = c.report(&np(t0, "a", 132.1, 30.0, false, None)) else { panic!() };
        assert!((error - 2.0).abs() < 1e-6, "{error}");
        assert!((c.position(at(30.0)).unwrap() - 132.1).abs() < 1e-9);
        // A seek: immediate re-sync.
        assert!(matches!(c.report(&np(t0, "a", 300.0, 31.0, false, None)), Correction::Jumped { .. }));
        // Pause: the clock stops.
        c.report(&np(t0, "a", 301.0, 32.0, true, None));
        assert_eq!(c.position(at(60.0)), Some(301.0));
        c.reset();
        assert_eq!(c.position(at(60.0)), None);
    }
}
