import { useEffect, useMemo, useRef, useState } from "react";
import { AlertCircle, CheckCircle2, Clock, Film, FolderSearch, Library, Loader2, RotateCcw, ScanLine, Search, Square } from "lucide-react";
import { api, useLoad, type Video } from "../api";
import { useApp } from "../App";
import { Alert, Button, Card, CardHeader, cx, Empty, Input, PageHeader, Pill, Progress, RunningAnalysis, SwitchRow, Toggle } from "../ui";

export function LibraryView() {
  const { cfg, status, patch, go } = useApp();
  const libs = useLoad(api.libraries);
  const [scanning, setScanning] = useState(false);
  const [note, setNote] = useState<string>();
  const [error, setError] = useState<string>();
  const q = status?.queue;
  const busy = !!q && (q.running.length > 0 || q.pending > 0);

  // Refresh the counts each time an Analysis finishes.
  const done = useRef(q?.done);
  useEffect(() => {
    if (q && done.current !== undefined && q.done !== done.current) libs.reload();
    done.current = q?.done;
  }, [q?.done]);

  const scan = async () => {
    setScanning(true);
    setError(undefined);
    try {
      const r = await api.scan();
      setNote(r.queued ? `${r.queued} file${r.queued > 1 ? "s" : ""} queued for analysis` : "Nothing new to analyze");
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setScanning(false);
    }
  };

  return (
    <div>
      <PageHeader
        title="Library"
        subtitle="Films and shows lueur has analyzed, and the ones still to do."
        action={
          <div className="flex gap-2">
            {busy && (
              <Button onClick={() => api.stop().catch((e: Error) => setError(e.message))} icon={<Square className="size-3.5" />}>
                Stop
              </Button>
            )}
            <Button variant="primary" onClick={scan} loading={scanning} disabled={!cfg.libraries.length} icon={<ScanLine className="size-4" />}>
              Analyze missing
            </Button>
          </div>
        }
      />

      {error && <div className="mb-4"><Alert>{error}</Alert></div>}
      {note && <p className="mb-4 text-sm text-zinc-400">{note}</p>}

      {!cfg.libraries.length ? (
        <Card>
          <Empty icon={<Library className="size-4" />} title="No library yet">
            Pick your Jellyfin libraries in Setup, then come back to scan them.
            <div className="mt-4">
              <Button onClick={() => go("setup")}>Open setup</Button>
            </div>
          </Empty>
        </Card>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          {(libs.data ?? cfg.libraries.map((l) => ({ ...l, exists: true, videos: 0, analyzed: 0 }))).map((l) => {
            const pct = l.videos ? l.analyzed / l.videos : 0;
            return (
              <Card key={l.jellyfin} className="p-5">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h3 className="truncate text-sm font-semibold">{l.name}</h3>
                    <p className="mt-0.5 truncate font-mono text-xs text-zinc-500" title={l.path}>{l.path}</p>
                  </div>
                  {libs.loading && !libs.data ? (
                    <Loader2 className="size-4 animate-spin text-zinc-500" />
                  ) : !l.exists ? (
                    <AlertCircle className="size-4 text-rose-400" aria-label="Folder not found" />
                  ) : pct === 1 ? (
                    <CheckCircle2 className="size-4 text-emerald-400" aria-label="All analyzed" />
                  ) : null}
                </div>
                {l.exists ? (
                  <>
                    <div className="mt-5 flex items-baseline gap-1.5">
                      <span className="text-2xl font-semibold tabular-nums">{l.analyzed}</span>
                      <span className="text-sm text-zinc-500">/ {l.videos} analyzed</span>
                    </div>
                    <Progress value={pct} className="mt-3" />
                  </>
                ) : (
                  <p className="mt-5 text-sm text-rose-300">
                    lueur can't see this folder. Check the volume mount, or pick another folder in Setup.
                  </p>
                )}
              </Card>
            );
          })}
        </div>
      )}

      <div className="mt-8 grid gap-5 lg:grid-cols-5">
        <Card className="lg:col-span-3">
          <CardHeader
            title="Queue"
            subtitle={q ? `${q.running.length} running · ${q.pending} waiting · ${q.done} done` : undefined}
            action={
              <div className="flex items-center gap-2 text-xs text-zinc-500">
                Parallel
                <div className="flex rounded-lg border border-white/10 p-0.5">
                  {[1, 2, 3, 4].map((n) => (
                    <button
                      key={n}
                      onClick={() => patch({ jobs: n })}
                      className={cx(
                        "h-6 w-7 rounded-md text-xs font-medium tabular-nums transition",
                        (cfg.jobs || 1) === n ? "bg-white/10 text-zinc-50" : "text-zinc-500 hover:text-zinc-200",
                      )}
                    >
                      {n}
                    </button>
                  ))}
                </div>
              </div>
            }
          />
          <div className="px-5 pt-3 pb-5">
            {q && (q.running.length || q.next.length) ? (
              <ul className="divide-y divide-white/[0.05]">
                {q.running.map((r) => (
                  <li key={r.name} className="py-3">
                    <RunningAnalysis {...r} />
                  </li>
                ))}
                {q.next.map((n) => (
                  <li key={n} className="flex items-center gap-2 py-3 text-sm text-zinc-400">
                    <Clock className="size-3.5 shrink-0 text-zinc-600" />
                    <span className="truncate">{n}</span>
                  </li>
                ))}
                {q.pending > q.next.length && (
                  <li className="py-3 text-xs text-zinc-500">and {q.pending - q.next.length} more</li>
                )}
              </ul>
            ) : (
              <Empty icon={<FolderSearch className="size-4" />} title="Nothing in the queue">
                “Analyze missing” queues every film and episode without a color track.
              </Empty>
            )}
          </div>
        </Card>

        <Automation />
      </div>

      {!!q?.failed.length && (
        <Card className="mt-5">
          <CardHeader title="Failed" subtitle={`${q.failed.length} file(s)`} />
          <ul className="grid gap-3 px-5 pt-3 pb-5 sm:grid-cols-2">
            {q.failed.map((f, i) => (
              <li key={i} className="rounded-lg border border-rose-400/10 bg-rose-400/[0.04] px-3 py-2.5">
                <p className="truncate text-sm text-zinc-200">{f.name}</p>
                <p className="mt-0.5 line-clamp-2 font-mono text-xs text-rose-300/80" title={f.error}>{f.error}</p>
              </li>
            ))}
          </ul>
        </Card>
      )}

      {cfg.libraries.length > 0 && <Media />}
    </div>
  );
}

