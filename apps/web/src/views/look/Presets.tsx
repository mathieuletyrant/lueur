import type { Look } from "../../api";
import { Card, CardHeader, cx } from "../../ui";

type Preset = Pick<Look, "smoothing_ms" | "spread" | "saturation" | "warm_greys">;

/** How lively the lights are, like Hue Sync's intensities. Each one sets a few
 *  sliders at once; brightness, the room's business, is never touched. */
export const PRESETS: { name: string; hint: string; values: Preset }[] = [
  { name: "Subtle", hint: "Slow fades, soft colors. Calm, for long evenings.", values: { smoothing_ms: 800, spread: 1.6, saturation: 1.0, warm_greys: 0.6 } },
  { name: "Moderate", hint: "Follows the film without drawing attention.", values: { smoothing_ms: 400, spread: 1.2, saturation: 1.15, warm_greys: 0.5 } },
  { name: "High", hint: "Quick and colorful, each side of the screen its own.", values: { smoothing_ms: 200, spread: 1.0, saturation: 1.3, warm_greys: 0.4 } },
  { name: "Intense", hint: "Every cut, every flash. For action and animation.", values: { smoothing_ms: 60, spread: 0.8, saturation: 1.5, warm_greys: 0.2 } },
];

// The server stores floats as f32: compare loosely.
const same = (look: Look, p: Preset) =>
  (Object.keys(p) as (keyof Preset)[]).every((k) => Math.abs(look[k] - p[k]) < 1e-3 * Math.max(1, Math.abs(p[k])));

/** Pick a preset; touching a slider afterwards makes it Custom. */
export function Presets({ look, onPick }: { look: Look; onPick: (values: Preset) => void }) {
  const current = PRESETS.find((p) => same(look, p.values));
  return (
    <Card className="order-first self-start lg:order-none lg:col-span-2">
      <CardHeader title="Intensity" subtitle={current ? "Fine-tune with the sliders." : "Custom: your own slider values."} />
      <div className="space-y-2 px-5 pt-4 pb-5">
        {PRESETS.map((p) => {
          const on = p === current;
          return (
            <button
              key={p.name}
              type="button"
              aria-pressed={on}
              onClick={() => onPick(p.values)}
              className={cx(
                "w-full rounded-xl border px-4 py-3 text-left transition",
                on
                  ? "border-glow-1/50 bg-glow-1/[0.07] ring-4 ring-glow-1/[0.07]"
                  : "border-white/[0.08] bg-white/[0.02] hover:border-white/15 hover:bg-white/[0.04]",
              )}
            >
              <span className="flex items-baseline justify-between gap-3">
                <span className="font-display text-xl">{p.name}</span>
                <span className="text-[11px] text-zinc-500 tabular-nums">
                  {p.values.smoothing_ms} ms · {Math.round(p.values.saturation * 100)}%
                </span>
              </span>
              <span className="mt-0.5 block text-xs text-zinc-400">{p.hint}</span>
            </button>
          );
        })}
      </div>
    </Card>
  );
}
