import { useState } from "react";
import { ArrowRight, Clapperboard, Loader2, Pause, Rainbow, Sparkles, Tv } from "lucide-react";
import { api } from "../api";
import { setupSteps, useApp } from "../App";
import { AreaView } from "../components/AreaView";
import { Alert, Button, Card, CardHeader, Empty, Pill, Progress, rgb, time } from "../ui";

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
