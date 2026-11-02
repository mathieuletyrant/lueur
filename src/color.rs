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
}

impl Default for Look {
    fn default() -> Self {
        Self { brightness: 1.0, saturation: 1.15, min_brightness: 0.0, smoothing_ms: 60.0, offset_ms: 0.0 }
    }
}

impl Look {
    pub fn apply(&self, mut c: Rgb) -> Rgb {
        let l = luminance(c);
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
    fn look_keeps_hue_when_over_range() {
        let look = Look { brightness: 2.0, saturation: 1.0, ..Default::default() };
        let c = look.apply([0.8, 0.4, 0.0]);
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 0.5).abs() < 1e-6);
    }
}
