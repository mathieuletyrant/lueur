//! Video file analysis: ffmpeg decodes (in hardware when possible) and
//! shrinks each frame to 128×72 right on the GPU; Rust only sees ~28 KB per
//! frame, detects black bars and computes the grid and the Edges.

use crate::color::{hdr_to_srgb, linear_to_srgb, srgb_to_linear};
use crate::track::Track;
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

pub const SAMPLE_W: usize = 128;
pub const SAMPLE_H: usize = 72;
/// How deep into the picture an Edge cell reads, as a share of its height.
const EDGE_DEPTH: f32 = 0.1;
const RENDER_NODE: &str = "/dev/dri/renderD128";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum HwAccel {
    /// VideoToolbox on macOS, VA-API when a GPU is present, software otherwise.
    Auto,
    /// VA-API (Intel/AMD), recommended on the N100.
    Vaapi,
    /// Intel Quick Sync.
    Qsv,
    /// VideoToolbox (macOS).
    Videotoolbox,
    /// Software decoding.
    None,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub fps: f32,
    pub cols: u8,
    pub rows: u8,
    /// Edge cells along the top and the bottom.
    pub edge_cols: u8,
    /// Edge cells along each side.
    pub edge_rows: u8,
    pub hwaccel: HwAccel,
    pub threads: u32,
    /// Incremented for every analyzed frame (progress reporting).
    pub progress: Option<Arc<AtomicUsize>>,
    /// Set to stop the analysis early (it then fails with "stopped").
    pub cancel: Option<Arc<AtomicBool>>,
    /// Set when the analysis decodes on the CPU (no GPU, or a codec the GPU can't take).
    pub on_cpu: Option<Arc<AtomicBool>>,
    /// Fastest decoding, in times real time (keeps the server cool).
    pub max_speed: Option<f32>,
}

impl Options {
    fn cancelled(&self) -> bool {
        self.cancel.as_ref().is_some_and(|c| c.load(Ordering::Relaxed))
    }
}

pub struct Stats {
    pub elapsed: f64,
    pub hwaccel: HwAccel,
}

pub fn analyze(video: &Path, opts: &Options) -> Result<(Track, Stats)> {
    let hw = match opts.hwaccel {
        HwAccel::Auto if cfg!(target_os = "macos") => HwAccel::Videotoolbox,
        HwAccel::Auto if Path::new(RENDER_NODE).exists() => HwAccel::Vaapi,
        HwAccel::Auto => HwAccel::None,
        other => other,
    };
    let cpu = |on: bool| {
        if let Some(c) = &opts.on_cpu {
            c.store(on, Ordering::Relaxed);
        }
    };
    cpu(hw == HwAccel::None);
    match run(video, opts, hw) {
        Ok(r) => Ok(r),
        // Codec not supported by the GPU (or missing driver): fall back to software.
        Err(e) if hw != HwAccel::None && opts.hwaccel == HwAccel::Auto && !opts.cancelled() => {
            let name = video.file_name().unwrap_or_default().to_string_lossy();
            tracing::warn!("hardware decoding failed for {name} ({e:#}), falling back to software");
            cpu(true);
            run(video, opts, HwAccel::None)
        }
        Err(e) => Err(e),
    }
}

/// What ffprobe says of the first real video stream (not a cover picture).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Probe {
    pub codec: Option<String>,
    /// HDR10 / Dolby Vision with an HDR10 base (PQ transfer, BT.2020 colors):
    /// decoded as is, it reads washed out and far too dim, so it is tone-mapped.
    pub hdr: bool,
}

