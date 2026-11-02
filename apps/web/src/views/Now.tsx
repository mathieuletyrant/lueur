import { useState } from "react";
import { ArrowRight, Clapperboard, Loader2, Pause, Rainbow, Sparkles, Tv } from "lucide-react";
import { api, useDebounced } from "../api";
import { setupSteps, useApp } from "../App";
import { AreaView } from "../components/AreaView";
import { Alert, Button, Card, CardHeader, Empty, Pill, Progress, rgb, time, Decoder, Slider, SwitchRow } from "../ui";

export function Now() {
  const { cfg, status, go } = useApp();
  const steps = setupSteps(cfg, status);
  const done = Object.values(steps).filter(Boolean).length;
  const p = status?.player;

  return (
    <div className="space-y-5">
      {done < 4 && (
        <Card className="flex flex-wrap items-center justify-between gap-4 px-5 py-4">
          <div className="flex items-center gap-3">
            <div className="grid size-9 place-items-center rounded-xl bg-gradient-to-br from-glow-1/20 to-glow-2/20 text-glow-1 ring-1 ring-glow-1/20">
              <Sparkles className="size-4" />
            </div>
            <div>
              <p className="text-sm font-medium">Finish setting up lueur</p>
              <p className="text-sm text-zinc-500">{done} of 4 steps done</p>
            </div>
          </div>
          <Button variant="primary" onClick={() => go("setup")}>
            Continue setup <ArrowRight className="size-4" />
          </Button>
        </Card>
      )}

      {done === 4 && p?.error && <Alert>{p.error}</Alert>}
      {p?.jellyfin_error && <Alert tone="amber">Jellyfin: {p.jellyfin_error}</Alert>}

      <NowPlaying />
      <SyncCard />

      <div className="grid gap-5 lg:grid-cols-5">
        <AreaCard />
        <QueueCard />
      </div>
    </div>
  );
}

function NowPlaying() {
  const { status } = useApp();
  const p = status?.player;
  const np = p?.playing;
  const lit = p?.streaming ? p.channels.filter((c) => c.color.some((v) => v > 4)) : [];

  let pill = <Pill>Idle</Pill>;
  if (np && !np.has_track) pill = <Pill tone="amber">No track yet</Pill>;
  else if (np?.paused) pill = <Pill tone="blue">Paused</Pill>;
  else if (p?.streaming) pill = <Pill tone="green" pulse>Streaming</Pill>;

  return (
    <Card className="min-h-60">
      {/* Ambient glow: each lit channel paints its part of the card. */}
      <div className="pointer-events-none absolute inset-0" aria-hidden>
        {lit.length ? (
          lit.map((c) => (
            <div
              key={c.id}
              className="absolute size-[60%] -translate-x-1/2 -translate-y-1/2 rounded-full opacity-45 blur-3xl transition-colors duration-500"
              style={{ left: `${((c.x + 1) / 2) * 100}%`, top: `${((1 - c.z) / 2) * 100}%`, background: rgb(c.color) }}
            />
          ))
        ) : (
          <div className="absolute -top-1/2 left-1/4 size-[80%] rounded-full bg-gradient-to-br from-glow-1/15 to-glow-2/10 blur-3xl" />
        )}
        <div className="absolute inset-0 bg-gradient-to-t from-zinc-950/90 via-zinc-950/40 to-transparent" />
      </div>

      <div className="relative flex min-h-60 flex-col justify-between gap-8 p-6 md:p-8">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium tracking-wider text-zinc-400 uppercase">Now playing</span>
          {pill}
        </div>

        {np ? (
          <div>
            <h2 className="text-3xl font-semibold tracking-tight text-balance text-white md:text-4xl">{np.title}</h2>
            <p className="mt-2 flex items-center gap-2 text-sm text-zinc-300">
              <Tv className="size-4 opacity-70" /> {np.device}
              {np.paused && (
                <>
                  <span className="text-zinc-600">·</span>
                  <Pause className="size-3.5" /> Paused
                </>
              )}
            </p>
            {np.duration ? (
              <div className="mt-6 flex items-center gap-3 text-xs text-zinc-400 tabular-nums">
                <span>{time(np.position)}</span>
                <Progress value={np.position / np.duration} className="flex-1 bg-white/10" />
                <span>{time(np.duration)}</span>
              </div>
            ) : (
              <p className="mt-6 text-sm text-amber-200/80">
                No color track for this file yet.{" "}
                {/* The player queues it when auto-analysis is on. */}
                Analyze it from the Library page, the lights will follow on the next playback.
              </p>
            )}
          </div>
        ) : (
          <div>
            <h2 className="text-2xl font-semibold tracking-tight text-zinc-200">Nothing playing</h2>
            <p className="mt-1.5 max-w-md text-sm text-zinc-500">
              Start a film or an episode on Jellyfin{p?.area ? ` and “${p.area}” will follow it` : ""}.
            </p>
          </div>
        )}
      </div>
    </Card>
  );
}

function AreaCard() {
  const { status, cfg } = useApp();
  const p = status?.player;
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string>();

  const test = async () => {
    setTesting(true);
    setError(undefined);
    try {
      await api.test();
      await new Promise((r) => setTimeout(r, 6000));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setTesting(false);
    }
  };

  return (
    <Card className="lg:col-span-3">
      <CardHeader
        title={p?.area ?? "Entertainment area"}
        subtitle={p?.channels.length ? `${p.channels.length} channels` : "Not connected"}
        action={
          <Button onClick={test} loading={testing} disabled={!cfg.paired || p?.streaming} icon={<Rainbow className="size-4" />}>
            Test
          </Button>
        }
      />
      <div className="px-5 pt-2 pb-5">
        {p?.channels.length ? (
          <>
            <AreaView channels={p.channels} onChannel={p.streaming ? undefined : (id) => api.test(id).catch((e: Error) => setError(e.message))} />
            {!p.streaming && <p className="mt-2 text-center text-xs text-zinc-500">Click a light to make it blink.</p>}
          </>
        ) : (
          <Empty icon={<Tv className="size-4" />} title="No area yet">
            Pair a bridge and pick an entertainment area in Setup.
          </Empty>
        )}
        {error && <Alert>{error}</Alert>}
      </div>
    </Card>
  );
}

