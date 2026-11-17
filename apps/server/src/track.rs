//! Color track: for every moment of the film, a small grid of colors
//! (16×9) and the Edges of the picture in finer cells, computed once and for
//! all during analysis.
//!
//! `.lueur` file format, version 3 (same layout as 2; HDR videos tone-mapped):
//! `LUEUR` | version u8 | fps f32 | cols u8 | rows u8 | frames u32 |
//! source size u64 | edge cols u8 | edge rows u8 | len u16 | source path (UTF-8) |
//! zstd(grids, then Edges, each frame stored as its difference from the previous one)
//!
//! Version 1 has no edge sizes, no Edges, and frames stored as they are. It
//! still plays, but is no longer fresh: the next Scan analyzes the video again.

use crate::color::{Rgb, linear_to_srgb, luminance, srgb_to_linear};
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 5] = b"LUEUR";
/// 3: HDR videos are tone-mapped at Analysis (same layout as 2).
const VERSION: u8 = 3;
/// A frame that differs this much from the one before (mean over the grid, 0
/// to 1) may be a cut…
const CUT_CHANGE: f32 = 0.08;
/// …if it changes this many times more than the frames around it.
const CUT_CONTRAST: f32 = 2.5;
/// A Flash lasts at most this long (seconds)…
const FLASH_MAX: f64 = 0.15;
/// …and is at least this many times as bright as the frame before it…
const FLASH_RATIO: f32 = 2.0;
/// …and at least this much brighter (mean linear luminance, 0 to 1).
const FLASH_RISE: f32 = 0.05;

#[derive(Debug, Clone)]
pub struct Track {
    pub fps: f32,
    pub cols: u8,
    pub rows: u8,
    /// Size of the video file at analysis time (detects replaced files).
    pub source_size: u64,
    pub source: String,
    /// Concatenated grids, `cols * rows * 3` sRGB bytes per frame.
    pub frames: Vec<u8>,
    /// Cells along the top and along the bottom of the picture (0 on a version 1 Track).
    pub edge_cols: u8,
    /// Cells along the left and along the right side.
    pub edge_rows: u8,
    /// Concatenated Edges, `edge_cells() * 3` sRGB bytes per frame, in the
    /// order of `edge_cells`: top, bottom (left to right), left, right (top to bottom).
    pub edges: Vec<u8>,
    /// Brightness gain that lifts a dark film to a usual level (≥ 1), set by `load`.
    pub exposure: f32,
    /// Cuts between two shots, in seconds from the start, sorted; found by `load`.
    pub cuts: Vec<f64>,
    /// Per frame, the gain that brings a Flash back to the level around it
    /// (1 elsewhere); empty when the film has none. Found by `load`.
    pub flash_gains: Vec<f32>,
}

impl Track {
    pub fn cells(&self) -> usize {
        self.cols as usize * self.rows as usize
    }

    pub fn frame_len(&self) -> usize {
        self.cells() * 3
    }

    pub fn edge_cells(&self) -> usize {
        2 * (self.edge_cols as usize + self.edge_rows as usize)
    }

    pub fn has_edges(&self) -> bool {
        self.edge_cells() > 0 && !self.edges.is_empty()
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len() / self.frame_len()
    }