pub fn probe(video: &Path) -> Probe {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "V:0", "-show_entries", "stream=codec_name,color_transfer"])
        .args(["-of", "default=noprint_wrappers=1"])
        .arg(video)
        .output();
    out.map(|o| parse_probe(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default()
}

fn parse_probe(text: &str) -> Probe {
    let mut p = Probe::default();
    for line in text.lines() {
        match line.trim().split_once('=') {
            Some(("codec_name", c)) if !c.is_empty() => p.codec = Some(c.to_string()),
            Some(("color_transfer", t)) => p.hdr = t == "smpte2084",
            _ => {}
        }
    }
    p
}

/// Whether the video is HDR, asked once per file (and size) for the life of the process.
pub fn is_hdr(video: &Path) -> bool {
    static SEEN: OnceLock<Mutex<HashMap<(PathBuf, u64), bool>>> = OnceLock::new();
    let Ok(size) = std::fs::metadata(video).map(|m| m.len()) else { return false };
    let key = (video.to_path_buf(), size);
    if let Some(h) = SEEN.get_or_init(Default::default).lock().unwrap().get(&key) {
        return *h;
    }
    let hdr = probe(video).hdr;
    SEEN.get_or_init(Default::default).lock().unwrap().insert(key, hdr);
    hdr
}

/// Video duration in seconds, from ffprobe.
pub fn duration(video: &Path) -> Option<f64> {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0"])
        .arg(video)
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

fn ffmpeg_args(video: &Path, opts: &Options, hw: HwAccel, probe: &Probe) -> Vec<String> {
    let fps = opts.fps;
    // HDR keeps its 10 bits down to Rust, which tone-maps (`color::hdr_to_srgb`).
    let gpu_fmt = if probe.hdr { "p010" } else { "nv12" };
    let cpu_fmt = if probe.hdr { "p010le" } else { "nv12" };
    let mut a: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostdin".into()];
    let filter = match hw {
        HwAccel::Vaapi => {
            a.extend(
                ["-hwaccel", "vaapi", "-hwaccel_device", RENDER_NODE, "-hwaccel_output_format", "vaapi"]
                    .map(String::from),
            );
            format!("fps={fps},scale_vaapi=w={SAMPLE_W}:h={SAMPLE_H}:format={gpu_fmt},hwdownload,format={cpu_fmt}")
        }
        HwAccel::Qsv => {
            a.extend(["-hwaccel", "qsv", "-hwaccel_output_format", "qsv"].map(String::from));
            format!("fps={fps},vpp_qsv=w={SAMPLE_W}:h={SAMPLE_H}:format={gpu_fmt},hwdownload,format={cpu_fmt}")
        }
        HwAccel::Videotoolbox => {
            // Decoded on the Mac's GPU, frames copied back then shrunk (negligible cost).
            a.extend(["-hwaccel", "videotoolbox"].map(String::from));
            format!("fps={fps},scale={SAMPLE_W}:{SAMPLE_H}:flags=area")
        }
        _ => {
            // Tiny output image: the loop filter is useless and expensive.
            a.extend(["-skip_loop_filter", "all"].map(String::from));
            if opts.threads > 0 {
                a.extend(["-threads".into(), opts.threads.to_string()]);
            }
            format!("fps={fps},scale={SAMPLE_W}:{SAMPLE_H}:flags=area")
        }
    };
    // ffmpeg prefers libdav1d for AV1, which never uses the GPU: ask for its own decoder.
    if hw == HwAccel::Vaapi && probe.codec.as_deref() == Some("av1") {
        a.extend(["-c:v", "av1"].map(String::from));
    }
    // Frames no other frame refers to (most B-frames) are skipped: about half
    // the decoding work, colors within 1/255, a cut at most ~1/8 s late.
    a.extend(["-skip_frame", "noref"].map(String::from));

    a.extend(["-i".into(), video.to_string_lossy().into_owned()]);
    // `V`, not `v`: skips cover pictures stored as video streams.
    a.extend(["-map", "0:V:0", "-an", "-sn", "-dn", "-vf"].map(String::from));
    if probe.hdr {
        // Explicit BT.2020 matrix and full range: the PQ signal must reach Rust intact.
        a.push(format!("{filter},scale=in_color_matrix=bt2020:in_range=tv:out_range=full,format=rgb48le"));
        a.extend(["-f", "rawvideo", "-pix_fmt", "rgb48le", "pipe:1"].map(String::from));
    } else {
        a.push(filter);
        a.extend(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"].map(String::from));
    }
    a
}

fn run(video: &Path, opts: &Options, hw: HwAccel) -> Result<(Track, Stats)> {
    let start = Instant::now();
    let source_size = std::fs::metadata(video).with_context(|| format!("{}", video.display()))?.len();
    let probe = probe(video);
    if probe.hdr {
        tracing::info!("{} is HDR: tone-mapped to SDR", video.file_name().unwrap_or_default().to_string_lossy());
    }
    // Lowest priority: an analysis never slows down Jellyfin or the rest of the server.
    let mut child = Command::new("nice")
        .args(["-n", "19", "ffmpeg"])
        .args(ffmpeg_args(video, opts, hw, &probe))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting ffmpeg (is it installed?)")?;
    let mut out = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        stderr.read_to_string(&mut s).ok();
        s
    });

    let frame_bytes = SAMPLE_W * SAMPLE_H * 3;
    let mut buf = vec![0u8; frame_bytes];
    // HDR: 16 bits per channel from ffmpeg, tone-mapped into `buf`.
    let mut wide = vec![0u8; if probe.hdr { frame_bytes * 2 } else { 0 }];
    let mut bars = Bars::new((opts.fps * 2.0) as u32);
    let mut frames = Vec::new();
    let mut edges = Vec::new();
    loop {
        let read = if probe.hdr { out.read_exact(&mut wide) } else { out.read_exact(&mut buf) };
        match read {
            Ok(()) if probe.hdr => hdr_frame(&wide, &mut buf),
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let crop = bars.update(&buf);
        grid(&buf, crop, opts.cols as usize, opts.rows as usize, &mut frames);
        edge_cells(&buf, crop, opts.edge_cols as usize, opts.edge_rows as usize, &mut edges);
        if let Some(p) = &opts.progress {
            p.fetch_add(1, Ordering::Relaxed);
        }
        if opts.cancelled() {
            child.kill().ok();
            child.wait().ok();
            bail!("stopped");
        }
        // Speed limit: read slower than ffmpeg decodes; once the pipe is full, ffmpeg waits.
        // (ffmpeg's own -readrate is unreliable across versions above ~8×.)
        if let Some(s) = opts.max_speed.filter(|s| *s > 0.0) {
            let video = (frames.len() / (opts.cols as usize * opts.rows as usize * 3)) as f64 / opts.fps as f64;
            let ahead = video / s as f64 - start.elapsed().as_secs_f64();
            if ahead > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(ahead.min(1.0)));
            }
        }
    }
    let status = child.wait()?;
    let err = err_reader.join().unwrap_or_default();
    if !status.success() || frames.is_empty() {
        bail!("ffmpeg failed ({status}): {}", err.trim());
    }
    let track = Track {
        fps: opts.fps,
        cols: opts.cols,
        rows: opts.rows,
        source_size,
        source: video.to_string_lossy().into_owned(),
        frames,
        edge_cols: opts.edge_cols,
        edge_rows: opts.edge_rows,
        edges,
        exposure: 1.0,
        cuts: Vec::new(),
        flash_gains: Vec::new(),
    };
    Ok((track, Stats { elapsed: start.elapsed().as_secs_f64(), hwaccel: hw }))
}

