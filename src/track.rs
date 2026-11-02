//! Piste de couleurs : pour chaque instant du film, une petite grille de
//! couleurs (par défaut 16×9) calculée une fois pour toutes à l'analyse.
//!
//! Format du fichier `.lueur` :
//! `LUEUR` | version u8 | fps f32 | cols u8 | rows u8 | frames u32 |
//! taille source u64 | len u16 | chemin source (UTF-8) | zstd(grilles sRGB u8)

use crate::color::{srgb_to_linear, Rgb};
use anyhow::{bail, Context, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8; 5] = b"LUEUR";
const VERSION: u8 = 1;

#[derive(Debug, Clone)]
pub struct Track {
    pub fps: f32,
    pub cols: u8,
    pub rows: u8,
    /// Taille du fichier vidéo au moment de l'analyse (détection des remplacements).
    pub source_size: u64,
    pub source: String,
    /// Grilles concaténées, `cols * rows * 3` octets sRGB par image.
    pub frames: Vec<u8>,
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
        // Écriture atomique : jamais de piste à moitié écrite lue par le lecteur.
        let tmp = path.with_extension("lueur.tmp");
        std::fs::File::create(&tmp)?.write_all(&buf)?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Track> {
        let mut data = Vec::new();
        std::fs::File::open(path)
            .with_context(|| format!("ouverture de {}", path.display()))?
            .read_to_end(&mut data)?;
        if data.len() < 25 || &data[..5] != MAGIC {
            bail!("{} n'est pas une piste lueur", path.display());
        }
        if data[5] != VERSION {
            bail!("version de piste {} non gérée", data[5]);
        }
        let fps = f32::from_le_bytes(data[6..10].try_into()?);
        let (cols, rows) = (data[10], data[11]);
        let count = u32::from_le_bytes(data[12..16].try_into()?) as usize;
        let source_size = u64::from_le_bytes(data[16..24].try_into()?);
        let src_len = u16::from_le_bytes(data[24..26].try_into()?) as usize;
        let source = String::from_utf8(data[26..26 + src_len].to_vec())?;
        let frames = zstd::decode_all(&data[26 + src_len..])?;
        let t = Track { fps, cols, rows, source_size, source, frames };
        if t.frame_count() != count {
            bail!("piste tronquée ({} images au lieu de {count})", t.frame_count());
        }
        Ok(t)
    }

    /// Grille en lumière linéaire à l'instant `t` (secondes), interpolée
    /// entre les deux images analysées les plus proches.
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

    pub fn frame(&self, i: usize) -> &[u8] {
        let l = self.frame_len();
        &self.frames[i * l..(i + 1) * l]
    }
}

/// Emplacement de la piste d'un fichier vidéo dans le dossier de pistes.
pub fn track_path(store: &Path, video: &Path) -> PathBuf {
    // Chemin absolu : l'analyse en ligne de commande et le lecteur doivent tomber sur la même piste.
    let canon = std::fs::canonicalize(video);
    let video = canon.as_deref().unwrap_or(video);
    // FNV-1a 64 bits : stable entre versions, contrairement au hasher de std.
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

/// Une piste existe et correspond toujours au fichier (même taille).
pub fn is_fresh(store: &Path, video: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(video) else { return false };
    let p = track_path(store, video);
    let Ok(mut f) = std::fs::File::open(&p) else { return false };
    let mut head = [0u8; 24];
    if f.read_exact(&mut head).is_err() || &head[..5] != MAGIC {
        return false;
    }
    u64::from_le_bytes(head[16..24].try_into().unwrap()) == meta.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_sample() {
        let dir = std::env::temp_dir().join(format!("lueur-test-{}", std::process::id()));
        let mut frames = vec![0u8; 2 * 2 * 3 * 2];
        frames[..12].fill(0); // image 0 noire
        frames[12..].fill(255); // image 1 blanche
        let t = Track { fps: 10.0, cols: 2, rows: 2, source_size: 42, source: "/x/film.mkv".into(), frames };
        let p = dir.join("t.lueur");
        t.save(&p).unwrap();
        let l = Track::load(&p).unwrap();
        assert_eq!(l.frame_count(), 2);
        assert_eq!(l.source, "/x/film.mkv");
        let mut g = Vec::new();
        l.sample(0.05, &mut g); // à mi-chemin entre les deux images
        assert!((g[0][0] - 0.5).abs() < 1e-3);
        l.sample(99.0, &mut g); // au-delà de la fin : dernière image
        assert!((g[3][2] - 1.0).abs() < 1e-6);
        std::fs::remove_dir_all(dir).ok();
    }
}