    pub fn duration(&self) -> f64 {
        self.frame_count() as f64 / self.fps as f64
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let mut buf = Vec::with_capacity(64 + self.source.len());
        buf.extend_from_slice(MAGIC);
        buf.push(VERSION);
        buf.extend_from_slice(&self.fps.to_le_bytes());
        buf.push(self.cols);
        buf.push(self.rows);
        buf.extend_from_slice(&(self.frame_count() as u32).to_le_bytes());
        buf.extend_from_slice(&self.source_size.to_le_bytes());
        buf.push(self.edge_cols);
        buf.push(self.edge_rows);
        let src = self.source.as_bytes();
        buf.extend_from_slice(&(src.len() as u16).to_le_bytes());
        buf.extend_from_slice(src);
        // Two frames in a row are almost the same: their difference is mostly
        // zeros, which zstd packs far better than the colors themselves.
        let mut body = differences(&self.frames, self.frame_len());
        body.extend(differences(&self.edges, self.edge_cells() * 3));
        buf.extend_from_slice(&zstd::encode_all(&body[..], 9)?);

        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        // Atomic write: the player never reads a half-written track.
        let tmp = path.with_extension("lueur.tmp");
        std::fs::File::create(&tmp)?.write_all(&buf)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Track> {
        let mut data = Vec::new();
        std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?.read_to_end(&mut data)?;
        if data.len() < 25 || &data[..5] != MAGIC {
            bail!("{} is not a lueur track", path.display());
        }
        let version = data[5];
        if !(1..=VERSION).contains(&version) {
            bail!("unsupported track version {version}");
        }
        let fps = f32::from_le_bytes(data[6..10].try_into()?);
        let (cols, rows) = (data[10], data[11]);
        let count = u32::from_le_bytes(data[12..16].try_into()?) as usize;
        let source_size = u64::from_le_bytes(data[16..24].try_into()?);
        // Version 1 has no edge sizes: its source path comes right after the file size.
        let (edge_cols, edge_rows, at) = if version == 1 { (0, 0, 24) } else { (data[24], data[25], 26) };
        let src_len = u16::from_le_bytes(data.get(at..at + 2).context("truncated track")?.try_into()?) as usize;
        let source = String::from_utf8(data.get(at + 2..at + 2 + src_len).context("truncated track")?.to_vec())?;
        let mut body = zstd::decode_all(&data[at + 2 + src_len..])?;
        let mut t = Track {
            fps,
            cols,
            rows,
            source_size,
            source,
            frames: Vec::new(),
            edge_cols,
            edge_rows,
            edges: Vec::new(),
            exposure: 1.0,
            cuts: Vec::new(),
            flash_gains: Vec::new(),
        };
        let grids = count * t.frame_len();
        if body.len() != grids + count * t.edge_cells() * 3 {
            bail!("truncated track (expected {count} frames)");
        }
        t.edges = body.split_off(grids);
        t.frames = body;
        if version != 1 {
            let (grid_len, edge_len) = (t.frame_len(), t.edge_cells() * 3);
            undo_differences(&mut t.frames, grid_len);
            undo_differences(&mut t.edges, edge_len);
        }
        t.exposure = t.measure_exposure();
        t.cuts = t.find_cuts();
        t.flash_gains = t.find_flashes();
        Ok(t)
    }

    /// How much to brighten this film so its usual brightest area reaches a
    /// common level: the 75th percentile, over the film, of each moment's
    /// brightest cell. Only ever brightens, at most 3×.
    fn measure_exposure(&self) -> f32 {
        let n = self.frame_count();
        let mut levels: Vec<f32> = (0..n)
            .step_by(6)
            .map(|i| {
                self.frame(i)
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|c| luminance([srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2])]))
                    .fold(0.0, f32::max)
            })
            .collect();
        if levels.is_empty() {
            return 1.0;
        }
        let k = levels.len() * 3 / 4;
        let p75 = *levels.select_nth_unstable_by(k, f32::total_cmp).1;
        (0.6 / p75.max(1e-3)).clamp(1.0, 3.0)
    }

    /// Where the picture changes at once, from one frame to the next, and
    /// much more than around it: a cut between two shots, not a pan or a fade.
    /// A cut sits halfway between its two frames, where `sample` crosses it.
    fn find_cuts(&self) -> Vec<f64> {
        let n = self.frame_count();
        // How much each frame differs from the one before (0 to 1, mean over the grid).
        let change: Vec<f32> = (0..n)
            .map(|i| {
                if i == 0 {
                    return 0.0;
                }
                let (a, b) = (self.frame(i - 1), self.frame(i));
                let sum: u32 = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b) as u32).sum();
                sum as f32 / (a.len().max(1) as f32 * 255.0)
            })
            .collect();
        (1..n)
            .filter(|&i| {
                let around = change[i - 1].max(change.get(i + 1).copied().unwrap_or(0.0));
                change[i] >= CUT_CHANGE && change[i] >= CUT_CONTRAST * around
            })
            .map(|i| (i as f64 - 0.5) / self.fps as f64)
            .collect()
    }

    /// Flashes (an explosion, lightning, a camera flash): a few frames much
    /// brighter than the frames before and after them. Each gets the gain that
    /// brings it back to the brighter of those two; a bright scene that lasts
    /// is no Flash. Empty when there is none.
    fn find_flashes(&self) -> Vec<f32> {
        let n = self.frame_count();
        let level: Vec<f32> = (0..n)
            .map(|i| {
                let cells = self.frame(i).as_chunks::<3>().0;
                let sum: f32 = cells
                    .iter()
                    .map(|c| luminance([srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2])]))
                    .sum();
                sum / cells.len().max(1) as f32
            })
            .collect();
        let longest = ((FLASH_MAX * self.fps as f64).round() as usize).max(1);
        let mut gains = Vec::new();
        let mut i = 1;
        while i < n {
            let before = level[i - 1];
            let bright = |l: f32| l >= before * FLASH_RATIO && l - before >= FLASH_RISE;
            if !bright(level[i]) {
                i += 1;
                continue;
            }
            // The Flash's frames, up to the first one back near the level before.
            let end = (i..n).find(|&j| !bright(level[j])).unwrap_or(n);
            if end < n && end - i <= longest {
                if gains.is_empty() {
                    gains = vec![1.0; n];
                }
                let around = before.max(level[end]);
                for j in i..end {
                    gains[j] = (around / level[j]).min(1.0);
                }
            }
            i = end.max(i + 1);
        }
        gains
    }

    /// The cells the lights read at `t`, in linear light: the Edges or the
    /// grid, averaged over `t ± half` with triangular weights when `half` is
    /// set (a cut becomes a fade centered on it, with no delay), with Flashes toned
    /// down by `flash_guard` (0: as they are, 1: back to the level around them).
    pub fn sample_for_lights(&self, t: f64, half: Option<f64>, edges: bool, flash_guard: f32, out: &mut Vec<Rgb>) {
        let (data, cells) = if edges { (&self.edges, self.edge_cells()) } else { (&self.frames, self.cells()) };
        let gains = Gains { gains: &self.flash_gains, guard: flash_guard.clamp(0.0, 1.0) };
        match half {
            Some(half) => sample_cells_centered(data, cells, self.fps, t, half, &gains, out),
            None => sample_cells(data, cells, self.fps, t, &gains, out),
        }
    }

    /// How far `t` is from the nearest cut (seconds), if the film has any.
    pub fn to_nearest_cut(&self, t: f64) -> Option<f64> {
        let i = self.cuts.partition_point(|c| *c < t);
        let after = self.cuts.get(i).map(|c| c - t);
        let before = i.checked_sub(1).map(|j| t - self.cuts[j]);
        match (before, after) {
            (Some(b), Some(a)) => Some(b.min(a)),
            (b, a) => b.or(a),
        }
    }

    /// The film's colors from start to end: `n` stripes, each the average of
    /// its stretch of the film.
    pub fn barcode(&self, n: usize) -> Vec<[u8; 3]> {
        let frames = self.frame_count();
        (0..n)
            .map(|i| {
                let (a, b) = (i * frames / n, ((i + 1) * frames / n).max(i * frames / n + 1).min(frames));
                let mut acc = [0f32; 3];
                let mut count = 0f32;
                for f in (a..b).step_by(((b - a) / 8).max(1)) {
                    for c in self.frame(f).as_chunks::<3>().0 {
                        for ch in 0..3 {
                            acc[ch] += srgb_to_linear(c[ch]);
                        }
                        count += 1.0;
                    }
                }
                acc.map(|v| (linear_to_srgb(v / count.max(1.0)) * 255.0).round() as u8)
            })
            .collect()
    }

    /// How bright the whole picture is at `t` (perceptual, 0 to 1), as
    /// analyzed: no smoothing, no Look.
    pub fn brightness(&self, t: f64) -> f32 {
        let mut grid = Vec::new();
        self.sample(t, &mut grid);
        let total: f32 = grid.iter().map(|cell| luminance(*cell)).sum();
        linear_to_srgb(total / grid.len().max(1) as f32)
    }

    /// Grid in linear light at time `t` (seconds), interpolated between
    /// the two nearest analyzed frames.
    pub fn sample(&self, t: f64, out: &mut Vec<Rgb>) {
        sample_cells(&self.frames, self.cells(), self.fps, t, &Gains::NONE, out);
    }

    pub fn frame(&self, i: usize) -> &[u8] {
        let l = self.frame_len();
        &self.frames[i * l..(i + 1) * l]
    }
}

