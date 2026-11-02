//! Analyse d'un fichier vidéo : ffmpeg décode (en matériel si possible) et
//! réduit chaque image à 96×54 directement sur le GPU ; Rust ne voit passer
//! que ~15 Ko par image, détecte les bandes noires et calcule la grille.

use crate::color::{linear_to_srgb, srgb_to_linear};
use crate::track::Track;
use anyhow::{bail, Context, Result};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

pub const SAMPLE_W: usize = 96;
pub const SAMPLE_H: usize = 54;
const RENDER_NODE: &str = "/dev/dri/renderD128";

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum HwAccel {
    /// VA-API si un GPU est présent, sinon logiciel.
    Auto,
    /// VA-API (Intel/AMD), recommandé sur le N100.
    Vaapi,
    /// Intel Quick Sync.
    Qsv,
    /// VideoToolbox (macOS).
    Videotoolbox,
    /// Décodage logiciel.
    None,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub fps: f32,
    pub cols: u8,
    pub rows: u8,
    pub hwaccel: HwAccel,
    pub threads: u32,
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
    match run(video, opts, hw) {
        Ok(r) => Ok(r),
        // Codec non géré par le GPU (ou pilote absent) : on retombe en logiciel.
        Err(e) if hw != HwAccel::None && opts.hwaccel == HwAccel::Auto => {
            tracing::warn!("décodage matériel impossible ({e:#}), passage en logiciel");
            run(video, opts, HwAccel::None)
        }
        Err(e) => Err(e),
    }
}

fn ffmpeg_args(video: &Path, opts: &Options, hw: HwAccel) -> Vec<String> {
    let fps = opts.fps;
    let mut a: Vec<String> = vec!["-hide_banner".into(), "-loglevel".into(), "error".into(), "-nostdin".into()];
    let filter = match hw {
        HwAccel::Vaapi => {
            a.extend(["-hwaccel", "vaapi", "-hwaccel_device", RENDER_NODE, "-hwaccel_output_format", "vaapi"].map(String::from));
            format!("fps={fps},scale_vaapi=w={SAMPLE_W}:h={SAMPLE_H}:format=nv12,hwdownload,format=nv12")
        }
        HwAccel::Qsv => {
            a.extend(["-hwaccel", "qsv", "-hwaccel_output_format", "qsv"].map(String::from));
            format!("fps={fps},vpp_qsv=w={SAMPLE_W}:h={SAMPLE_H}:format=nv12,hwdownload,format=nv12")
        }
        HwAccel::Videotoolbox => {
            // Décodage sur le GPU du Mac, images rapatriées puis réduites (coût négligeable).
            a.extend(["-hwaccel", "videotoolbox"].map(String::from));
            format!("fps={fps},scale={SAMPLE_W}:{SAMPLE_H}:flags=area")
        }
        _ => {
            // Image finale minuscule : le filtre de boucle ne sert à rien et coûte cher.
            a.extend(["-skip_loop_filter", "all"].map(String::from));
            if opts.threads > 0 {
                a.extend(["-threads".into(), opts.threads.to_string()]);
            }
            format!("fps={fps},scale={SAMPLE_W}:{SAMPLE_H}:flags=area")
        }
    };
    a.extend(["-i".into(), video.to_string_lossy().into_owned()]);
    a.extend(["-map", "0:v:0", "-an", "-sn", "-dn", "-vf"].map(String::from));
    a.push(filter);
    a.extend(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"].map(String::from));
    a
}

fn run(video: &Path, opts: &Options, hw: HwAccel) -> Result<(Track, Stats)> {
    let start = Instant::now();
    let source_size = std::fs::metadata(video).with_context(|| format!("{}", video.display()))?.len();
    let mut child = Command::new("ffmpeg")
        .args(ffmpeg_args(video, opts, hw))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("lancement de ffmpeg (est-il installé ?)")?;
    let mut out = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let err_reader = std::thread::spawn(move || {
        let mut s = String::new();
        stderr.read_to_string(&mut s).ok();
        s
    });

    let frame_bytes = SAMPLE_W * SAMPLE_H * 3;
    let mut buf = vec![0u8; frame_bytes];
    let mut grid = Grid::new(opts.cols as usize, opts.rows as usize);
    let mut bars = Bars::new((opts.fps * 2.0) as u32);
    let mut frames = Vec::new();
    loop {
        match out.read_exact(&mut buf) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        }
        let crop = bars.update(&buf);
        grid.compute(&buf, crop, &mut frames);
    }
    let status = child.wait()?;
    let err = err_reader.join().unwrap_or_default();
    if !status.success() || frames.is_empty() {
        bail!("ffmpeg a échoué ({status}) : {}", err.trim());
    }
    let track = Track {
        fps: opts.fps,
        cols: opts.cols,
        rows: opts.rows,
        source_size,
        source: video.to_string_lossy().into_owned(),
        frames,
    };
    Ok((track, Stats { elapsed: start.elapsed().as_secs_f64(), hwaccel: hw }))
}