function QueueCard() {
  const { status, go } = useApp();
  const q = status?.queue;
  const busy = !!q && (q.running.length > 0 || q.pending > 0);

  return (
    <Card className="flex flex-col lg:col-span-2">
      <CardHeader
        title="Analysis"
        subtitle={busy ? `${q!.pending} waiting` : "Queue is empty"}
        action={
          <Button variant="ghost" onClick={() => go("library")}>
            Library <ArrowRight className="size-4" />
          </Button>
        }
      />
      <div className="flex-1 px-5 pt-3 pb-5">
        {busy ? (
          <ul className="space-y-4">
            {q!.running.map((r) => (
              <li key={r.name}>
                <div className="mb-1.5 flex items-center justify-between gap-3 text-sm">
                  <span className="flex min-w-0 items-center gap-2">
                    <Loader2 className="size-3.5 shrink-0 animate-spin text-glow-1" />
                    <span className="truncate">{r.name}</span>
                    <Decoder cpu={r.cpu} />
                  </span>
                  {r.progress !== null && <span className="text-xs text-zinc-500 tabular-nums">{Math.round(r.progress * 100)}%</span>}
                </div>
                <Progress value={r.progress} />
              </li>
            ))}
          </ul>
        ) : (
          <Empty icon={<Clapperboard className="size-4" />} title={q?.done ? `${q.done} analyzed this session` : "All caught up"}>
            Scan your libraries to analyze new films.
          </Empty>
        )}
      </div>
    </Card>
  );
}

const STEP = 100;

/** How the lights follow the picture: the offset, and how the clock locks on Jellyfin. */
function SyncCard() {
  const { cfg, patch, status } = useApp();
  const [offset, setOffset] = useState(cfg.look.offset_ms);
  const [sync, setSync] = useState(cfg.sync);
  const [error, setError] = useState<string>();
  const report = (p: Promise<unknown>) => p.then(() => setError(undefined), (e: Error) => setError(e.message));
  const saveOffset = useDebounced((v: number) => report(patch({ look: { offset_ms: v } })));
  // Sync settings restart the player: wait for the slider to rest.
  const saveSync = useDebounced((v: typeof sync) => report(patch({ sync: v })), 600);

  const set = (v: number) => {
    v = Math.max(-10000, Math.min(10000, Math.round(v / STEP) * STEP));
    setOffset(v);
    saveOffset(v);
  };
  const setS = (v: Partial<typeof sync>) => {
    const next = { ...sync, ...v };
    setSync(next);
    saveSync(next);
  };
  const delay = status?.player.playing?.report_delay;

  return (
    <Card className="px-5 py-4">
      <div className="flex flex-wrap items-end gap-x-6 gap-y-3">
        <div className="min-w-48 flex-1">
          <Slider
            id="sync-offset"
            label="Sync offset"
            display={`${offset > 0 ? "+" : ""}${(offset / 1000).toFixed(1)} s`}
            value={offset}
            min={-10000}
            max={10000}
            step={STEP}
            onChange={set}
            hint="Lights late on cuts? Move right. Early? Move left. Applies live."
          />
        </div>
        <div className="flex gap-2 pb-5">
          <Button onClick={() => set(offset - STEP)} aria-label="Lights earlier by 0.1 s">−0.1 s</Button>
          <Button onClick={() => set(offset + STEP)} aria-label="Lights later by 0.1 s">+0.1 s</Button>
          <Button variant="ghost" onClick={() => set(0)} disabled={offset === 0}>Reset</Button>
        </div>
      </div>

      <details className="group mt-2 border-t border-white/[0.06] pt-3">
        <summary className="cursor-pointer list-none text-xs font-medium text-zinc-400 hover:text-zinc-200">
          <span className="inline-block transition group-open:rotate-90">›</span> Advanced
        </summary>
        <div className="mt-4 grid gap-6 md:grid-cols-3">
          <SwitchRow
            label="Learn each app's report delay"
            hint={
              <>
                Some apps tell Jellyfin their position late. lueur measures it on each report and corrects the first
                seconds after a play or a seek.
                {delay != null && ` Measured for this app: ${delay.toFixed(1)} s.`}
              </>
            }
            checked={sync.learn_report_delay}
            onChange={(v) => setS({ learn_report_delay: v })}
          />
          <Slider
            id="sync-jump"
            label="Re-sync after"
            display={`${(sync.jump_ms / 1000).toFixed(2)} s`}
            value={sync.jump_ms}
            min={250}
            max={3000}
            step={50}
            onChange={(v) => setS({ jump_ms: v })}
            hint="A gap this large moves the lights at once. Lower: snappier after a seek, but may twitch with an app that reports unevenly."
          />
          <Slider
            id="sync-drift"
            label="Correct drifts above"
            display={`${(sync.drift_ms / 1000).toFixed(2)} s`}
            value={sync.drift_ms}
            min={0}
            max={1000}
            step={25}
            onChange={(v) => setS({ drift_ms: v })}
            hint="Smaller gaps are corrected gently, without a visible jump."
          />
        </div>
      </details>
      {error && <div className="mt-3"><Alert>{error}</Alert></div>}
    </Card>
  );
}
