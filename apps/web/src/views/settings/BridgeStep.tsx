import { ArrowRight, Check, Lightbulb, Loader2, RefreshCw, Router } from "lucide-react";
import { useEffect, useState } from "react";
import { api, useLoad } from "../../api";
import { useApp } from "../../context";
import { Alert, Button, cx, Field, Input } from "../../ui";
import { Choice, Step } from "./Step";

export function BridgeStep({ done }: { done: boolean }) {
  const { cfg } = useApp();
  const [open, setOpen] = useState(!done);
  return (
    <Step
      n={1}
      title="Hue bridge"
      done={done}
      open={open}
      onToggle={() => setOpen(!open)}
      summary={done ? `Paired with ${cfg.hue.bridge}` : "Find your bridge and pair lueur with it"}
    >
      <BridgePicker onPaired={() => setOpen(false)} />
    </Step>
  );
}

function BridgePicker({ onPaired }: { onPaired: () => void }) {
  const { cfg } = useApp();
  const found = useLoad(api.bridges);
  const [host, setHost] = useState(cfg.hue.bridge);
  const [pairing, setPairing] = useState(false);

  useEffect(() => {
    if (!host && found.data?.length) setHost(found.data[0].ip);
  }, [found.data]);

  if (pairing) return <Pairing host={host} onDone={onPaired} onCancel={() => setPairing(false)} />;

  return (
    <div className="space-y-5">
      <div>
        <div className="mb-2 flex items-center justify-between">
          <span className="text-xs font-medium text-zinc-400">On your network</span>
          <Button variant="ghost" className="h-7 px-2 text-xs" onClick={found.reload} disabled={found.loading} icon={<RefreshCw className={cx("size-3.5", found.loading && "animate-spin")} />}>
            Search again
          </Button>
        </div>
        {found.loading && !found.data ? (
          <div className="flex items-center gap-2 rounded-xl border border-dashed border-white/10 px-4 py-5 text-sm text-zinc-500">
            <Loader2 className="size-4 animate-spin" /> Looking for bridges…
          </div>
        ) : found.data?.length ? (
          <div className="grid gap-2 sm:grid-cols-2">
            {found.data.map((b) => (
              <Choice key={b.ip} selected={host === b.ip} onClick={() => setHost(b.ip)}>
                <Router className="size-4 text-zinc-400" />
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium">{b.name ?? "Hue Bridge"}</p>
                  <p className="font-mono text-xs text-zinc-500">{b.ip}{b.model ? ` · ${b.model}` : ""}</p>
                </div>
              </Choice>
            ))}
          </div>
        ) : (
          <p className="rounded-xl border border-dashed border-white/10 px-4 py-5 text-sm text-zinc-500">
            No bridge found automatically. Enter its IP address below (Hue app → Settings → Bridges).
          </p>
        )}
      </div>
      <div className="flex flex-wrap items-end gap-3">
        <div className="min-w-48 flex-1">
          <Field label="Bridge address">
            <Input value={host} onChange={(e) => setHost(e.target.value)} placeholder="192.168.1.20" spellCheck={false} />
          </Field>
        </div>
        <Button variant="primary" disabled={!host.trim()} onClick={() => setPairing(true)}>
          Pair <ArrowRight className="size-4" />
        </Button>
      </div>
    </div>
  );
}

const PAIR_SECONDS = 45;

function Pairing({ host, onDone, onCancel }: { host: string; onDone: () => void; onCancel: () => void }) {
  const { reloadConfig } = useApp();
  const [left, setLeft] = useState(PAIR_SECONDS);
  const [state, setState] = useState<"waiting" | "paired" | "timeout">("waiting");
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (state !== "waiting") return;
    let alive = true;
    const start = Date.now();
    const tick = async () => {
      const l = PAIR_SECONDS - Math.floor((Date.now() - start) / 1000);
      if (!alive) return;
      setLeft(l);
      if (l <= 0) return setState("timeout");
      try {
        const r = await api.pair(host);
        if (!alive) return;
        if (r.paired) {
          setState("paired");
          reloadConfig();
          setTimeout(onDone, 1200);
          return;
        }
        setError(undefined);
      } catch (e) {
        if (alive) setError((e as Error).message);
      }
      setTimeout(tick, 1000);
    };
    tick();
    return () => {
      alive = false;
    };
  }, [state, host]);

  const r = 26;
  const circ = 2 * Math.PI * r;

  return (
    <div className="flex flex-col items-center py-4 text-center">
      <div className="relative grid size-28 place-items-center">
        {state === "waiting" && (
          <svg className="absolute inset-0 -rotate-90" viewBox="0 0 64 64">
            <circle cx="32" cy="32" r={r} fill="none" stroke="rgb(255 255 255 / 0.08)" strokeWidth="3" />
            <circle
              cx="32"
              cy="32"
              r={r}
              fill="none"
              stroke="url(#pair)"
              strokeWidth="3"
              strokeLinecap="round"
              strokeDasharray={circ}
              strokeDashoffset={circ * (1 - left / PAIR_SECONDS)}
              className="transition-[stroke-dashoffset] duration-1000 ease-linear"
            />
            <defs>
              <linearGradient id="pair">
                <stop offset="0" stopColor="var(--color-glow-1)" />
                <stop offset="1" stopColor="var(--color-glow-2)" />
              </linearGradient>
            </defs>
          </svg>
        )}
        <div
          className={cx(
            "grid size-16 place-items-center rounded-full",
            state === "paired" ? "bg-emerald-400/15 text-emerald-300" : "bg-white/[0.04] text-zinc-200",
          )}
        >
          {state === "paired" ? <Check className="size-7" /> : <Lightbulb className="size-7" />}
        </div>
      </div>
      {state === "waiting" && (
        <>
          <p className="mt-4 text-base font-medium">Press the link button on your bridge</p>
          <p className="mt-1 text-sm text-zinc-500">
            The big round button on top of {host}. Waiting {left} s…
          </p>
        </>
      )}
      {state === "paired" && <p className="mt-4 text-base font-medium">Paired</p>}
      {state === "timeout" && (
        <>
          <p className="mt-4 text-base font-medium">The button wasn't pressed in time</p>
          <Button className="mt-3" onClick={() => setState("waiting")}>Try again</Button>
        </>
      )}
      {error && <div className="mt-4 w-full max-w-md text-left"><Alert>{error}</Alert></div>}
      {state !== "paired" && (
        <Button variant="ghost" className="mt-3" onClick={onCancel}>
          Cancel
        </Button>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ Area */
