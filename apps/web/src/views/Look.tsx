import { useEffect, useRef, useState } from "react";
import { RotateCcw } from "lucide-react";
import type { Look } from "../api";
import { useApp } from "../App";
import { Alert, Button, Card, CardHeader, PageHeader, rgb } from "../ui";

const DEFAULT: Look = { brightness: 1, saturation: 1.15, min_brightness: 0, smoothing_ms: 60, offset_ms: 0 };

type Spec = { key: keyof Look; label: string; hint: string; min: number; max: number; step: number; fmt: (v: number) => string };

const specs: Spec[] = [
  { key: "brightness", label: "Brightness", hint: "Multiplies every color.", min: 0.1, max: 2, step: 0.05, fmt: (v) => `${Math.round(v * 100)}%` },
  { key: "saturation", label: "Saturation", hint: "Above 100% makes colors pop, below washes them out.", min: 0, max: 2, step: 0.05, fmt: (v) => `${Math.round(v * 100)}%` },
  { key: "min_brightness", label: "Minimum brightness", hint: "Keeps a soft light during dark scenes instead of going black.", min: 0, max: 0.3, step: 0.01, fmt: (v) => (v ? `${Math.round(v * 100)}%` : "Off") },
  { key: "smoothing_ms", label: "Smoothing", hint: "Higher is calmer, lower follows cuts more sharply.", min: 0, max: 500, step: 10, fmt: (v) => (v ? `${v} ms` : "Off") },
  { key: "offset_ms", label: "Sync offset", hint: "Lights change late on cuts? Increase it. Early? Decrease it.", min: -2000, max: 2000, step: 50, fmt: (v) => `${v > 0 ? "+" : ""}${v} ms` },
];

// Same maths as the player (apps/server/src/color.rs), for the preview.
const toLin = (v: number) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const toSrgb = (c: number) => {
  c = Math.min(1, Math.max(0, c));
  return Math.round((c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055) * 255);
};
function apply(look: Look, srgb: [number, number, number]): [number, number, number] {
  let c = srgb.map(toLin);
  const l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
  c = c.map((v) => Math.max(0, l + (v - l) * look.saturation) * look.brightness);
  const max = Math.max(...c);
  if (max > 1) c = c.map((v) => v / max);
  else if (look.min_brightness > 0 && max < look.min_brightness)
    c = max > 1e-4 ? c.map((v) => (v * look.min_brightness) / max) : [look.min_brightness, look.min_brightness, look.min_brightness];
  return c.map(toSrgb) as [number, number, number];
}

const palette: { name: string; c: [number, number, number] }[] = [
  { name: "Night", c: [22, 34, 66] },
  { name: "Teal", c: [38, 104, 112] },
  { name: "Skin", c: [214, 150, 112] },
  { name: "Sunset", c: [232, 96, 48] },
  { name: "Forest", c: [52, 92, 44] },
  { name: "Neon", c: [190, 40, 150] },
  { name: "Dark room", c: [14, 10, 8] },
];

export function LookView() {
  const { cfg, patch, status } = useApp();
  const [look, setLook] = useState<Look>(cfg.look);
  const [error, setError] = useState<string>();
  const timer = useRef<number>(undefined);

  useEffect(() => () => clearTimeout(timer.current), []);

  const set = (next: Look) => {
    setLook(next);
    clearTimeout(timer.current);
    timer.current = window.setTimeout(
      () => patch({ look: next }).then(() => setError(undefined), (e: Error) => setError(e.message)),
      150,
    );
  };

  const changed = (Object.keys(DEFAULT) as (keyof Look)[]).some((k) => Math.abs(look[k] - DEFAULT[k]) > 1e-4); // f32 on the server side

  return (
    <div>
      <PageHeader
        title="Look"
        subtitle={status?.player.streaming ? "Changes apply live to the lights." : "Changes apply instantly, even during a film."}
        action={
          <Button variant="ghost" disabled={!changed} onClick={() => set(DEFAULT)} icon={<RotateCcw className="size-4" />}>
            Reset
          </Button>
        }
      />
      {error && <div className="mb-4"><Alert>{error}</Alert></div>}

      <div className="grid gap-5 lg:grid-cols-5">
        <Card className="lg:col-span-3">
          <div className="divide-y divide-white/[0.05]">
            {specs.map((s) => {
              const v = look[s.key];
              const pct = ((v - s.min) / (s.max - s.min)) * 100;
              return (
                <div key={s.key} className="px-5 py-5">
                  <div className="flex items-baseline justify-between gap-4">
                    <label htmlFor={s.key} className="text-sm font-medium">{s.label}</label>
                    <span className="text-sm text-zinc-300 tabular-nums">{s.fmt(v)}</span>
                  </div>
                  <input
                    id={s.key}
                    type="range"
                    min={s.min}
                    max={s.max}
                    step={s.step}
                    value={v}
                    onChange={(e) => set({ ...look, [s.key]: Number(e.target.value) })}
                    className="mt-3 w-full"
                    style={{ "--p": `${pct}%` } as React.CSSProperties}
                  />
                  <p className="mt-1 text-xs text-zinc-500">{s.hint}</p>
                </div>
              );
            })}
          </div>
        </Card>

        <Card className="self-start lg:col-span-2">
          <CardHeader title="Preview" subtitle="Screen colors, and what the lights will show." />
          <div className="space-y-2 px-5 pt-4 pb-5">
            {palette.map((p) => {
              const out = apply(look, p.c);
              return (
                <div key={p.name} className="flex items-center gap-3">
                  <span className="w-20 shrink-0 text-xs text-zinc-500">{p.name}</span>
                  <span className="h-8 flex-1 rounded-l-lg" style={{ background: rgb(p.c) }} />
                  <span
                    className="h-8 flex-1 rounded-r-lg transition-colors"
                    style={{ background: rgb(out), boxShadow: `0 0 24px -4px ${rgb(out)}` }}
                  />
                </div>
              );
            })}
            <div className="flex pt-1 pl-23 text-[11px] text-zinc-600">
              <span className="flex-1">Screen</span>
              <span className="flex-1 pl-3">Lights</span>
            </div>
          </div>
        </Card>
      </div>
    </div>
  );
}
