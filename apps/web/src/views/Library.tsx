import { useEffect, useMemo, useRef, useState } from "react";
import { AlertCircle, CheckCircle2, Clock, Film, Library, Loader2, RotateCcw, ScanLine, Search, Settings2, Square } from "lucide-react";
import { api, useLoad, type Video } from "../api";
import { useApp } from "../App";
import { analysisSummary, hh, toLocal } from "./Analysis";
import { Alert, Button, Card, CardHeader, Empty, Input, PageHeader, Pill, Progress, RunningAnalysis, Toggle } from "../ui";

export function LibraryView() {
  const { cfg, status, go } = useApp();
  const libs = useLoad(api.libraries);
  const [scanning, setScanning] = useState(false);
  const [note, setNote] = useState<string>();
  const [error, setError] = useState<string>();
  const q = status?.queue;
  const busy = !!q && (q.running.length > 0 || q.pending > 0);
  const openSettings = () => {
    go("setup");
    setTimeout(() => document.getElementById("analysis")?.scrollIntoView({ behavior: "smooth" }), 50);
  };

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

      <div className="mt-8">
        <Card>
          <CardHeader
            title="Queue"
            subtitle={
              q
                ? q.waiting_for_window && !q.running.length && cfg.analysis.window
                  ? `${q.pending} waiting for ${hh(toLocal(cfg.analysis.window.start))}`
                  : `${q.running.length} running · ${q.pending} waiting · ${q.done} done`
                : undefined
            }
            action={
              <Button variant="ghost" className="h-8 px-2.5 text-xs" onClick={openSettings} icon={<Settings2 className="size-3.5" />}>
                {analysisSummary(cfg)}
              </Button>
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
              <p className="py-2 text-sm text-zinc-500">
                Nothing in the queue. “Analyze missing” queues every film and episode without a color track.
              </p>
            )}
          </div>
        </Card>
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
