//! sRGB <-> linear light conversions and rendering settings.

use std::sync::OnceLock;

pub type Rgb = [f32; 3];

fn lut() -> &'static [f32; 256] {
    static LUT: OnceLock<[f32; 256]> = OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0f32; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}

#[inline]
pub fn srgb_to_linear(v: u8) -> f32 {
    lut()[v as usize]
}

#[inline]
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// HDR reference white (BT.2408): what SDR shows at 100 %.
const HDR_WHITE_NITS: f32 = 203.0;
/// Below this (relative to reference white) HDR passes as is; above, highlights are rolled off.
const HDR_KNEE: f32 = 0.75;

/// PQ (SMPTE ST 2084) code, 16 bits, to linear light relative to HDR reference white.
fn pq_lut() -> &'static [f32] {
    static LUT: OnceLock<Vec<f32>> = OnceLock::new();
    LUT.get_or_init(|| {
        let (m1, m2) = (0.159_301_76_f64, 78.843_75_f64);
        let (c1, c2, c3) = (0.835_937_5_f64, 18.851_562_5_f64, 18.687_5_f64);
        (0..=u16::MAX)
            .map(|v| {
                let e = (v as f64 / 65535.0).powf(1.0 / m2);
                let nits = 10000.0 * ((e - c1).max(0.0) / (c2 - c3 * e)).powf(1.0 / m1);
                (nits / HDR_WHITE_NITS as f64) as f32
            })
            .collect()
    })
}

/// Highlight roll-off: unchanged up to the knee, then bends smoothly (same
/// slope at the knee) towards 1, never reaching it.
fn roll_off(x: f32) -> f32 {
    if x <= HDR_KNEE {
        return x;
    }
    let (t, room) = (x - HDR_KNEE, 1.0 - HDR_KNEE);
    HDR_KNEE + room * t / (t + room)
}

/// One HDR10 pixel (PQ-encoded BT.2020 RGB, 16 bits per channel, full range)
/// to an SDR sRGB pixel: linear light, BT.2020 to BT.709 primaries, highlights
/// rolled off on the strongest channel so the hue holds, sRGB encoding.
pub fn hdr_to_srgb(px: [u16; 3]) -> [u8; 3] {
    let lut = pq_lut();
    let [r, g, b] = px.map(|v| lut[v as usize]);
    // BT.2020 -> BT.709 (linear light); colors outside BT.709 clip at 0.
    let mut c = [
        (1.660_491 * r - 0.587_641 * g - 0.072_850 * b).max(0.0),
        (-0.124_550 * r + 1.132_9 * g - 0.008_349 * b).max(0.0),
        (-0.018_151 * r - 0.100_579 * g + 1.118_73 * b).max(0.0),
    ];
    let max = c[0].max(c[1]).max(c[2]);
    if max > HDR_KNEE {
        let k = roll_off(max) / max;
        c.iter_mut().for_each(|v| *v *= k);
    }
    c.map(|v| (linear_to_srgb(v) * 255.0).round() as u8)
}

#[inline]
pub fn luminance(c: Rgb) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Settings applied to colors before they are sent to the bridge.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct Look {
    /// Brightness multiplier (1.0 = unchanged).
    pub brightness: f32,
    /// Saturation multiplier (1.0 = unchanged).
    pub saturation: f32,
    /// Brightness floor in linear light (0 = black allowed).
    pub min_brightness: f32,
    /// Smoothing time constant in milliseconds (0 = none).
    pub smoothing_ms: f32,
    /// How long a cut between two shots fades, centered on the cut, in
    /// milliseconds: smoothing stays low so the lights follow the picture,
    /// cuts still fade softly. Below `smoothing_ms` (or 0), a cut fades like
    /// the rest. Only with centered smoothing.
    pub cut_fade_ms: f32,
    /// Offset added to the Jellyfin position, in milliseconds
    /// (positive = the lights run further ahead in the film).
    pub offset_ms: f64,
    /// Width of the screen zone each light reads (1.0 = default, larger = calmer, more uniform).
    pub spread: f32,
    /// Stretch the lights' positions to the area's extent, so lights grouped
    /// around the TV still read the screen's edges.
    pub fit_area: bool,
    /// Smooth around each moment (the Track is known ahead, so no delay)
    /// rather than after it like a live signal (which lags by `smoothing_ms`).
    pub centered_smoothing: bool,
    /// Brighten dark films to a usual level (gain measured on each Track).
    pub auto_exposure: bool,
    /// How much grey, washed-out colors lean to a warm white (0 = none, 1 = fully).
    pub warm_greys: f32,
    /// How much Flashes (a few frames much brighter than around them) are
    /// toned down: 0 = as they are, 1 = back to the level around them.
    pub flash_guard: f32,
    /// Colors darker than this (luminance, linear light, after exposure) fade
    /// out instead of showing dim and muddy (0 = off).
    pub black_threshold: f32,
    /// How fast they fade under the threshold: 1 is linear, higher is darker sooner.
    pub black_falloff: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            saturation: 1.15,
            min_brightness: 0.0,
            // Centered, so a softer default costs no delay.
            smoothing_ms: 250.0,
            cut_fade_ms: 400.0,
            offset_ms: 0.0,
            spread: 1.0,
            fit_area: true,
            centered_smoothing: true,
            auto_exposure: true,
            warm_greys: 0.5,
            flash_guard: 0.5,
            black_threshold: 0.0,
            black_falloff: 2.0,
        }
    }
}

/// A 2700 K warm white, in linear light.
const WARM: Rgb = [1.0, 0.456, 0.147];

impl Look {
    /// This Look on a film whose Track measured `exposure`.
    pub fn with_exposure(&self, exposure: f32) -> Look {
        let gain = if self.auto_exposure { exposure } else { 1.0 };
        Look { brightness: self.brightness * gain, ..self.clone() }
    }