/// Useful area of the picture (excluding black bars), in sample pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
}

/// One rgb48le HDR frame (PQ, BT.2020) into sRGB bytes.
fn hdr_frame(wide: &[u8], out: &mut [u8]) {
    for (px, o) in wide.as_chunks::<6>().0.iter().zip(out.as_chunks_mut::<3>().0) {
        let c = |i: usize| u16::from_le_bytes([px[i], px[i + 1]]);
        *o = hdr_to_srgb([c(0), c(2), c(4)]);
    }
}

/// Black bar tracking (2.39:1 letterbox, 4:3 pillarbox).
/// A bar shrinks as soon as picture shows up in it, but only grows after
/// staying black for `hold` consecutive frames, so a dark scene does not
/// make the framing jump.
pub struct Bars {
    crop: [usize; 4],
    cand: [usize; 4],
    count: [u32; 4],
    hold: u32,
}

const BLACK: u8 = 24;

impl Bars {
    pub fn new(hold: u32) -> Self {
        Self { crop: [0; 4], cand: [0; 4], count: [0; 4], hold: hold.max(1) }
    }

    // Rows, columns and the 4 sides index several arrays at once.
    #[allow(clippy::needless_range_loop)]
    pub fn update(&mut self, px: &[u8]) -> Crop {
        let (w, h) = (SAMPLE_W, SAMPLE_H);
        // Strongest channel, not luminance: pure blue is not a black bar.
        let level = |i: usize| px[i].max(px[i + 1]).max(px[i + 2]);
        let mut row_max = [0u8; SAMPLE_H];
        let mut col_max = [0u8; SAMPLE_W];
        for y in 0..h {
            for x in 0..w {
                let l = level((y * w + x) * 3);
                row_max[y] = row_max[y].max(l);
                col_max[x] = col_max[x].max(l);
            }
        }
        // (Almost) black frame: fade, end credits… leave the framing alone.
        if row_max.iter().all(|&l| l < BLACK * 2) {
            return self.current();
        }
        let lead = |v: &[u8], limit: usize| v.iter().take(limit).take_while(|&&l| l < BLACK).count();
        let (hl, wl) = (h * 3 / 10, w * 3 / 10);
        let mut rev_r = row_max;
        rev_r.reverse();
        let mut rev_c = col_max;
        rev_c.reverse();
        let measured = [lead(&row_max, hl), lead(&rev_r, hl), lead(&col_max, wl), lead(&rev_c, wl)];
        for s in 0..4 {
            let m = measured[s];
            if m < self.crop[s] {
                self.crop[s] = m;
                self.count[s] = 0;
            } else if m > self.crop[s] {
                self.cand[s] = if self.count[s] == 0 { m } else { self.cand[s].min(m) };
                self.count[s] += 1;
                if self.count[s] >= self.hold {
                    self.crop[s] = self.cand[s];
                    self.count[s] = 0;
                }
            } else {
                self.count[s] = 0;
            }
        }
        self.current()
    }