const INTERVALS = [
  { m: 0, label: "Off" },
  { m: 5, label: "5 min" },
  { m: 15, label: "15 min" },
  { m: 60, label: "1 h" },
  { m: 360, label: "6 h" },
];

function ago(unix: number) {
  const s = Math.max(0, Date.now() / 1000 - unix);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  return `${Math.floor(s / 3600)} h ago`;
}

/** When lueur analyzes on its own. */
function Automation() {
  const { cfg, status, patch } = useApp();
  const last = status?.last_scan;
  return (
    <Card className="lg:col-span-2">
      <CardHeader title="Automation" subtitle="New films get their track without you asking." />
      <div className="space-y-5 px-5 pt-4 pb-5">
        <div>
          <p className="text-sm font-medium">Auto scan</p>
          <p className="mt-0.5 text-xs text-zinc-500">Looks for new videos in your libraries.</p>
          <div className="mt-3 flex rounded-lg border border-white/10 p-0.5" role="radiogroup" aria-label="Auto scan interval">
            {INTERVALS.map(({ m, label }) => (
              <button
                key={m}
                role="radio"
                aria-checked={cfg.auto_scan_minutes === m}
                onClick={() => patch({ auto_scan_minutes: m })}
                className={cx(
                  "h-7 flex-1 rounded-md text-xs font-medium whitespace-nowrap transition",
                  cfg.auto_scan_minutes === m ? "bg-white/10 text-zinc-50" : "text-zinc-500 hover:text-zinc-200",
                )}
              >
                {label}
              </button>
            ))}
          </div>
          {last && (
            <p className="mt-2 text-xs text-zinc-500">
              Last {last.auto ? "auto " : ""}scan {ago(last.at)}: {last.queued} queued
              {last.removed > 0 && `, ${last.removed} old track${last.removed > 1 ? "s" : ""} removed`}
            </p>
          )}
        </div>
        <Exclude />
        <SwitchRow
          className="border-t border-white/[0.06] pt-5"
          label="Analyze on first playback"
          hint="A film played without a track is analyzed first; the lights join in when it is done."
          checked={cfg.auto_analyze}
          onChange={(v) => patch({ auto_analyze: v })}
        />
      </div>
    </Card>
  );
}