/// Per-frame gains (the Flash guard), applied as strongly as `guard` says.
struct Gains<'a> {
    gains: &'a [f32],
    guard: f32,
}

impl Gains<'_> {
    const NONE: Gains<'static> = Gains { gains: &[], guard: 0.0 };

    fn at(&self, frame: usize) -> f32 {
        match self.gains.get(frame) {
            Some(g) if self.guard > 0.0 => 1.0 - self.guard * (1.0 - g),
            _ => 1.0,
        }
    }
}

/// The cells of `data` (frames of `cells` sRGB colors) at `t`, in linear
/// light, interpolated between the two nearest frames.
fn sample_cells(data: &[u8], cells: usize, fps: f32, t: f64, gains: &Gains, out: &mut Vec<Rgb>) {
    out.clear();
    let len = cells * 3;
    let n = data.len().checked_div(len).unwrap_or(0);
    if n == 0 {
        out.resize(cells, [0.0; 3]);
        return;
    }
    let f = (t.max(0.0) * fps as f64).min((n - 1) as f64);
    let i = f.floor() as usize;
    let j = (i + 1).min(n - 1);
    let k = (f - i as f64) as f32;
    let (a, b) = (&data[i * len..(i + 1) * len], &data[j * len..(j + 1) * len]);
    let (ga, gb) = (gains.at(i), gains.at(j));
    for c in 0..cells {
        let mut px = [0f32; 3];
        for ch in 0..3 {
            let va = srgb_to_linear(a[c * 3 + ch]) * ga;
            let vb = srgb_to_linear(b[c * 3 + ch]) * gb;
            px[ch] = va + (vb - va) * k;
        }
        out.push(px);
    }
}

