import { Pause, Tv } from "lucide-react";
import { useApp } from "../../context";
import { Card, Pill, Progress, rgb, time } from "../../ui";

export function NowPlaying() {
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
              {np.exposure != null && np.exposure > 1.05 && (
                <>
                  <span className="text-zinc-600">·</span>
                  <span title="Auto exposure: this film is dark, the lights are brightened">Brightened ×{np.exposure.toFixed(1)}</span>
                </>
              )}
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