/** Keywords that keep files (trailers, samples, extras…) out of every analysis. */
function Exclude() {
  const { cfg, patch } = useApp();
  const [draft, setDraft] = useState("");
  const save = (exclude: string[]) => patch({ exclude });
  const add = () => {
    const k = draft.trim();
    if (k && !cfg.exclude.includes(k)) save([...cfg.exclude, k]);
    setDraft("");
  };
  return (
    <div className="border-t border-white/[0.06] pt-5">
      <p className="text-sm font-medium">Skip files containing</p>
      <p className="mt-0.5 text-xs text-zinc-500">Matched anywhere in the path, ignoring case. Their tracks are removed.</p>
      <ul className="mt-3 flex flex-wrap gap-1.5">
        {cfg.exclude.map((k) => (
          <li key={k} className="flex items-center gap-1 rounded-md bg-white/[0.06] py-0.5 pr-1 pl-2 font-mono text-xs text-zinc-300 ring-1 ring-white/10 ring-inset">
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

const SHOWN = 25;

/** Every film and episode, to analyze one right away. */
function Media() {
  const { status } = useApp();
  const q = status?.queue;
  const videos = useLoad(api.videos);
  const [query, setQuery] = useState("");
  const [missing, setMissing] = useState(false);
  const [shown, setShown] = useState(SHOWN);
  const [error, setError] = useState<string>();

  // Follow the queue: a film starts, finishes or gets dropped.
  const key = `${q?.done}|${q?.pending}|${q?.running.map((r) => r.name).join()}`;
  const first = useRef(true);
  useEffect(() => {
    if (first.current) first.current = false;
    else videos.reload();
  }, [key]);

  const list = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return (videos.data ?? []).filter(
      (v) => (!missing || !v.analyzed) && words.every((w) => `${v.name} ${v.folder} ${v.library}`.toLowerCase().includes(w)),
    );
  }, [videos.data, query, missing]);

  const analyze = async (v: Video) => {
    setError(undefined);
    videos.setData(videos.data?.map((x) => (x.path === v.path ? { ...x, state: "queued" } : x)));
    try {
      await api.analyze(v.path);
    } catch (e) {
      setError((e as Error).message);
      videos.reload();
    }
  };

  const total = videos.data?.length ?? 0;
  const todo = videos.data?.filter((v) => !v.analyzed).length ?? 0;

  return (
    <Card className="mt-8">
      <CardHeader
        title="Films and episodes"
        subtitle={videos.data ? `${total} files · ${todo} without a track` : "Loading…"}
        action={
          <label className="flex items-center gap-2 text-xs text-zinc-400">
            Missing only
            <Toggle label="Missing only" checked={missing} onChange={setMissing} />
          </label>
        }
      />
      <div className="px-5 pt-4 pb-5">
        <div className="relative">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-zinc-500" />
          <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search a film, a show, a season…" className="pl-9" />
        </div>
        {error && <div className="mt-3"><Alert>{error}</Alert></div>}
        {videos.error && <div className="mt-3"><Alert>{videos.error}</Alert></div>}

        <ul className="mt-3 divide-y divide-white/[0.05]">
          {list.slice(0, shown).map((v) => {
            const running = q?.running.find((r) => v.state === "running" && v.path.endsWith(r.name));
            return (
              <li key={v.path} className="flex items-center gap-3 py-2.5">
                <Film className="size-4 shrink-0 text-zinc-500" />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm text-zinc-100">{v.name}</p>
                  <p className="truncate text-xs text-zinc-500">
                    {v.library}
                    {v.folder && ` · ${v.folder}`}
                  </p>
                </div>
                {v.state === "running" ? (
                  <span className="flex w-28 items-center gap-2">
                    <Progress value={running?.progress ?? null} className="flex-1" />
                    {running?.progress != null && <span className="text-xs text-zinc-500 tabular-nums">{Math.round(running.progress * 100)}%</span>}
                  </span>
                ) : v.state === "queued" ? (
                  <Pill tone="blue">Queued</Pill>
                ) : v.analyzed ? (
                  <>
                    <Pill tone="green">Analyzed</Pill>
                    <Button variant="ghost" className="h-7 px-2" onClick={() => analyze(v)} aria-label={`Analyze ${v.name} again`} title="Analyze again">
                      <RotateCcw className="size-3.5" />
                    </Button>
                  </>
                ) : (
                  <Button className="h-7 px-2.5 text-xs" onClick={() => analyze(v)}>
                    Analyze
                  </Button>
                )}
              </li>
            );
          })}
        </ul>
        {videos.loading && !videos.data && <Loader2 className="mx-auto mt-4 size-4 animate-spin text-zinc-500" />}
        {videos.data && list.length === 0 && <p className="py-6 text-center text-sm text-zinc-500">No match</p>}
        {list.length > shown && (
          <div className="flex items-center justify-between gap-3 pt-3">
            <span className="text-xs text-zinc-500">{list.length - shown} more</span>
            <Button variant="ghost" className="h-7 px-2.5 text-xs" onClick={() => setShown(shown + 50)}>
              Show more
            </Button>
          </div>
        )}
      </div>
    </Card>
  );
}
