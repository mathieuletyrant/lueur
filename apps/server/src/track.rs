//! Color track: for every moment of the film, a small grid of colors
//! (16×9 by default) computed once and for all during analysis.
//!
//! `.lueur` file format:
//! `LUEUR` | version u8 | fps f32 | cols u8 | rows u8 | frames u32 |
//! source size u64 | len u16 | source path (UTF-8) | zstd(sRGB u8 grids)

use crate::color::{Rgb, linear_to_srgb, luminance, srgb_to_linear};
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 5] = b"LUEUR";
const VERSION: u8 = 1;

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
    /// Brightness gain that lifts a dark film to a usual level (≥ 1), set by `load`.
    pub exposure: f32,
}

impl Track {
    pub fn cells(&self) -> usize {
        self.cols as usize * self.rows as usize
    }

    pub fn frame_len(&self) -> usize {
        self.cells() * 3
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
        let src = self.source.as_bytes();
        buf.extend_from_slice(&(src.len() as u16).to_le_bytes());
        buf.extend_from_slice(src);
        buf.extend_from_slice(&zstd::encode_all(&self.frames[..], 9)?);

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
        if data[5] != VERSION {
            bail!("unsupported track version {}", data[5]);
        }
        let fps = f32::from_le_bytes(data[6..10].try_into()?);
        let (cols, rows) = (data[10], data[11]);
        let count = u32::from_le_bytes(data[12..16].try_into()?) as usize;
        let source_size = u64::from_le_bytes(data[16..24].try_into()?);
        let src_len = u16::from_le_bytes(data[24..26].try_into()?) as usize;
        let source = String::from_utf8(data[26..26 + src_len].to_vec())?;
        let frames = zstd::decode_all(&data[26 + src_len..])?;
        let mut t = Track { fps, cols, rows, source_size, source, frames, exposure: 1.0 };
        t.exposure = t.measure_exposure();
        if t.frame_count() != count {
            bail!("truncated track ({} frames instead of {count})", t.frame_count());
        }
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
        let n = self.frame_count();
        out.clear();
        if n == 0 {
            out.resize(self.cells(), [0.0; 3]);
            return;
        }
        let f = (t.max(0.0) * self.fps as f64).min((n - 1) as f64);
        let i = f.floor() as usize;
        let j = (i + 1).min(n - 1);
        let k = (f - i as f64) as f32;
        let (a, b) = (self.frame(i), self.frame(j));
        for c in 0..self.cells() {
            let mut px = [0f32; 3];
            for ch in 0..3 {
                let va = srgb_to_linear(a[c * 3 + ch]);
                let vb = srgb_to_linear(b[c * 3 + ch]);
                px[ch] = va + (vb - va) * k;
            }
            out.push(px);
        }
    }

    /// Like `sample`, averaged over `t ± half` seconds with triangular
    /// weights: a cut becomes a fade centered on it, with no delay.
    pub fn sample_centered(&self, t: f64, half: f64, out: &mut Vec<Rgb>) {
        let step = 1.0 / self.fps as f64;
        let m = (half / step).ceil() as i32;
        if m < 1 {
            return self.sample(t, out);
        }
        let mut acc = vec![[0f32; 3]; self.cells()];
        let mut total = 0.0;
        let mut g = Vec::new();
        for i in -m..=m {
            let w = 1.0 - i.abs() as f32 / (m + 1) as f32;
            self.sample(t + i as f64 * step, &mut g);
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

    pub fn frame(&self, i: usize) -> &[u8] {
        let l = self.frame_len();
        &self.frames[i * l..(i + 1) * l]
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

/// A track exists and still matches the file (same size).
pub fn is_fresh(store: &Path, video: &Path) -> bool {
    fresh_duration(store, video).is_some()
}

/// Duration in seconds of the video's track, when it is up to date (header only).
pub fn fresh_duration(store: &Path, video: &Path) -> Option<f64> {
    let meta = std::fs::metadata(video).ok()?;
    let mut head = [0u8; 24];
    std::fs::File::open(track_path(store, video)).ok()?.read_exact(&mut head).ok()?;
    if &head[..5] != MAGIC || u64::from_le_bytes(head[16..24].try_into().ok()?) != meta.len() {
        return None;
    }
    let fps = f32::from_le_bytes(head[6..10].try_into().ok()?);
    let frames = u32::from_le_bytes(head[12..16].try_into().ok()?);
    (fps > 0.0).then(|| frames as f64 / fps as f64)
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
        let t =
            Track { fps: 10.0, cols: 2, rows: 2, source_size: 42, source: "/x/film.mkv".into(), frames, exposure: 1.0 };
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
            exposure: 1.0,
        };
        let mut g = Vec::new();
        cut.sample_centered(0.95, 0.3, &mut g);
        assert!((g[0][0] - 0.5).abs() < 0.05, "{:?}", g[0]);
        cut.sample_centered(0.7, 0.3, &mut g);
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
}
