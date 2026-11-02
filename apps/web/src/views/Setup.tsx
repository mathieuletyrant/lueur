import { useEffect, useState, type ReactNode } from "react";
import { ArrowRight, Check, Film, FolderOpen, Library, Lightbulb, Loader2, RefreshCw, Router, Server, Tv } from "lucide-react";
import { api, useLoad, type Library as Lib } from "../api";
import { setupSteps, useApp } from "../App";
import { AreaView } from "../components/AreaView";
import { FolderPicker } from "../components/FolderPicker";
import { Alert, Button, Card, cx, Field, Input, PageHeader, Pill, Select, Toggle } from "../ui";

export function Setup() {
  const { cfg, status } = useApp();
  const s = setupSteps(cfg, status);
  return (
    <div>
      <PageHeader title="Setup" subtitle="Connect your Hue bridge and Jellyfin, then tell lueur where your films are." />
      <ol className="space-y-4">
        <BridgeStep done={s.bridge} />
        <AreaStep done={s.area} locked={!s.bridge} />
        <JellyfinStep done={s.jellyfin} />
        <LibrariesStep done={s.libraries} locked={!s.jellyfin} />
      </ol>
    </div>
  );
}

function Step({
  n,
  title,
  summary,
  done,
  locked,
  open,
  onToggle,
  children,
}: {
  n: number;
  title: string;
  summary: ReactNode;
  done: boolean;
  locked?: boolean;
  open: boolean;
  onToggle?: () => void;
  children: ReactNode;
}) {
  return (
    <li>
      <Card className={cx(locked && "opacity-50")}>
        <div className="flex items-center gap-4 px-5 py-4">
          <span
            className={cx(
              "grid size-7 shrink-0 place-items-center rounded-full text-xs font-semibold",
              done ? "bg-emerald-400/15 text-emerald-300 ring-1 ring-emerald-400/30" : "bg-white/[0.06] text-zinc-300 ring-1 ring-white/10",
            )}
          >
            {done ? <Check className="size-3.5" /> : n}
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="text-sm font-semibold">{title}</h2>
            <div className="truncate text-sm text-zinc-500">{summary}</div>
          </div>
          {onToggle && !locked && (
            <Button variant="ghost" onClick={onToggle}>
              {open ? "Close" : done ? "Change" : "Set up"}
            </Button>
          )}
        </div>
        {open && !locked && <div className="border-t border-white/[0.06] px-5 py-5">{children}</div>}
      </Card>
    </li>
  );
}

/* ---------------------------------------------------------------- Bridge */

