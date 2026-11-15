//! What the lights show for a moment of a film: the Track sampled at that
//! moment, each light's part of the screen, the Look. The player and the
//! still frame both go through here, so they render the same way.

use crate::color::{Look, Rgb};
use crate::hue::Channel;
use crate::track::Track;

/// Renders a Track on an Area's lights or a Strip's LEDs, frame after frame.
pub struct Renderer {
    channels: Vec<Channel>,
    /// The Channels are a Strip's LEDs: they read the Edges, when the Track has them.
    strip: bool,
    /// The Track being rendered (its address): a new one restarts the smoothing.
    track: usize,
    mapping: Option<Mapping>,
    smoother: Smoother,
    grid: Vec<Rgb>,
    colors: Vec<(u8, Rgb)>,
}

impl Renderer {
    /// For a Hue Area: each light reads the grid.
    pub fn for_area(channels: &[Channel]) -> Self {
        Self::new(channels, false)
    }

    /// For a WLED Strip: each LED reads the Edge cells in front of it.
    pub fn for_strip(channels: &[Channel]) -> Self {
        Self::new(channels, true)
    }

    fn new(channels: &[Channel], strip: bool) -> Self {
        Self {
            channels: channels.to_vec(),
            strip,
            track: 0,
            mapping: None,
            smoother: Smoother { state: Vec::new() },
            grid: Vec::new(),
            colors: Vec::new(),
        }
    }

    /// Each light's color (linear light) at `t` seconds of `track`, `dt`
    /// seconds after the previous call (for the smoothing).
    pub fn colors(&mut self, track: &Track, t: f64, look: &Look, dt: f32) -> &[(u8, Rgb)] {
        let id = track as *const Track as usize;
        if id != self.track {
            self.track = id;
            self.mapping = None;
            self.smoother.state.clear();
        }
        let look = &look.with_exposure(track.exposure);
        // A version 1 Track has no Edges: a Strip reads its grid, like an Area.
        let edges = self.strip && track.has_edges();
        // Built once per Track, again when the Look's spread or fit_area change.
        if !self.mapping.as_ref().is_some_and(|m| m.made_for(track, look, edges)) {
            self.mapping = Some(if edges {
                Mapping::along_edges(&self.channels, track.edge_cols, track.edge_rows, look)
            } else {
                Mapping::new(&self.channels, track.cols, track.rows, look)
            });
        }
        let half = look.smoothing_ms as f64 / 1000.0;
        match (edges, look.centered_smoothing) {
            (false, true) => track.sample_centered(t, half, &mut self.grid),
            (false, false) => track.sample(t, &mut self.grid),
            (true, true) => track.sample_edges_centered(t, half, &mut self.grid),
            (true, false) => track.sample_edges(t, &mut self.grid),
        }
        self.mapping.as_ref().unwrap().colors(&self.grid, &mut self.colors);
        for c in self.colors.iter_mut() {
            c.1 = look.apply(c.1);
        }
        if !look.centered_smoothing {
            self.smoother.apply(&mut self.colors, look, dt);
        }
        &self.colors
    }
}

/// For each channel, the grid cells (or Edge cells) that light it and their weights.
struct Mapping {
    channels: Vec<(u8, Vec<(usize, f32)>)>,
    /// What it was built for: Edges or grid, their size, and the Look's spread and fit_area.
    made_for: (bool, u8, u8, f32, bool),
}

