import { RotateCcw } from "lucide-react";
import { useState } from "react";
import { type Look, useDebounced } from "../../api";
import { useApp } from "../../context";
import { Alert, Button, Card, PageHeader, Slider, SwitchRow } from "../../ui";
import { FrameTest } from "./FrameTest";
import { LookPreview } from "./LookPreview";

const DEFAULT: Look = {
  brightness: 1,
  saturation: 1.15,
  min_brightness: 0,
  smoothing_ms: 250,
  offset_ms: 0,
  spread: 1,
  fit_area: true,
  centered_smoothing: true,
  auto_exposure: true,
  warm_greys: 0.5,
};

type NumKey = Exclude<keyof Look, "fit_area" | "centered_smoothing" | "auto_exposure">;

type Spec = { key: NumKey; label: string; hint: string; min: number; max: number; step: number; fmt: (v: number) => string };

const specs: Spec[] = [
  { key: "brightness", label: "Brightness", hint: "Multiplies every color.", min: 0.1, max: 2, step: 0.05, fmt: (v) => `${Math.round(v * 100)}%` },
  { key: "saturation", label: "Saturation", hint: "Above 100% makes colors pop, below washes them out.", min: 0, max: 2, step: 0.05, fmt: (v) => `${Math.round(v * 100)}%` },
  { key: "warm_greys", label: "Warm greys", hint: "Grey, washed-out scenes lean to a warm white instead of a cold grey. Colorful scenes are untouched.", min: 0, max: 1, step: 0.05, fmt: (v) => (v ? `${Math.round(v * 100)}%` : "Off") },
  { key: "min_brightness", label: "Minimum brightness", hint: "Keeps a soft light during dark scenes instead of going black.", min: 0, max: 0.3, step: 0.01, fmt: (v) => (v ? `${Math.round(v * 100)}%` : "Off") },
  { key: "smoothing_ms", label: "Smoothing", hint: "How long a change of color takes. Higher is calmer, lower follows cuts more sharply.", min: 0, max: 2000, step: 50, fmt: (v) => (v ? `${v} ms` : "Off") },
  { key: "spread", label: "Spread", hint: "How much of the screen each light reads. Lower: each light more distinct. Higher: calmer, closer colors.", min: 0.4, max: 3, step: 0.1, fmt: (v) => `${Math.round(v * 100)}%` },
];


// The sync offset lives on the Now page; never overwrite it from here.
const withoutOffset = ({ offset_ms: _, ...rest }: Look) => rest;

export function LookView() {
  const { cfg, patch, status } = useApp();
  const [look, setLook] = useState<Look>(cfg.look);
  const [error, setError] = useState<string>();
  const save = useDebounced((next: Look) =>
    patch({ look: withoutOffset(next) }).then(() => setError(undefined), (e: Error) => setError(e.message)),
  );
  const set = (next: Look) => {
    setLook(next);
    save(next);
  };

  const changed =
    look.fit_area !== DEFAULT.fit_area ||
    look.centered_smoothing !== DEFAULT.centered_smoothing ||
    look.auto_exposure !== DEFAULT.auto_exposure ||
    specs.some(({ key }) => Math.abs(look[key] - DEFAULT[key]) > 1e-4); // f32 on the server side

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

      <FrameTest />

      <div className="grid gap-5 lg:grid-cols-5">
        <Card className="lg:col-span-3">
          <div className="divide-y divide-white/[0.05]">
            {specs.map((s) => (
              <div key={s.key} className="px-5 py-5">
                <Slider
                  id={s.key}
                  label={s.label}
                  display={s.fmt(look[s.key])}
                  value={look[s.key]}
                  min={s.min}
                  max={s.max}
                  step={s.step}
                  onChange={(v) => set({ ...look, [s.key]: v })}
                  hint={s.hint}
                />
              </div>
            ))}
            <SwitchRow
              className="px-5 py-5"
              label="Auto exposure"
              hint="Dark films are brightened to a usual level, measured once per film (up to 3×, never darker). The gain shows on the Now page."
              checked={look.auto_exposure}
              onChange={(v) => set({ ...look, auto_exposure: v })}
            />
            <SwitchRow
              className="px-5 py-5"
              label="Centered smoothing"
              hint="lueur knows the film ahead, so it fades around each cut instead of after it: no delay, however calm the smoothing. Off: the lights follow like a live signal and lag by the smoothing time."
              checked={look.centered_smoothing}
              onChange={(v) => set({ ...look, centered_smoothing: v })}
            />
            <SwitchRow
              className="px-5 py-5"
              label="Fit to area"
              hint="Stretches the lights' positions so the outermost ones read the screen's edges. Turn it off if your lights are placed far apart in the Hue app already."
              checked={look.fit_area}
              onChange={(v) => set({ ...look, fit_area: v })}
            />
          </div>
        </Card>

        <LookPreview look={look} />
      </div>
    </div>
  );
}
