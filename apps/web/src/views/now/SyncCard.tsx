import { useState } from "react";
import { useDebounced } from "../../api";
import { useApp } from "../../context";
import { Alert, Button, Card, Slider, SwitchRow } from "../../ui";

const STEP = 100;

/** How the lights follow the picture: the offset, and how the clock locks on Jellyfin. */
export function SyncCard() {
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
        <summary className="flex cursor-pointer list-none flex-wrap items-center gap-x-2 gap-y-1 text-xs text-zinc-400 hover:text-zinc-200">
          <span className="inline-block font-medium transition group-open:rotate-90">›</span>
          <span className="font-medium">How lueur follows the film</span>
          <span className="text-zinc-600">
            {sync.learn_report_delay ? (delay != null ? `app delay ${delay.toFixed(1)} s corrected` : "app delay: learning") : "app delay ignored"}
            {` · ignores gaps < ${(sync.drift_ms / 1000).toFixed(2)} s · jumps > ${(sync.jump_ms / 1000).toFixed(2)} s`}
          </span>
        </summary>
        <div className="mt-5 space-y-6">
          <SwitchRow
            label="Correct the app's late reports"
            hint={
              <>
                Some Jellyfin apps send their position a few seconds late. lueur measures that delay on each report and
                takes it into account right after you press play or seek.
                {delay != null && <strong className="font-medium text-zinc-300"> This app: {delay.toFixed(1)} s late.</strong>}
              </>
            }
            checked={sync.learn_report_delay}
            onChange={(v) => setS({ learn_report_delay: v })}
          />
          <div>
            <p className="text-sm font-medium">When the lights drift from the film</p>
            <p className="mt-1 text-xs text-zinc-500">
              Each report from Jellyfin tells where the film really is. Depending on how far off the lights are:
            </p>
            {/* The three zones, to scale up to 3 s. */}
            <div className="mt-3 flex h-7 overflow-hidden rounded-lg text-[11px] font-medium">
              <span className="grid place-items-center bg-white/[0.05] text-zinc-400" style={{ width: `${(sync.drift_ms / 3000) * 100}%` }}>
                {sync.drift_ms >= 300 && "ignored"}
              </span>
              <span className="grid place-items-center bg-sky-400/15 text-sky-200" style={{ width: `${((sync.jump_ms - sync.drift_ms) / 3000) * 100}%` }}>
                caught up smoothly
              </span>
              <span className="grid flex-1 place-items-center bg-amber-400/15 text-amber-200">jump at once</span>
            </div>
            <div className="mt-5 grid gap-6 md:grid-cols-2">
              <Slider
                id="sync-drift"
                label="Ignore gaps under"
                display={`${(sync.drift_ms / 1000).toFixed(2)} s`}
                value={sync.drift_ms}
                min={0}
                max={1000}
                step={25}
                onChange={(v) => setS({ drift_ms: Math.min(v, sync.jump_ms - 50) })}
                hint="Too small to see: left alone. Raise it if the lights wobble slightly."
              />
              <Slider
                id="sync-jump"
                label="Jump when off by more than"
                display={`${(sync.jump_ms / 1000).toFixed(2)} s`}
                value={sync.jump_ms}
                min={250}
                max={3000}
                step={50}
                onChange={(v) => setS({ jump_ms: Math.max(v, sync.drift_ms + 50) })}
                hint="After a seek, the lights jump straight to the right moment. Lower: they jump sooner; too low and an app that reports unevenly makes them twitch."
              />
            </div>
          </div>
        </div>
      </details>
      {error && <div className="mt-3"><Alert>{error}</Alert></div>}
    </Card>
  );
}
