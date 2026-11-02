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

#[inline]
pub fn luminance(c: Rgb) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// Settings applied to colors before they are sent to the bridge.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
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
}

impl Default for Look {
    fn default() -> Self {
        Self {
            brightness: 1.0,
            saturation: 1.15,
            min_brightness: 0.0,
            // Centered, so a softer default costs no delay.
            smoothing_ms: 250.0,
            offset_ms: 0.0,
            spread: 1.0,
            fit_area: true,
            centered_smoothing: true,
            auto_exposure: true,
            warm_greys: 0.5,
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
}
