import type { Corner, WledConfig } from "../api";

/** Corners clockwise from the couch: side i runs from corner i to corner i + 1. */
const CORNERS: Corner[] = ["bottom-left", "top-left", "top-right", "bottom-right"];
const POINTS: [number, number][] = [
  [-1, -1],
  [-1, 1],
  [1, 1],
  [1, -1],
];

export type Led = { id: number; x: number; z: number };

/** Each LED of the Strip in wiring order, on the edge it sits behind (x and z
 *  from -1 to 1, z up). The same placement as `wled::channels` on the server. */
export function stripLeds(layout: Pick<WledConfig, "start" | "clockwise" | "left" | "top" | "right" | "bottom">): Led[] {
  const perSide = [layout.left, layout.top, layout.right, layout.bottom];
  const start = CORNERS.indexOf(layout.start);
  const leds: Led[] = [];
  for (let k = 0; k < 4; k++) {
    let side: number;
    let from: number;
    let to: number;
    if (layout.clockwise) {
      side = (start + k) % 4;
      from = side;
      to = (side + 1) % 4;
    } else {
      side = (start + 3 - k) % 4;
      from = (side + 1) % 4;
      to = side;
    }
    const [x0, z0] = POINTS[from];
    const [x1, z1] = POINTS[to];
    const count = Math.max(0, Math.floor(perSide[side] || 0));
    for (let j = 0; j < count; j++) {
      const f = (j + 0.5) / count;
      leds.push({ id: leds.length, x: x0 + (x1 - x0) * f, z: z0 + (z1 - z0) * f });
    }
  }
  return leds;
}
