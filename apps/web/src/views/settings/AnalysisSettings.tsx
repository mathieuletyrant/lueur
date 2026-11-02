import { type ReactNode, useState } from "react";
import { type Config } from "../../api";
import { useApp } from "../../context";
import { ago, hh, toLocal, toUtc } from "../../lib/hours";
import { Button, Card, CardHeader, Input, Segmented, Select, SwitchRow } from "../../ui";

const INTERVALS = [
  { value: 0, label: "Off" },
  { value: 5, label: "5 min" },
  { value: 15, label: "15 min" },
  { value: 60, label: "1 h" },
  { value: 360, label: "6 h" },
];

export const SPEEDS = [
  { value: 5, label: "Gentle", hint: "5× real time: a 2-hour film in about 25 min. Quiet and cool, for daytime." },
  { value: 10, label: "Normal", hint: "10× real time: a 2-hour film in about 12 min." },
  { value: 25, label: "Fast", hint: "25× real time: a 2-hour film in about 5 min. The server warms up." },
  { value: 0, label: "Hurt my GPU", hint: "As fast as the GPU goes. Great for a first run at night, hot otherwise." },
];

/** One line for the Library page: how lueur analyzes right now. */
export function analysisSummary(cfg: Config) {
  const w = cfg.analysis.window;
  return [
    SPEEDS.find((s) => s.value === cfg.analysis.max_speed)?.label ?? `${cfg.analysis.max_speed}×`,
    cfg.auto_scan_minutes ? `auto scan every ${INTERVALS.find((i) => i.value === cfg.auto_scan_minutes)?.label}` : "no auto scan",
    w && `${hh(toLocal(w.start))}–${hh(toLocal(w.end))}`,
  ]
    .filter(Boolean)
    .join(" · ");
}

/** When and how hard lueur analyzes on its own. */
export function AnalysisSettings() {
  const { cfg, status, patch } = useApp();
  const last = status?.last_scan;
  const speed = SPEEDS.find((s) => s.value === cfg.analysis.max_speed);
  return (
    <Card>
      <span id="analysis" className="absolute -top-20" />
      <CardHeader title="Analysis" subtitle="New films get their track on their own, at the pace you choose." />
      <div className="grid gap-x-10 gap-y-6 px-5 pt-5 pb-5 md:grid-cols-2">
        <div className="space-y-6">
          <Setting
            label="Speed"
            hint={speed?.hint ?? `${cfg.analysis.max_speed}× real time.`}
          >
            <Segmented
              label="Analysis speed"
              options={SPEEDS}
              value={cfg.analysis.max_speed}
              onChange={(v) => patch({ analysis: { max_speed: v } })}
            />
          </Setting>
          <Setting
            label="Auto scan"
            hint={
              last
                ? `Last ${last.auto ? "auto " : ""}scan ${ago(last.at)}: ${last.queued} queued${last.removed ? `, ${last.removed} old track${last.removed > 1 ? "s" : ""} removed` : ""}.`
                : "Looks for new videos in your libraries."
            }
          >
            <Segmented
              label="Auto scan interval"
              options={INTERVALS}
              value={cfg.auto_scan_minutes}
              onChange={(v) => patch({ auto_scan_minutes: v })}
            />
          </Setting>
          <Setting label="Parallel analyses" hint="More is faster, and hotter.">
            <Segmented
              label="Parallel analyses"
              options={[1, 2, 3, 4].map((n) => ({ value: n, label: String(n) }))}
              value={cfg.jobs || 1}
              onChange={(v) => patch({ jobs: v })}
            />
          </Setting>
        </div>
        <div className="space-y-6">
          <AnalysisHours />
          <SwitchRow
            label="Analyze on first playback"
            hint="A film played without a track is analyzed first; the lights join in when it is done."
            checked={cfg.auto_analyze}
            onChange={(v) => patch({ auto_analyze: v })}
          />
          <Exclude />
        </div>
      </div>
    </Card>
  );
}

function Setting({ label, hint, children }: { label: string; hint: ReactNode; children: ReactNode }) {
  return (
    <div>
      <p className="text-sm font-medium">{label}</p>
      <div className="mt-2">{children}</div>
      <p className="mt-2 text-xs text-zinc-500">{hint}</p>
    </div>
  );
}

/** Analyze only during some hours (a film you ask for or play starts anyway). */
function AnalysisHours() {
  const { cfg, patch } = useApp();
  const w = cfg.analysis.window;
  const [start, end] = w ? [toLocal(w.start), toLocal(w.end)] : [1, 7];
  const save = (s: number, e: number) => patch({ analysis: { window: { start: toUtc(s), end: toUtc(e) } } });
  const hours = Array.from({ length: 24 }, (_, h) => h);
  return (
    <div className="space-y-3">
      <SwitchRow
        label="Only at certain hours"
        hint="Outside these hours, new videos wait. A film you play or ask to analyze starts anyway."
        checked={!!w}
        onChange={(on) => (on ? save(start, end) : patch({ analysis: { window: null } }))}
      />
      {w && (
        <div className="flex items-center gap-2 text-sm text-zinc-400">
          From
          <Select value={start} onChange={(e) => save(Number(e.target.value), end)} className="w-24" aria-label="Start">
            {hours.map((h) => <option key={h} value={h}>{hh(h)}</option>)}
          </Select>
          to
          <Select value={end} onChange={(e) => save(start, Number(e.target.value))} className="w-24" aria-label="End">
            {hours.map((h) => <option key={h} value={h}>{hh(h)}</option>)}
          </Select>
        </div>
      )}
    </div>
  );
}

/** Keywords that keep files (trailers, samples, extras…) out of every analysis. */
export function Exclude() {
  const { cfg, patch } = useApp();
  const [draft, setDraft] = useState("");
  const save = (exclude: string[]) => patch({ exclude });
  const add = () => {
    const k = draft.trim();
    if (k && !cfg.exclude.includes(k)) save([...cfg.exclude, k]);
    setDraft("");
  };
  return (
    <div>
      <p className="text-sm font-medium">Skip files containing</p>
      <p className="mt-0.5 text-xs text-zinc-500">Matched anywhere in the path, ignoring case. Their tracks are removed.</p>
      <ul className="mt-3 flex flex-wrap gap-1.5">
        {cfg.exclude.map((k) => (
          <li
            key={k}
            className="flex items-center gap-1 rounded-md bg-white/[0.06] py-0.5 pr-1 pl-2 font-mono text-xs text-zinc-300 ring-1 ring-white/10 ring-inset"
          >
            {k}
            <button
              onClick={() => save(cfg.exclude.filter((x) => x !== k))}
              className="rounded px-1 text-zinc-500 hover:bg-white/10 hover:text-zinc-100"
              aria-label={`Stop skipping ${k}`}
            >
              ×
            </button>
          </li>
        ))}
      </ul>
      <form
        className="mt-2 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <Input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="-trailer, /extras/…" className="h-8 font-mono text-xs" />
        <Button type="submit" className="h-8 px-3 text-xs" disabled={!draft.trim()}>
          Add
        </Button>
      </form>
    </div>
  );
}
