import { Check, Loader2, Play, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { api, type Corner, type WledConfig, type WledFound, useLoad } from "../../api";
import { StripView } from "../../components/StripView";
import { useApp } from "../../context";
import { stripLeds } from "../../lib/strip";
import { Alert, Button, cx, Field, Input, Segmented, Select } from "../../ui";
import { Choice, Step } from "./Step";

const CORNERS: { value: Corner; label: string }[] = [
  { value: "bottom-left", label: "Bottom left" },
  { value: "top-left", label: "Top left" },
  { value: "top-right", label: "Top right" },
  { value: "bottom-right", label: "Bottom right" },
];

const SIDES = ["left", "top", "right", "bottom"] as const;

/** The WLED Strip: its controller and its Layout, seen from the couch. */
export function WledStep({ done }: { done: boolean }) {
  const { cfg, status, patch } = useApp();
  const [open, setOpen] = useState(!done);
  const [layout, setLayout] = useState<WledConfig>(cfg.wled);
  const [controller, setController] = useState<WledFound>();
  const [checking, setChecking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();
  const found = useLoad(api.wledDiscover);

  const leds = stripLeds(layout);
  const saved = JSON.stringify(layout) === JSON.stringify(cfg.wled) && !!cfg.wled.host;
  const set = (p: Partial<WledConfig>) => setLayout((l) => ({ ...l, ...p }));

  const check = async (host: string) => {
    setChecking(true);
    setError(undefined);
    try {
      const c = await api.wledInfo(host);
      setController(c);
      // A new Strip: start with all its LEDs on the top, to be split across the sides.
      const total = layout.left + layout.top + layout.right + layout.bottom;
      set(total === 0 ? { host, top: c.leds } : { host });
    } catch (e) {
      setController(undefined);
      setError((e as Error).message);
    } finally {
      setChecking(false);
    }
  };

  // The saved controller's LED count, to compare with the Layout.
  useEffect(() => {
    if (cfg.wled.host) api.wledInfo(cfg.wled.host).then(setController, () => {});
  }, [cfg.wled.host]);

  const save = async () => {
    setSaving(true);
    setError(undefined);
    try {
      await patch({ wled: { ...layout, host: layout.host.trim() } });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const live = new Map((status?.player.channels ?? []).map((c) => [c.id, c.color]));
  const mismatch = controller && leds.length !== controller.leds;

  return (
    <Step
      n={1}
      title="WLED strip"
      done={done}
      open={open}
      onToggle={() => setOpen(!open)}
      summary={done ? `${cfg.wled.host} · ${stripLeds(cfg.wled).length} LEDs` : "Find the controller and describe the strip"}
    >
      <div className="space-y-6">
        <section>
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-medium text-zinc-400">On your network</span>
            <Button
              variant="ghost"
              className="h-7 px-2 text-xs"
              onClick={found.reload}
              disabled={found.loading}
              icon={<RefreshCw className={cx("size-3.5", found.loading && "animate-spin")} />}
            >
              Search again
            </Button>
          </div>
          {found.loading && !found.data ? (
            <div className="flex items-center gap-2 rounded-xl border border-dashed border-white/10 px-4 py-5 text-sm text-zinc-500">
              <Loader2 className="size-4 animate-spin" /> Looking for WLED controllers…
            </div>
          ) : found.data?.length ? (
            <div className="grid gap-3 sm:grid-cols-2">
              {found.data.map((c) => (
                <Choice key={c.host} selected={layout.host === c.host} onClick={() => check(c.host)}>
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">{c.name}</p>
                    <p className="text-xs text-zinc-500">
                      {c.host} · {c.leds} LEDs · WLED {c.version}
                    </p>
                  </div>
                </Choice>
              ))}
            </div>
          ) : (
            <p className="text-sm text-zinc-500">
              None found. Type its address below (WLED shows it in Config → WiFi Setup).
            </p>
          )}
          <div className="mt-4 flex items-end gap-3">
            <Field label="Controller address">
              <Input
                value={layout.host}
                onChange={(e) => set({ host: e.target.value })}
                placeholder="192.168.1.80"
                spellCheck={false}
              />
            </Field>
            <Button onClick={() => check(layout.host.trim())} loading={checking} disabled={!layout.host.trim()}>
              Check
            </Button>
          </div>
          {controller && controller.host === layout.host.trim() && (
            <p className="mt-2 flex items-center gap-1.5 text-sm text-emerald-300">
              <Check className="size-4" /> {controller.name}, {controller.leds} LEDs set up in WLED
            </p>
          )}
        </section>

        <section className="grid gap-6 lg:grid-cols-2">
          <div className="space-y-4">
            <p className="text-sm text-zinc-500">
              Seen from the couch: where LED 0 is, which way the strip runs from there, and how many LEDs are behind each
              side of the TV (0 for a side without any, like the bottom behind a TV stand).
            </p>
            <Field label="LED 0 is at the">
              <Select value={layout.start} onChange={(e) => set({ start: e.target.value as Corner })}>
                {CORNERS.map((c) => (
                  <option key={c.value} value={c.value}>
                    {c.label} corner
                  </option>
                ))}
              </Select>
            </Field>
            <div>
              <span className="mb-1.5 block text-xs font-medium text-zinc-400">Then it runs</span>
              <Segmented
                label="Direction"
                value={layout.clockwise ? "cw" : "ccw"}
                onChange={(v) => set({ clockwise: v === "cw" })}
                options={[
                  { value: "cw", label: "Clockwise" },
                  { value: "ccw", label: "Counter-clockwise" },
                ]}
              />
            </div>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
              {SIDES.map((side) => (
                <Field key={side} label={`LEDs ${side}`}>
                  <Input
                    type="number"
                    min={0}
                    max={500}
                    value={layout[side]}
                    onChange={(e) => set({ [side]: Math.max(0, Number(e.target.value) || 0) })}
                  />
                </Field>
              ))}
            </div>
            <p className={cx("text-sm", mismatch ? "text-amber-300" : "text-zinc-500")}>
              {leds.length} LEDs in this layout
              {mismatch && `, but WLED has ${controller!.leds}: the ones past the layout stay as WLED leaves them`}
            </p>
            {leds.length > 256 && <Alert>A strip longer than 256 LEDs isn't supported yet.</Alert>}
          </div>
          <div>
            <StripView leds={leds} colors={saved ? live : undefined} onLed={saved ? (id) => api.test(id).catch(() => {}) : undefined} />
            <p className="mt-2 text-xs text-zinc-500">
              {saved
                ? "Click a LED to make it blink on the strip, or play the rainbow: it starts at LED 0 and runs in the direction of the arrow."
                : "Save, then check it on the strip itself."}
            </p>
          </div>
        </section>

        {error && <Alert>{error}</Alert>}
        <div className="flex flex-wrap gap-3">
          <Button
            variant="primary"
            onClick={save}
            loading={saving}
            disabled={saved || !layout.host.trim() || leds.length === 0 || leds.length > 256}
          >
            Save
          </Button>
          <Button onClick={() => api.test().catch(() => {})} disabled={!saved} icon={<Play className="size-4" />}>
            Rainbow
          </Button>
        </div>
      </div>
    </Step>
  );
}