/// `sample_cells` averaged over `t ± half` seconds with triangular weights,
/// down to zero at `± half`: the average changes smoothly with `half`, so a
/// wider window shows no jump.
fn sample_cells_centered(data: &[u8], cells: usize, fps: f32, t: f64, half: f64, gains: &Gains, out: &mut Vec<Rgb>) {
    let step = 1.0 / fps as f64;
    let m = (half / step).floor() as i32;
    if m < 1 {
        return sample_cells(data, cells, fps, t, gains, out);
    }
    let mut acc = vec![[0f32; 3]; cells];
    let mut total = 0.0;
    let mut g = Vec::new();
    for i in -m..=m {
        let w = (1.0 - i.abs() as f64 * step / half) as f32;
        if w <= 0.0 {
            continue;
        }
        sample_cells(data, cells, fps, t + i as f64 * step, gains, &mut g);
        for (a, c) in acc.iter_mut().zip(&g) {
            for ch in 0..3 {
                a[ch] += c[ch] * w;
            }
        }
        total += w;
    }
    out.clear();
    out.extend(acc.iter().map(|a| a.map(|v| v / total)));
}

/// Each frame of `len` bytes minus the frame before it (the first one as it is).
fn differences(data: &[u8], len: usize) -> Vec<u8> {
    let mut out = data.to_vec();
    if len == 0 {
        return out;
    }
    for i in (len..data.len()).rev() {
        out[i] = data[i].wrapping_sub(data[i - len]);
    }
    out
}

/// Undoes `differences`, in place.
fn undo_differences(data: &mut [u8], len: usize) {
    if len == 0 {
        return;
    }
    for i in len..data.len() {
        data[i] = data[i].wrapping_add(data[i - len]);
    }
}

/// Location of a video file's track in the track store.
pub fn track_path(store: &Path, video: &Path) -> PathBuf {
    // Absolute path: command-line analysis and the player must land on the same track.
    let canon = std::fs::canonicalize(video);
    let video = canon.as_deref().unwrap_or(video);
    // 64-bit FNV-1a: stable across versions, unlike std's hasher.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in video.to_string_lossy().as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let stem = video
        .file_stem()
        .map(|s| s.to_string_lossy().chars().filter(|c| c.is_alphanumeric()).take(40).collect::<String>())
        .unwrap_or_default();
    store.join(format!("{stem}-{h:016x}.lueur"))
}

/// A track exists, still matches the file (same size) and is of the current
/// version, or of version 2 for an SDR video (version 3 only changed HDR):
/// nothing to analyze again.
pub fn is_fresh(store: &Path, video: &Path) -> bool {
    fresh_duration(store, video).is_some()
}

/// Duration in seconds of the video's track, when it is fresh (header only).
pub fn fresh_duration(store: &Path, video: &Path) -> Option<f64> {
    let (version, duration) = header(store, video)?;
    (version == VERSION || (version == 2 && !crate::analyze::is_hdr(video))).then_some(duration)
}

/// A track exists and still matches the file, maybe of an older version: it plays.
pub fn is_playable(store: &Path, video: &Path) -> bool {
    header(store, video).is_some()
}