impl Mapping {
    /// Hue position: x from -1 (left) to 1 (right), z from -1 (floor) to 1 (ceiling).
    /// Positions are stretched to the area's extent: lights (or the segments
    /// of a gradient strip) grouped around the TV still reach the screen's
    /// edges. Each channel takes a Gaussian average of the screen near its edge.
    fn new(channels: &[Channel], cols: u8, rows: u8, look: &Look) -> Self {
        let (cols, rows) = (cols as usize, rows as usize);
        let (spread, fit_area) = (look.spread.clamp(0.2, 5.0), look.fit_area);
        let fit = |vals: Vec<f32>| {
            let (lo, hi) = vals.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            // All on one line (a single row of lights): keep Hue's own scale.
            move |v: f32| {
                if fit_area && hi - lo > 0.2 { (v - lo) / (hi - lo) } else { ((v + 1.0) / 2.0).clamp(0.0, 1.0) }
            }
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
                        let d = ((cu - u) / (0.15 * spread)).powi(2) + ((cv - v) / (0.18 * spread)).powi(2);
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
        Self { channels, made_for: (false, cols as u8, rows as u8, look.spread, look.fit_area) }
    }

    /// Strip LEDs, placed on the screen's edges (x and z from -1 to 1, as
    /// `wled::channels` puts them). Each LED takes a Gaussian average of the
    /// Edge cells next to it, along the edge and around the corners; distances
    /// are measured on a 16:9 screen, so a cell is as wide on the sides as on top.
    fn along_edges(channels: &[Channel], cols: u8, rows: u8, look: &Look) -> Self {
        let (cols_n, rows_n) = (cols as usize, rows as usize);
        // Where each Edge cell touches the screen's border, in the order of the Track.
        let mut cells = Vec::with_capacity(2 * (cols_n + rows_n));
        for v in [0.0, 1.0] {
            cells.extend((0..cols_n).map(|c| ((c as f32 + 0.5) / cols_n as f32, v)));
        }
        for u in [0.0, 1.0] {
            cells.extend((0..rows_n).map(|r| (u, (r as f32 + 0.5) / rows_n as f32)));
        }
        // The Look's spread widens it, for a calmer Strip.
        let sigma = EDGE_SPREAD * look.spread.clamp(0.2, 5.0);
        let channels = channels
            .iter()
            .map(|ch| {
                let u = (ch.position.x + 1.0) / 2.0;
                let v = (1.0 - ch.position.z) / 2.0;
                let mut w = Vec::new();
                let mut total = 0.0;
                for (i, (cu, cv)) in cells.iter().enumerate() {
                    let d = (((cu - u) * 16.0).powi(2) + ((cv - v) * 9.0).powi(2)) / sigma.powi(2);
                    let k = (-0.5 * d).exp();
                    if k > 0.01 {
                        w.push((i, k));
                        total += k;
                    }
                }
                w.iter_mut().for_each(|(_, k)| *k /= total);
                (ch.channel_id, w)
            })
            .collect();
        Self { channels, made_for: (true, cols, rows, look.spread, look.fit_area) }
    }

    fn made_for(&self, track: &Track, look: &Look, edges: bool) -> bool {
        let size = if edges { (track.edge_cols, track.edge_rows) } else { (track.cols, track.rows) };
        self.made_for == (edges, size.0, size.1, look.spread, look.fit_area)
    }

    fn colors(&self, grid: &[Rgb], out: &mut Vec<(u8, Rgb)>) {
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

/// How far along the edge a Strip LED reads (the Gaussian's width), in
/// sixteenths of the screen's width: 0.15 is about half an Edge cell (64
/// along the top), so a LED mostly sees the cells right in front of it.
const EDGE_SPREAD: f32 = 0.15;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hue::Position;

    fn track(frames: Vec<u8>) -> Track {
        Track {
            fps: 10.0,
            cols: 1,
            rows: 1,
            source_size: 0,
            source: String::new(),
            frames,
            edge_cols: 0,
            edge_rows: 0,
            edges: Vec::new(),
            exposure: 1.0,
        }
    }

    #[test]
    fn smooths_within_a_track_and_restarts_on_a_new_one() {
        let ch = [Channel { channel_id: 3, position: Position { x: 0.0, y: 0.0, z: 0.0 } }];
        let look = Look { centered_smoothing: false, smoothing_ms: 500.0, warm_greys: 0.0, ..Look::default() };
        let mut r = Renderer::for_area(&ch);
        // A film that goes from black to white at 1.5 s.
        let cut = track([[0; 45], [255; 45]].concat());
        assert_eq!(r.colors(&cut, 1.0, &look, 0.02), [(3, [0.0; 3])]);
        let after = r.colors(&cut, 2.0, &look, 0.02)[0].1;
        assert!(after[0] > 0.0 && after[0] < 0.2, "smoothed toward white: {after:?}");
        // Another film: its first frame isn't held back by the previous one.
        let white = track(vec![255; 90]);
        let first = r.colors(&white, 1.0, &look, 0.02)[0].1;
        assert!(first[0] > 0.99, "{first:?}");
    }

    #[test]
    fn centered_smoothing_fades_around_the_cut() {
        let ch = [Channel { channel_id: 0, position: Position { x: 0.0, y: 0.0, z: 0.0 } }];
        let look = Look { smoothing_ms: 300.0, warm_greys: 0.0, saturation: 1.0, ..Look::default() };
        let cut = track([[0; 45], [255; 45]].concat());
        let mut r = Renderer::for_area(&ch);
        // Already lighter just before the cut, halfway on it: no delay.
        let before = r.colors(&cut, 1.4, &look, 0.02)[0].1[0];
        let on = r.colors(&cut, 1.45, &look, 0.02)[0].1[0];
        assert!(before > 0.0 && before < on, "{before} {on}");
    }

    #[test]
    fn strip_segments_around_the_tv_see_their_own_edge() {
        // A gradient strip's segments, as the Hue app places them: close to the center.
        let ch = |id, x, z| Channel { channel_id: id, position: Position { x, y: 0.0, z } };
        let m = Mapping::new(
            &[ch(0, -0.37, -0.47), ch(1, -0.37, -0.1), ch(2, -0.37, 0.27), ch(3, 0.0, 0.27), ch(6, 0.37, -0.47)],
            16,
            9,
            &Look::default(),
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

        // Without fit_area, the same segments read close to the center: much less contrast.
        let look = Look { fit_area: false, ..Look::default() };
        let m = Mapping::new(&[ch(0, -0.37, -0.47)], 16, 9, &look);
        m.colors(&grid, &mut out);
        assert!(out[0].1[2] < 0.9, "{:?}", out[0]);
    }

    #[test]
    fn strip_leds_see_the_edge_just_in_front_of_them() {
        let cfg = crate::config::WledConfig { top: 65, left: 35, right: 35, ..Default::default() };
        let leds = crate::wled::channels(&cfg).unwrap();
        let (cols, rows) = (64usize, 36usize);
        // Edges all blue, but a thin green band in the middle of the top, cells 31
        // and 32, and the left side red.
        let mut edges = vec![[0.0, 0.0, 1.0]; 2 * (cols + rows)];
        edges[31] = [0.0, 1.0, 0.0];
        edges[32] = [0.0, 1.0, 0.0];
        for cell in &mut edges[2 * cols..2 * cols + rows] {
            *cell = [1.0, 0.0, 0.0];
        }
        let m = Mapping::along_edges(&leds, cols as u8, rows as u8, &Look::default());
        let mut out = Vec::new();
        m.colors(&edges, &mut out);
        // Clockwise from the bottom left: the left side first (35 LEDs), then the top.
        let top_middle = out[35 + 32].1;
        assert!(top_middle[1] > 0.8, "the LED in front of the green band is green: {top_middle:?}");
        let top_quarter = out[35 + 16].1;
        assert!(top_quarter[2] > 0.99, "a quarter along, the top is blue: {top_quarter:?}");
        let left = out[17].1;
        assert!(left[0] > 0.99, "the left side is red: {left:?}");
        // On the grid, the same band is a tenth of a cell: lost in the blue around it.
        let grid: Vec<Rgb> = (0..16 * 9).map(|_| [0.0, 0.0, 1.0]).collect();
        let on_grid = Mapping::new(&leds, 16, 9, &Look::default());
        on_grid.colors(&grid, &mut out);
        assert!(out[35 + 32].1[1] < 0.01);
    }

    #[test]
    fn a_strip_reads_the_grid_of_a_track_without_edges() {
        let cfg = crate::config::WledConfig { top: 10, ..Default::default() };
        let leds = crate::wled::channels(&cfg).unwrap();
        let look = Look { warm_greys: 0.0, saturation: 1.0, ..Look::default() };
        let mut r = Renderer::for_strip(&leds);
        let red = Track { cols: 16, rows: 9, ..track([255, 0, 0].repeat(16 * 9 * 2)) };
        let colors = r.colors(&red, 0.0, &look, 0.02);
        assert_eq!(colors.len(), 10);
        assert!(colors.iter().all(|c| c.1[0] > 0.99), "{colors:?}");
    }

    #[test]
    fn left_light_sees_left_of_screen() {
        let ch = |id, x| Channel { channel_id: id, position: Position { x, y: 0.8, z: 0.0 } };
        let m = Mapping::new(&[ch(0, -1.0), ch(1, 1.0)], 16, 9, &Look::default());
        // Grid: left half red, right half blue.
        let grid: Vec<Rgb> = (0..16 * 9).map(|i| if i % 16 < 8 { [1.0, 0.0, 0.0] } else { [0.0, 0.0, 1.0] }).collect();
        let mut out = Vec::new();
        m.colors(&grid, &mut out);
        assert!(out[0].1[0] > 0.95 && out[0].1[2] < 0.05, "{:?}", out[0]);
        assert!(out[1].1[2] > 0.95 && out[1].1[0] < 0.05, "{:?}", out[1]);
    }
}

