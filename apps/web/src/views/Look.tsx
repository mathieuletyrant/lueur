import { useEffect, useMemo, useRef, useState } from "react";
import { Lightbulb, RotateCcw, Square } from "lucide-react";
import { api, useDebounced, useLoad, type Look } from "../api";
import { useApp } from "../App";
import { AreaView } from "../components/AreaView";
import { Barcode } from "../components/Barcode";
import { Alert, Button, Card, CardHeader, PageHeader, rgb, Select, Slider, SwitchRow, time } from "../ui";

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

// Same maths as the player (apps/server/src/color.rs), for the preview.
const toLin = (v: number) => ((v /= 255) <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);
const toSrgb = (c: number) => {
  c = Math.min(1, Math.max(0, c));
  return Math.round((c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055) * 255);
};
const WARM = [1, 0.456, 0.147]; // 2700 K, linear
const lum = (c: number[]) => 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
function apply(look: Look, srgb: [number, number, number]): [number, number, number] {
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

const palette: { name: string; c: [number, number, number] }[] = [
  { name: "Night", c: [22, 34, 66] },
  { name: "Teal", c: [38, 104, 112] },
  { name: "Skin", c: [214, 150, 112] },
  { name: "Sunset", c: [232, 96, 48] },
  { name: "Forest", c: [52, 92, 44] },
  { name: "Neon", c: [190, 40, 150] },
  { name: "Grey sky", c: [150, 156, 162] },
  { name: "Dark room", c: [14, 10, 8] },
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

/** Hold a moment of a film on the lights, to tune the Look in front of them. */
function FrameTest() {
  const { status, cfg } = useApp();
  const videos = useLoad(api.videos);
  const analyzed = useMemo(() => (videos.data ?? []).filter((v) => v.duration), [videos.data]);
  const [path, setPath] = useState("");
  const [t, setT] = useState(0);
  const [shown, setShown] = useState(0); // the time of the frame on screen (debounced)
  const [error, setError] = useState<string>();
  const holding = !!status?.hold;
  const video = analyzed.find((v) => v.path === path);
  const p = status?.player;

  // Default: the first analyzed video, a quarter in.
  useEffect(() => {
    if (!path && analyzed.length) {
      setPath(analyzed[0].path);
      const t0 = Math.round((analyzed[0].duration ?? 0) / 4);
      setT(t0);
      setShown(t0);
    }
  }, [analyzed]);

  // Debounce: one frame (and one move of the lights) per pause of the slider.
  useEffect(() => {
    const id = setTimeout(() => {
      setShown(t);
      if (holding && path) api.hold(path, t).catch((e: Error) => setError(e.message));
    }, 250);
    return () => clearTimeout(id);
  }, [t, path]);

  // Leaving the page gives the lights back.
  const holdingRef = useRef(holding);
  holdingRef.current = holding;
  useEffect(() => () => void (holdingRef.current && api.unhold()), []);

  const toggle = () => {
    setError(undefined);
    (holding ? api.unhold() : api.hold(path, t)).catch((e: Error) => setError(e.message));
  };

  if (videos.data && !analyzed.length) return null;

  return (
    <Card className="mb-5">
      <CardHeader
        title="Test on a frame"
        subtitle="Hold a moment of a film on your lights, then tune the settings below while looking at the room."
        action={
          <Button
            variant={holding ? "secondary" : "primary"}
            onClick={toggle}
            disabled={!video || !cfg.paired || (!!p?.playing?.has_track && !holding)}
            icon={holding ? <Square className="size-3.5" /> : <Lightbulb className="size-4" />}
          >
            {holding ? "Stop" : "Show on lights"}
          </Button>
        }
      />
      <div className="grid gap-5 px-5 pt-4 pb-5 lg:grid-cols-5">
        <div className="space-y-4 lg:col-span-2">
          <label className="block">
            <span className="mb-1.5 block text-xs font-medium text-zinc-400">Film or episode</span>
            <Select
              value={path}
              onChange={(e) => {
                const v = analyzed.find((x) => x.path === e.target.value);
                setPath(e.target.value);
                setT(Math.round((v?.duration ?? 0) / 4));
              }}
            >
              {[...new Set(analyzed.map((v) => v.library))].map((lib) => (
                <optgroup key={lib} label={lib}>
                  {analyzed
                    .filter((v) => v.library === lib)
                    .map((v) => (
                      <option key={v.path} value={v.path}>
                        {v.folder && v.folder !== v.name ? `${v.folder} · ${v.name}` : v.name}
                      </option>
                    ))}
                </optgroup>
              ))}
            </Select>
          </label>
          {video?.duration && <Barcode path={video.path} duration={video.duration} at={t / video.duration} className="h-3" />}
          <Slider
            id="frame-t"
            label="Moment"
            display={
              <>
                {time(t)} <span className="text-zinc-600">/ {time(video?.duration ?? 0)}</span>
              </>
            }
            value={t}
            min={0}
            max={Math.max(1, Math.floor(video?.duration ?? 1))}
            step={1}
            onChange={setT}
            hint={holding ? "The lights follow the slider." : "Pick a scene with strong colors, then show it on the lights."}
          />
          {error && <Alert>{error}</Alert>}
        </div>
        <div className="lg:col-span-3">
          <AreaView
            screen={video ? api.frameUrl(video.path, shown) : undefined}
            channels={holding ? (p?.channels ?? []) : (p?.channels ?? []).map((c) => ({ ...c, color: undefined }))}
          />
        </div>
      </div>
    </Card>
  );
}
