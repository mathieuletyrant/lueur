import { Pause, Tv } from "lucide-react";
import { api } from "../../api";
import { useApp } from "../../context";
import { Pill, Progress, rgb, time } from "../../ui";

/** A new frame every this many seconds of film (cached by the browser). */
const FRAME_EVERY = 15;

/** The film as a cinema screen: its current frame in scope format, the
 *  lights' colors spilling around its edges like the real ambilight. */
export function NowPlaying() {
  const { status } = useApp();
  const p = status?.player;
  const np = p?.playing;
  const lit = p?.streaming ? p.channels.filter((c) => c.color.some((v) => v > 4)) : [];
  const frame =
    np?.path && np.has_track
      ? api.frameUrl(np.path, Math.floor(np.position / FRAME_EVERY) * FRAME_EVERY, 1280)
      : undefined;

  let pill = <Pill>Idle</Pill>;
  if (np && !np.has_track) pill = <Pill tone="amber">No track yet</Pill>;
  else if (np?.paused) pill = <Pill tone="blue">Paused</Pill>;
  else if (p?.streaming) pill = <Pill tone="green" pulse>Streaming</Pill>;

  return (
    <div className="relative">
      {/* The lights, around the screen: each lit channel glows on its side. */}
      <div className="pointer-events-none absolute -inset-6 md:-inset-10" aria-hidden>
        {lit.map((c) => (
          <div
            key={c.id}
            className="absolute size-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full opacity-50 blur-3xl transition-colors duration-700"
            style={{ left: `${((c.x + 1) / 2) * 100}%`, top: `${((1 - c.z) / 2) * 100}%`, background: rgb(c.color) }}
          />
        ))}
      </div>

      <section className="relative isolate flex min-h-64 flex-col justify-between overflow-hidden rounded-2xl border border-white/[0.08] bg-black shadow-2xl shadow-black/60 md:aspect-[2.39/1] md:min-h-0">
        {frame ? (
          <img
            key={frame}
            src={frame}
            alt=""
            className="absolute inset-0 -z-10 size-full object-cover [animation:fade-in_1s_ease]"
          />
        ) : (
          <div className="absolute -top-1/2 left-1/4 -z-10 size-[80%] rounded-full bg-gradient-to-br from-glow-1/15 to-glow-2/10 blur-3xl" />
        )}
        <div className="absolute inset-0 -z-10 bg-gradient-to-t from-black via-black/40 to-black/30" />

        <div className="flex items-center justify-between p-6 md:p-8">
          <span className="text-[11px] font-medium tracking-[0.25em] text-zinc-300/80 uppercase">Now playing</span>
          {pill}
        </div>

        <div className="p-6 pt-0 md:p-8 md:pt-0">
          {np ? (
            <>
              <h2 className="font-display text-4xl leading-none text-balance text-white drop-shadow-lg md:text-6xl">{np.title}</h2>
              <p className="mt-3 flex items-center gap-2 text-sm text-zinc-300">
                <Tv className="size-4 opacity-70" /> {np.device}
                {np.exposure != null && np.exposure > 1.05 && (
                  <>
                    <span className="text-zinc-500">·</span>
                    <span title="Auto exposure: this film is dark, the lights are brightened">Brightened ×{np.exposure.toFixed(1)}</span>
                  </>
                )}
                {np.paused && (
                  <>
                    <span className="text-zinc-500">·</span>
                    <Pause className="size-3.5" /> Paused
                  </>
                )}
              </p>
              {np.duration ? (
                <div className="mt-5 flex items-center gap-3 text-xs text-zinc-300 tabular-nums">
                  <span>{time(np.position)}</span>
                  <Progress value={np.position / np.duration} className="h-1 flex-1 bg-white/15" />
                  <span>{time(np.duration)}</span>
                </div>
              ) : (
                <p className="mt-5 text-sm text-amber-200/90">
                  No color track for this file yet. It is being analyzed; the lights join in as soon as it is done.
                </p>
              )}
            </>
          ) : (
            <>
              <h2 className="font-display text-4xl text-zinc-200 md:text-5xl">Nothing playing</h2>
              <p className="mt-2 max-w-md text-sm text-zinc-400">
                Start a film or an episode on Jellyfin{p?.area ? ` and “${p.area}” will follow it` : ""}.
              </p>
            </>
          )}
        </div>
      </section>
    </div>
  );
}