    pub fn apply(&self, mut c: Rgb) -> Rgb {
        let l = luminance(c);
        // Greys (low saturation) lean to a warm white of the same brightness.
        let max = c[0].max(c[1]).max(c[2]);
        if self.warm_greys > 0.0 && max > 1e-4 {
            let grey = (1.0 - (max - c[0].min(c[1]).min(c[2])) / max / 0.3).clamp(0.0, 1.0);
            let w = self.warm_greys.clamp(0.0, 1.0) * grey;
            let k = l / luminance(WARM);
            for (v, t) in c.iter_mut().zip(WARM) {
                *v += (t * k - *v) * w;
            }
        }
        for v in &mut c {
            *v = (l + (*v - l) * self.saturation).max(0.0) * self.brightness;
        }
        // Under the Black threshold, fade out along a curve: 1 at the threshold, 0 at black.
        let dim = luminance(c);
        if self.black_threshold > 0.0 && dim < self.black_threshold {
            let k = (dim / self.black_threshold).powf(self.black_falloff.max(0.1));
            for v in &mut c {
                *v *= k;
            }
        }
        let max = c[0].max(c[1]).max(c[2]);
        if max > 1.0 {
            // Keep the hue instead of clipping each channel separately.
            for v in &mut c {
                *v /= max;
            }
        } else if self.min_brightness > 0.0 && max < self.min_brightness {
            if max > 1e-4 {
                let k = self.min_brightness / max;
                for v in &mut c {
                    *v *= k;
                }
            } else {
                c = [self.min_brightness; 3];
            }
        }
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_srgb() {
        for v in [0u8, 1, 10, 128, 200, 255] {
            let back = (linear_to_srgb(srgb_to_linear(v)) * 255.0).round() as u8;
            assert_eq!(back, v);
        }
    }

    /// PQ code (16 bits) of a light level in nits.
    fn pq(nits: f64) -> u16 {
        let (m1, m2) = (0.159_301_76_f64, 78.843_75_f64);
        let (c1, c2, c3) = (0.835_937_5_f64, 18.851_562_5_f64, 18.687_5_f64);
        let y = (nits / 10000.0).powf(m1);
        (((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2) * 65535.0).round() as u16
    }

    #[test]
    fn hdr_reference_white_is_sdr_white_and_highlights_roll_off() {
        let w = pq(203.0);
        let white = hdr_to_srgb([w, w, w]);
        assert!(white.iter().all(|&v| v >= 240), "{white:?}");
        let g = pq(20.0);
        let grey = hdr_to_srgb([g, g, g]);
        assert!(grey[0] > 60 && grey[0] < 110 && grey[0] == grey[2], "{grey:?}");
        // 1000 nits is brighter than reference white, still not clipped flat.
        let p = pq(1000.0);
        assert!(hdr_to_srgb([p, p, p])[0] > white[0] || white[0] == 255);
        assert_eq!(hdr_to_srgb([0, 0, 0]), [0, 0, 0]);
    }

    #[test]
    fn hdr_fire_stays_orange_not_white() {
        // A bright orange flame, graded at a few hundred nits.
        let fire = hdr_to_srgb([pq(400.0), pq(120.0), pq(15.0)]);
        assert!(fire[0] > 200, "{fire:?}");
        let [r, g, b] = fire.map(u32::from);
        assert!(b < g / 2 && g < r * 3 / 4, "{fire:?}");
        // Read as if it were SDR (the old way), the same signal turns pale.
        let naive = [pq(400.0), pq(120.0), pq(15.0)].map(|v| (v >> 8) as u8);
        assert!(naive[2] > 80 && naive[0] < 200, "{naive:?}");
    }

    #[test]
    fn greys_lean_warm_colors_stay() {
        let look = Look { saturation: 1.0, warm_greys: 1.0, ..Default::default() };
        let g = look.apply([0.3, 0.3, 0.3]);
        assert!(g[0] > g[1] && g[1] > g[2], "{g:?}");
        assert!((luminance(g) - 0.3).abs() < 0.02, "{g:?}");
        let red = look.apply([0.8, 0.1, 0.1]);
        assert!(red.iter().zip([0.8, 0.1, 0.1]).all(|(a, b)| (a - b).abs() < 1e-5), "{red:?}");
        let off = Look { warm_greys: 0.0, ..look };
        assert_eq!(off.apply([0.3, 0.3, 0.3]), [0.3, 0.3, 0.3]);
    }

    #[test]
    fn look_keeps_hue_when_over_range() {
        let look = Look { brightness: 2.0, saturation: 1.0, warm_greys: 0.0, ..Default::default() };
        let c = look.apply([0.8, 0.4, 0.0]);
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn under_the_black_threshold_colors_fade_out_with_no_step() {
        let look =
            Look { saturation: 1.0, warm_greys: 0.0, black_threshold: 0.02, black_falloff: 2.0, ..Default::default() };
        // Above: untouched.
        assert_eq!(look.apply([0.1, 0.05, 0.02]), [0.1, 0.05, 0.02]);
        // Across the threshold, no step; under it, darker and darker, down to black.
        let lum = |v: f32| luminance(look.apply([v; 3]));
        assert!((lum(0.0201) - lum(0.0199)).abs() < 1e-3);
        assert!((lum(0.01) - 0.01 * 0.25).abs() < 1e-5, "{}", lum(0.01));
        assert!(lum(0.005) < lum(0.01) && lum(0.0) == 0.0);
        // Off: dim colors pass.
        let off = Look { black_threshold: 0.0, ..look };
        assert_eq!(off.apply([0.01; 3]), [0.01; 3]);
    }
}
