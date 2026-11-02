import { type Look } from "../api";

// Same maths as the player (apps/server/src/color.rs), for the preview.
const toLin = (v: number) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);

const toSrgb = (c: number) => {
  c = Math.min(1, Math.max(0, c));
  return Math.round((c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055) * 255);
};

const WARM = [1, 0.456, 0.147]; // 2700 K, linear

const lum = (c: number[]) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];

export function apply(look: Look, srgb: [number, number, number]): [number, number, number] {
  let c = srgb.map(toLin);
  const l = lum(c);
  const top = Math.max(...c);
  if (look.warm_greys > 0 && top > 1e-4) {
    const grey = Math.min(1, Math.max(0, 1 - (top - Math.min(...c)) / top / 0.3));
    const w = Math.min(1, look.warm_greys) * grey;
    c = c.map((v, i) => v + (WARM[i] * (l / lum(WARM)) - v) * w);
  }
  c = c.map((v) => Math.max(0, l + (v - l) * look.saturation) * look.brightness);
  const max = Math.max(...c);
  if (max > 1) c = c.map((v) => v / max);
  else if (look.min_brightness > 0 && max < look.min_brightness)
    c = max > 1e-4 ? c.map((v) => (v * look.min_brightness) / max) : [look.min_brightness, look.min_brightness, look.min_brightness];
  return c.map(toSrgb) as [number, number, number];
}