/// Zone utile de l'image (hors bandes noires), en pixels de l'échantillon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
}

/// Suivi des bandes noires (letterbox 2.39:1, pillarbox 4:3).
/// Une bande se réduit tout de suite quand de l'image y apparaît, mais ne
/// s'élargit qu'après avoir été noire en continu pendant `hold` images :
/// une scène sombre ne fait donc pas « sauter » le cadrage.
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

    pub fn update(&mut self, px: &[u8]) -> Crop {
        let (w, h) = (SAMPLE_W, SAMPLE_H);
        // Canal le plus fort, pas la luminance : un bleu pur n'est pas une bande noire.
        let luma = |i: usize| px[i].max(px[i + 1]).max(px[i + 2]);
        let mut row_max = [0u8; SAMPLE_H];
        let mut col_max = [0u8; SAMPLE_W];
        for y in 0..h {
            for x in 0..w {
                let l = luma((y * w + x) * 3);
                row_max[y] = row_max[y].max(l);
                col_max[x] = col_max[x].max(l);
            }
        }
        // Image (quasi) noire : fondu, générique… on ne touche pas au cadrage.
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

/// Moyenne par case, en lumière linéaire (une moyenne en sRGB assombrit).
struct Grid {
    cols: usize,
    rows: usize,
    acc: Vec<[f32; 4]>,
}

impl Grid {
    fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows, acc: vec![[0.0; 4]; cols * rows] }
    }

    fn compute(&mut self, px: &[u8], crop: Crop, out: &mut Vec<u8>) {
        let (x0, x1) = (crop.left, SAMPLE_W - crop.right);
        let (y0, y1) = (crop.top, SAMPLE_H - crop.bottom);
        let (aw, ah) = (x1 - x0, y1 - y0);
        self.acc.iter_mut().for_each(|a| *a = [0.0; 4]);
        for y in y0..y1 {
            let r = ((y - y0) * self.rows / ah).min(self.rows - 1);
            for x in x0..x1 {
                let c = ((x - x0) * self.cols / aw).min(self.cols - 1);
                let i = (y * SAMPLE_W + x) * 3;
                let a = &mut self.acc[r * self.cols + c];
                a[0] += srgb_to_linear(px[i]);
                a[1] += srgb_to_linear(px[i + 1]);
                a[2] += srgb_to_linear(px[i + 2]);
                a[3] += 1.0;
            }
        }
        for a in &self.acc {
            let n = a[3].max(1.0);
            for ch in 0..3 {
                out.push((linear_to_srgb(a[ch] / n) * 255.0).round() as u8);
            }
        }
    }
}

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
    fn letterbox_is_detected_after_hold_and_ignores_dark_scenes() {
        let mut b = Bars::new(3);
        let lb = frame(7..47, 200); // 7 lignes noires en haut et en bas
        for _ in 0..2 {
            assert_eq!(b.update(&lb).top, 0);
        }
        assert_eq!(b.update(&lb).top, 7);
        assert_eq!(b.current().bottom, 7);
        // Scène toute noire : le cadrage ne bouge pas.
        assert_eq!(b.update(&frame(0..0, 0)).top, 7);
        // Image plein cadre : la bande se réduit immédiatement.
        assert_eq!(b.update(&frame(0..54, 200)).top, 0);
    }
}