/// The version and duration of the video's track, when it matches the file.
fn header(store: &Path, video: &Path) -> Option<(u8, f64)> {
    let meta = std::fs::metadata(video).ok()?;
    let mut head = [0u8; 24];
    std::fs::File::open(track_path(store, video)).ok()?.read_exact(&mut head).ok()?;
    if &head[..5] != MAGIC || u64::from_le_bytes(head[16..24].try_into().ok()?) != meta.len() {
        return None;
    }
    let fps = f32::from_le_bytes(head[6..10].try_into().ok()?);
    let frames = u32::from_le_bytes(head[12..16].try_into().ok()?);
    (fps > 0.0).then(|| (head[5], frames as f64 / fps as f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_sample() {
        let dir = std::env::temp_dir().join(format!("lueur-test-{}", std::process::id()));
        let mut frames = vec![0u8; 2 * 2 * 3 * 2];
        frames[..12].fill(0); // frame 0 black
        frames[12..].fill(255); // frame 1 white
        let t = Track {
            fps: 10.0,
            cols: 2,
            rows: 2,
            source_size: 42,
            source: "/x/film.mkv".into(),
            frames,
            edge_cols: 0,
            edge_rows: 0,
            edges: Vec::new(),
            exposure: 1.0,
            cuts: Vec::new(),
            flash_gains: Vec::new(),
        };
        let p = dir.join("t.lueur");
        t.save(&p).unwrap();
        let l = Track::load(&p).unwrap();
        assert_eq!(l.frame_count(), 2);
        assert_eq!(l.source, "/x/film.mkv");
        let mut g = Vec::new();
        l.sample(0.05, &mut g); // halfway between the two frames
        assert!((g[0][0] - 0.5).abs() < 1e-3);
        l.sample(99.0, &mut g); // past the end: last frame
        assert!((g[3][2] - 1.0).abs() < 1e-6);
        // Centered: halfway through the fade exactly on the cut, already moving before it.
        let cut = Track {
            fps: 10.0,
            cols: 1,
            rows: 1,
            source_size: 0,
            source: String::new(),
            frames: [[0; 30], [255; 30]].concat(),
            edge_cols: 0,
            edge_rows: 0,
            edges: Vec::new(),
            exposure: 1.0,
            cuts: Vec::new(),
            flash_gains: Vec::new(),
        };
        let mut g = Vec::new();
        cut.sample_for_lights(0.95, Some(0.3), false, 0.0, &mut g);
        assert!((g[0][0] - 0.5).abs() < 0.05, "{:?}", g[0]);
        cut.sample_for_lights(0.75, Some(0.3), false, 0.0, &mut g);
        assert!(g[0][0] > 0.0 && g[0][0] < 0.2, "{:?}", g[0]);
        // Barcode: black then white.
        assert_eq!(cut.barcode(2), vec![[0; 3], [255; 3]]);
        // Exposure: a dim film is lifted, a bright one left alone.
        let dim = Track { frames: vec![60; 3 * 100], ..cut.clone() };
        assert!(dim.measure_exposure() > 2.0, "{}", dim.measure_exposure());
        let bright = Track { frames: vec![230; 3 * 100], ..cut.clone() };
        assert_eq!(bright.measure_exposure(), 1.0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn edges_survive_saving_and_version_1_still_loads() {
        let dir = std::env::temp_dir().join(format!("lueur-test-edges-{}", std::process::id()));
        // 2 Edge cols and 1 Edge row: 6 cells; 3 frames, each brighter than the last.
        let edges: Vec<u8> = (0..3u8).flat_map(|f| (0..18u8).map(move |i| f * 50 + i)).collect();
        let t = Track {
            fps: 10.0,
            cols: 1,
            rows: 1,
            source_size: 7,
            source: "/x/film.mkv".into(),
            frames: vec![10, 20, 30, 200, 100, 0, 255, 255, 255],
            edge_cols: 2,
            edge_rows: 1,
            edges: edges.clone(),
            exposure: 1.0,
            cuts: Vec::new(),
            flash_gains: Vec::new(),
        };
        let p = dir.join("t.lueur");
        t.save(&p).unwrap();
        let l = Track::load(&p).unwrap();
        assert_eq!((l.edge_cols, l.edge_rows, l.edge_cells()), (2, 1, 6));
        assert_eq!(l.frames, t.frames);
        assert_eq!(l.edges, edges);
        let mut g = Vec::new();
        l.sample_for_lights(0.2, None, true, 0.0, &mut g);
        assert_eq!(g.len(), 6);
        assert!((g[0][0] - srgb_to_linear(100)).abs() < 1e-6, "{:?}", g[0]);

        // A version 1 file, as 0.19 wrote it: grids only, stored as they are.
        let mut v1 = b"LUEUR".to_vec();
        v1.push(1);
        v1.extend_from_slice(&10f32.to_le_bytes());
        v1.extend_from_slice(&[1, 1]);
        v1.extend_from_slice(&2u32.to_le_bytes());
        v1.extend_from_slice(&7u64.to_le_bytes());
        v1.extend_from_slice(&3u16.to_le_bytes());
        v1.extend_from_slice(b"/x/");
        v1.extend_from_slice(&zstd::encode_all(&[0u8, 0, 0, 255, 255, 255][..], 9).unwrap());
        std::fs::write(&p, &v1).unwrap();
        let old = Track::load(&p).unwrap();
        assert_eq!((old.frame_count(), old.has_edges(), old.source.as_str()), (2, false, "/x/"));
        assert_eq!(old.frame(1), [255, 255, 255]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn finds_cuts_not_pans_or_fades() {
        // 10 fps, one cell: a slow fade up (0 → 90), a cut to white, a pan-like
        // drift, then a cut back to black.
        let mut frames: Vec<u8> = (0..10u8).map(|i| i * 10).collect();
        frames.extend([250, 248, 246, 244, 242, 240]);
        frames.extend([0, 0, 0, 0]);
        let t = Track {
            fps: 10.0,
            cols: 1,
            rows: 1,
            source_size: 0,
            source: String::new(),
            frames: frames.iter().flat_map(|v| [*v; 3]).collect(),
            edge_cols: 0,
            edge_rows: 0,
            edges: Vec::new(),
            exposure: 1.0,
            cuts: Vec::new(),
            flash_gains: Vec::new(),
        };
        let cuts = t.find_cuts();
        assert_eq!(cuts.len(), 2, "{cuts:?}");
        assert!((cuts[0] - 0.95).abs() < 1e-9 && (cuts[1] - 1.55).abs() < 1e-9, "{cuts:?}");
        let t = Track { cuts, ..t };
        assert!((t.to_nearest_cut(1.0).unwrap() - 0.05).abs() < 1e-9);
        assert!((t.to_nearest_cut(1.5).unwrap() - 0.05).abs() < 1e-9);
        assert!((t.to_nearest_cut(0.0).unwrap() - 0.95).abs() < 1e-9);
        assert_eq!(Track { cuts: Vec::new(), ..t }.to_nearest_cut(1.0), None);
    }

    #[test]
    fn a_short_flash_is_toned_down_a_lasting_bright_scene_is_not() {
        // 24 fps, one cell: dark, a 2-frame white flash, dark, then a white scene that lasts.
        let mut levels = vec![40u8; 24];
        levels.extend([255, 255]);
        levels.extend(vec![40u8; 22]);
        levels.extend(vec![255u8; 24]);
        let t = Track {
            fps: 24.0,
            cols: 1,
            rows: 1,
            source_size: 0,
            source: String::new(),
            frames: levels.iter().flat_map(|v| [*v; 3]).collect(),
            edge_cols: 0,
            edge_rows: 0,
            edges: Vec::new(),
            exposure: 1.0,
            cuts: Vec::new(),
            flash_gains: Vec::new(),
        };
        let gains = t.find_flashes();
        assert_eq!(gains.len(), levels.len());
        assert!(gains[24] < 0.1 && gains[25] < 0.1, "{:?}", &gains[22..28]);
        assert!(gains[..24].iter().chain(&gains[26..]).all(|g| *g == 1.0));
        let t = Track { flash_gains: gains, ..t };
        let mut g = Vec::new();
        let dark = srgb_to_linear(40);
        // Full guard: the flash is back to the level around it.
        t.sample_for_lights(24.5 / 24.0, None, false, 1.0, &mut g);
        assert!((g[0][0] - dark).abs() < 1e-3, "{:?}", g[0]);
        // Half: halfway; off: the flash as it is.
        t.sample_for_lights(24.5 / 24.0, None, false, 0.5, &mut g);
        assert!((g[0][0] - (dark + 1.0) / 2.0).abs() < 0.02, "{:?}", g[0]);
        t.sample_for_lights(24.5 / 24.0, None, false, 0.0, &mut g);
        assert!(g[0][0] > 0.99);
        // The lasting white scene stays white.
        t.sample_for_lights(3.5, None, false, 1.0, &mut g);
        assert!(g[0][0] > 0.99);
        // No Flash in a film without any.
        assert!(Track { frames: vec![40; 3 * 48], flash_gains: Vec::new(), ..t }.find_flashes().is_empty());
    }
}