function BridgeStep({ done }: { done: boolean }) {
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

function AreaStep({ done, locked }: { done: boolean; locked: boolean }) {
  const { cfg, status, patch } = useApp();
  const [open, setOpen] = useState(!done);
  const areas = useLoad(() => (cfg.paired ? api.areas() : Promise.resolve([])), [cfg.paired, cfg.hue.bridge]);
  const current = cfg.hue.area ?? undefined;

  return (
    <Step
      n={2}
      title="Entertainment area"
      done={done}
      locked={locked}
      open={open}
      onToggle={() => setOpen(!open)}
      summary={done ? status?.player.area ?? "Selected" : "The lights that follow the screen"}
    >
      <p className="mb-4 text-sm text-zinc-500">
        Each dot is a light, or a segment of a gradient strip, where you placed it in the Hue app. lueur colors it from
        that side of the screen. Click a dot of the selected area to make it blink.
      </p>
      {areas.error && <Alert>{areas.error}</Alert>}
      {areas.loading && !areas.data ? (
        <Loader2 className="size-4 animate-spin text-zinc-500" />
      ) : areas.data?.length ? (
        <div className="grid gap-3 sm:grid-cols-2">
          {areas.data.map((a) => {
            const selected = current === a.id || current === a.metadata.name || (!current && areas.data!.length === 1);
            return (
              <Choice
                key={a.id}
                selected={selected}
                onClick={() => patch({ hue: { area: a.id } }).then(() => setOpen(false))}
                className="flex-col items-stretch"
              >
                <AreaView
                  compact
                  channels={a.channels.map((c) => ({ id: c.channel_id, x: c.position.x, z: c.position.z }))}
                  onChannel={selected ? (id) => api.test(id).catch(() => {}) : undefined}
                />
                <div className="flex items-center justify-between">
                  <p className="flex items-center gap-2 text-sm font-medium">
                    {a.metadata.name}
                    {selected && <Pill tone="green">Selected</Pill>}
                  </p>
                  <p className="text-xs text-zinc-500">{a.channels.length} channels · {a.configuration_type}</p>
                </div>
              </Choice>
            );
          })}
        </div>
      ) : (
        !areas.error && (
          <p className="text-sm text-zinc-500">
            No entertainment area on this bridge. Create one in the Hue app (Settings → Entertainment areas), then come back.
          </p>
        )
      )}
    </Step>
  );
}

/* -------------------------------------------------------------- Jellyfin */

function JellyfinStep({ done }: { done: boolean }) {
  const { cfg, patch } = useApp();
  const [open, setOpen] = useState(!done);
  const [url, setUrl] = useState(cfg.jellyfin.url);
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();
  const info = useLoad(() => (done ? api.jellyfinInfo() : Promise.resolve(undefined)), [done, cfg.jellyfin.url]);
  const devices = useLoad(() => (info.data ? api.jellyfinDevices() : Promise.resolve([])), [info.data]);

  const save = async () => {
    setSaving(true);
    setError(undefined);
    try {
      await patch({ jellyfin: { url: url.trim().replace(/\/$/, ""), api_key: key.trim() } });
      setKey("");
      info.reload();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Step
      n={3}
      title="Jellyfin"
      done={done}
      open={open}
      onToggle={() => setOpen(!open)}
      summary={info.data ? `${info.data.name} · Jellyfin ${info.data.version}` : done ? cfg.jellyfin.url : "Where lueur learns what's playing"}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Server URL">
          <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="http://192.168.1.10:8096" spellCheck={false} />
        </Field>
        <Field label="API key" hint="Dashboard → API keys. Must be an admin key, to read file paths.">
          <Input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={cfg.has_api_key ? "Saved, leave empty to keep" : "Paste the key"}
          />
        </Field>
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <Button variant="primary" onClick={save} loading={saving} disabled={!url.trim() || (!key.trim() && !cfg.has_api_key)}>
          Save and test
        </Button>
        {info.loading && done && <Loader2 className="size-4 animate-spin text-zinc-500" />}
        {info.data && (
          <span className="flex items-center gap-1.5 text-sm text-emerald-300">
            <Check className="size-4" /> Connected to {info.data.name}
          </span>
        )}
      </div>
      {(error || (done && info.error)) && <div className="mt-4"><Alert>{error ?? info.error}</Alert></div>}

      {info.data && (
        <div className="mt-6 border-t border-white/[0.06] pt-5">
          <Field label="Follow one device only" hint="Useful when several people watch Jellyfin at home.">
            <Select
              value={cfg.jellyfin.device ?? ""}
              onChange={(e) => patch({ jellyfin: { device: e.target.value || null } })}
              className="max-w-sm"
            >
              <option value="">Any device</option>
              {[...new Set([...(devices.data ?? []), ...(cfg.jellyfin.device ? [cfg.jellyfin.device] : [])])].map((d) => (
                <option key={d} value={d}>{d}</option>
              ))}
            </Select>
          </Field>
        </div>
      )}
    </Step>
  );
}

/* ------------------------------------------------------------- Libraries */

const kindIcon = { movies: Film, tvshows: Tv, mixed: Library } as Record<string, typeof Film>;

function LibrariesStep({ done, locked }: { done: boolean; locked: boolean }) {
  const { cfg, patch } = useApp();
  const [open, setOpen] = useState(!done);
  const jf = useLoad(() => (locked ? Promise.resolve([]) : api.jellyfinLibraries()), [locked, cfg.jellyfin.url]);
  const stats = useLoad(api.libraries, [cfg.libraries]);
  const [rows, setRows] = useState<Record<string, { on: boolean; path: string; name: string }>>({});
  const [picking, setPicking] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();

  // One row per Jellyfin folder; followed ones come from the config.
  useEffect(() => {
    const next: typeof rows = {};
    for (const l of jf.data ?? []) {
      for (const loc of l.locations) {
        const saved = cfg.libraries.find((c) => c.jellyfin === loc);
        next[loc] = { on: !!saved, path: saved?.path ?? loc, name: l.name };
      }
    }
    setRows(next);
  }, [jf.data]);

  const save = async () => {
    setSaving(true);
    setError(undefined);
    try {
      const libraries: Lib[] = Object.entries(rows)
        .filter(([, r]) => r.on)
        .map(([jellyfin, r]) => ({ name: r.name, jellyfin, path: r.path.trim() }));
      await patch({ libraries });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const followed = cfg.libraries.map((l) => l.name);
  const dirty = JSON.stringify(
    Object.entries(rows).filter(([, r]) => r.on).map(([j, r]) => [j, r.path.trim()]),
  ) !== JSON.stringify(cfg.libraries.map((l) => [l.jellyfin, l.path]));

  return (
    <Step
      n={4}
      title="Libraries"
      done={done}
      locked={locked}
      open={open}
      onToggle={() => setOpen(!open)}
      summary={done ? [...new Set(followed)].join(", ") : "Pick the films and shows to analyze"}
    >
      <p className="mb-4 text-sm text-zinc-500">
        Jellyfin and lueur may see the same files under different paths (different containers or machines). For each
        folder, choose where lueur finds it.
      </p>
      {jf.error && <Alert>{jf.error}</Alert>}
      {jf.loading && !jf.data && <Loader2 className="size-4 animate-spin text-zinc-500" />}
      {jf.data && !jf.data.length && <p className="text-sm text-zinc-500">No movie or show library on this Jellyfin server.</p>}

      <ul className="space-y-2">
        {jf.data?.flatMap((l) =>
          l.locations.map((loc) => {
            const r = rows[loc];
            if (!r) return null;
            const Icon = kindIcon[l.kind] ?? Library;
            const st = stats.data?.find((s) => s.jellyfin === loc && s.path === r.path.trim());
            return (
              <li
                key={loc}
                className={cx(
                  "rounded-xl border p-4 transition",
                  r.on ? "border-white/10 bg-white/[0.03]" : "border-white/[0.05]",
                )}
              >
                <div className="flex items-center gap-3">
                  <Icon className="size-4 shrink-0 text-zinc-400" />
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium">{l.name}</p>
                    <p className="truncate font-mono text-xs text-zinc-500" title={loc}>
                      <Server className="mr-1 inline size-3 align-[-1px]" />
                      Jellyfin: {loc}
                    </p>
                  </div>
                  <Toggle label={`Follow ${l.name}`} checked={r.on} onChange={(on) => setRows({ ...rows, [loc]: { ...r, on } })} />
                </div>
                {r.on && (
                  <div className="mt-3 flex items-center gap-2 pl-7">
                    <Input
                      value={r.path}
                      onChange={(e) => setRows({ ...rows, [loc]: { ...r, path: e.target.value } })}
                      className="font-mono text-xs"
                      spellCheck={false}
                      aria-label={`Folder of ${l.name} for lueur`}
                    />
                    <Button onClick={() => setPicking(loc)} icon={<FolderOpen className="size-4" />}>
                      Browse
                    </Button>
                  </div>
                )}
                {r.on && st && (
                  <p className={cx("mt-2 pl-7 text-xs", st.exists ? "text-zinc-500" : "text-rose-300")}>
                    {st.exists ? `${st.videos} videos found, ${st.analyzed} analyzed` : "lueur can't see this folder"}
                  </p>
                )}
              </li>
            );
          }),
        )}
      </ul>

      {jf.data && jf.data.length > 0 && (
        <div className="mt-5 flex flex-wrap items-center justify-between gap-4 border-t border-white/[0.06] pt-5">
          <p className="text-xs text-zinc-500">Automatic scans and analysis are set in Library.</p>
          <Button variant="primary" onClick={save} loading={saving} disabled={!dirty}>
            Save libraries
          </Button>
        </div>
      )}
      {error && <div className="mt-4"><Alert>{error}</Alert></div>}

      {picking && rows[picking] && (
        <FolderPicker
          start={rows[picking].path}
          onClose={() => setPicking(undefined)}
          onPick={(path) => setRows({ ...rows, [picking]: { ...rows[picking], path } })}
        />
      )}
    </Step>
  );
}

function Choice({
  selected,
  onClick,
  className,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && e.target === e.currentTarget && (e.preventDefault(), onClick())}
      aria-pressed={selected}
      className={cx(
        "flex cursor-pointer items-center gap-3 rounded-xl border p-3.5 text-left transition outline-none focus-visible:ring-2 focus-visible:ring-glow-1/60",
        selected
          ? "border-glow-1/50 bg-glow-1/[0.06] ring-4 ring-glow-1/[0.07]"
          : "border-white/[0.08] bg-white/[0.02] hover:border-white/15 hover:bg-white/[0.04]",
        className,
      )}
    >
      {children}
    </div>
  );
}