    pub fn current(&self) -> Crop {
        Crop { top: self.crop[0], bottom: self.crop[1], left: self.crop[2], right: self.crop[3] }
    }
}

/// The grid: `cols × rows` cells over the useful picture, row after row.
fn grid(px: &[u8], crop: Crop, cols: usize, rows: usize, out: &mut Vec<u8>) {
    let (x0, x1) = (crop.left, SAMPLE_W - crop.right);
    let (y0, y1) = (crop.top, SAMPLE_H - crop.bottom);
    for r in 0..rows {
        for c in 0..cols {
            out.extend(average(px, part(x0, x1, c, cols), part(y0, y1, r, rows)));
        }
    }
}

/// The Edges: thin cells along each side of the useful picture, `EDGE_DEPTH`
/// deep. Top then bottom (left to right), then left and right (top to bottom).
fn edge_cells(px: &[u8], crop: Crop, cols: usize, rows: usize, out: &mut Vec<u8>) {
    let (x0, x1) = (crop.left, SAMPLE_W - crop.right);
    let (y0, y1) = (crop.top, SAMPLE_H - crop.bottom);
    // As many pixels deep on every side: the band looks as thick on the sides as on top.
    let depth = ((y1 - y0) as f32 * EDGE_DEPTH).round().max(1.0) as usize;
    for ys in [y0..y0 + depth, y1 - depth..y1] {
        for c in 0..cols {
            out.extend(average(px, part(x0, x1, c, cols), ys.clone()));
        }
    }
    for xs in [x0..x0 + depth, x1 - depth..x1] {
        for r in 0..rows {
            out.extend(average(px, xs.clone(), part(y0, y1, r, rows)));
        }
    }
}

/// The `i`th of `n` equal parts of `start..end`, never empty.
fn part(start: usize, end: usize, i: usize, n: usize) -> std::ops::Range<usize> {
    let len = end - start;
    let from = start + i * len / n;
    let to = (start + (i + 1) * len / n).max(from + 1).min(end);
    from.min(end - 1)..to
}

/// The color of an area of the picture, in sRGB: an average in linear light
/// (averaging in sRGB darkens) where colorful pixels count more than grey
/// ones, so a red object on a grey wall stays red instead of a dull pink.
/// A grey area still gets its grey.
fn average(px: &[u8], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> [u8; 3] {
    let mut acc = [0f32; 3];
    let mut total = 0f32;
    for y in ys {
        for x in xs.clone() {
            let i = (y * SAMPLE_W + x) * 3;
            let (r, g, b) = (px[i], px[i + 1], px[i + 2]);
            let chroma = (r.max(g).max(b) - r.min(g).min(b)) as f32 / 255.0;
            let weight = GREY_WEIGHT + chroma;
            acc[0] += srgb_to_linear(r) * weight;
            acc[1] += srgb_to_linear(g) * weight;
            acc[2] += srgb_to_linear(b) * weight;
            total += weight;
        }
    }
    acc.map(|v| (linear_to_srgb(v / total.max(1e-6)) * 255.0).round() as u8)
}

/// How much a grey pixel counts, next to a fully colorful one (1 + this).
const GREY_WEIGHT: f32 = 0.25;

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(content_rows: std::ops::Range<usize>, v: u8) -> Vec<u8> {
        let mut f = vec![0u8; SAMPLE_W * SAMPLE_H * 3];
        for y in content_rows {
            f[y * SAMPLE_W * 3..(y + 1) * SAMPLE_W * 3].fill(v);
        }
        f
    }

    #[test]
    fn av1_uses_ffmpeg_own_decoder_on_vaapi_and_skips_covers() {
        let opts = crate::analyze_options();
        let av1 = Probe { codec: Some("av1".into()), hdr: false };
        let a = ffmpeg_args(Path::new("f.mkv"), &opts, HwAccel::Vaapi, &av1);
        let (c, i) = (a.iter().position(|x| x == "-c:v").unwrap(), a.iter().position(|x| x == "-i").unwrap());
        assert!(c < i && a[c + 1] == "av1");
        assert!(a.contains(&"0:V:0".to_string()));
        assert!(a.windows(2).any(|w| w == ["-skip_frame", "noref"]));

        let hevc = Probe { codec: Some("hevc".into()), hdr: false };
        let a = ffmpeg_args(Path::new("f.mkv"), &opts, HwAccel::Vaapi, &hevc);
        assert!(!a.contains(&"-c:v".to_string()));
    }

    #[test]
    fn hdr_keeps_ten_bits_and_reads_bt2020() {
        let p = parse_probe("codec_name=hevc\ncolor_transfer=smpte2084\n");
        assert_eq!(p, Probe { codec: Some("hevc".into()), hdr: true });
        assert!(!parse_probe("codec_name=h264\ncolor_transfer=bt709\n").hdr);
        assert!(!parse_probe("codec_name=h264\ncolor_transfer=unknown\n").hdr);

        let opts = crate::analyze_options();
        for hw in [HwAccel::Vaapi, HwAccel::None] {
            let a = ffmpeg_args(Path::new("f.mkv"), &opts, hw, &p);
            let vf = &a[a.iter().position(|x| x == "-vf").unwrap() + 1];
            assert!(vf.contains("in_color_matrix=bt2020") && vf.ends_with("format=rgb48le"), "{vf}");
            assert!(a.windows(2).any(|w| w == ["-pix_fmt", "rgb48le"]));
            if hw == HwAccel::Vaapi {
                assert!(vf.contains("format=p010,hwdownload,format=p010le"), "{vf}");
            }
        }
        let sdr = ffmpeg_args(Path::new("f.mkv"), &opts, HwAccel::Vaapi, &Probe::default());
        assert!(sdr.windows(2).any(|w| w == ["-pix_fmt", "rgb24"]));
    }

    #[test]
    fn letterbox_is_detected_after_hold_and_ignores_dark_scenes() {
        let mut b = Bars::new(3);
        let lb = frame(9..63, 200); // 9 black rows at the top and bottom
        for _ in 0..2 {
            assert_eq!(b.update(&lb).top, 0);
        }
        assert_eq!(b.update(&lb).top, 9);
        assert_eq!(b.current().bottom, 9);
        // Fully black scene: the framing does not move.
        assert_eq!(b.update(&frame(0..0, 0)).top, 9);
        // Full-frame picture: the bar shrinks immediately.
        assert_eq!(b.update(&frame(0..SAMPLE_H, 200)).top, 0);
    }

    /// A frame of one color, with `patch` painted over the area `xs` × `ys`.
    fn painted(color: [u8; 3], patch: [u8; 3], xs: std::ops::Range<usize>, ys: std::ops::Range<usize>) -> Vec<u8> {
        let mut f: Vec<u8> = color.repeat(SAMPLE_W * SAMPLE_H);
        for y in ys {
            for x in xs.clone() {
                f[(y * SAMPLE_W + x) * 3..][..3].copy_from_slice(&patch);
            }
        }
        f
    }

    #[test]
    fn a_red_patch_on_grey_keeps_its_red() {
        // A cell a third red, two thirds mid grey.
        let f = painted([128; 3], [220, 20, 20], 0..10, 0..10);
        let [r, g, b] = average(&f, 0..10, 0..30);
        assert!(r > 180 && g < 120 && b < 120, "red, not a dull pink: {r} {g} {b}");
        // A grey cell stays grey.
        assert_eq!(average(&f, 20..30, 20..30), [128; 3]);
    }

    #[test]
    fn edges_read_a_thin_band_along_each_side() {
        let crop = Crop { top: 0, bottom: 0, left: 0, right: 0 };
        // A green stripe 4 pixels wide along the top tenth, in the middle; blue everywhere else.
        let f = painted([0, 0, 255], [0, 255, 0], 62..66, 0..8);
        let mut out = Vec::new();
        edge_cells(&f, crop, 64, 36, &mut out);
        assert_eq!(out.len(), 2 * (64 + 36) * 3);
        let cell = |i: usize| [out[i * 3], out[i * 3 + 1], out[i * 3 + 2]];
        assert_eq!((cell(31), cell(32)), ([0, 255, 0], [0, 255, 0]));
        assert_eq!(cell(30), [0, 0, 255]);
        assert_eq!(cell(64 + 31), [0, 0, 255], "the bottom doesn't see it");
        // The grid's cell there is 8×8 pixels: the stripe is a sixth of it.
        let mut grid_out = Vec::new();
        grid(&f, crop, 16, 9, &mut grid_out);
        assert!(grid_out[7 * 3 + 2] > 200, "{:?}", &grid_out[21..24]);
    }
}
