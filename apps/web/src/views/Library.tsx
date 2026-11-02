import { useEffect, useRef, useState } from "react";
import { AlertCircle, CheckCircle2, Clock, FolderSearch, Library, Loader2, ScanLine } from "lucide-react";
import { api, useLoad } from "../api";
import { useApp } from "../App";
import { Alert, Button, Card, CardHeader, cx, Empty, PageHeader, Progress } from "../ui";

export function LibraryView() {
  const { cfg, status, patch, go } = useApp();
  const libs = useLoad(api.libraries);
  const [scanning, setScanning] = useState(false);
  const [note, setNote] = useState<string>();
  const [error, setError] = useState<string>();
  const q = status?.queue;

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
      setNote(r.queued ? `${r.queued} file${r.queued > 1 ? "s" : ""} queued for analysis` : "Everything is already analyzed");
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
          <Button variant="primary" onClick={scan} loading={scanning} disabled={!cfg.libraries.length} icon={<ScanLine className="size-4" />}>
            Scan now
          </Button>
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
                    <div className="mb-2 flex items-center justify-between gap-3 text-sm">
                      <span className="flex min-w-0 items-center gap-2">
                        <Loader2 className="size-3.5 shrink-0 animate-spin text-glow-1" />
                        <span className="truncate">{r.name}</span>
                      </span>
                      <span className="text-xs text-zinc-500 tabular-nums">
                        {r.progress !== null ? `${Math.round(r.progress * 100)}%` : "…"}
                      </span>
                    </div>
                    <Progress value={r.progress} />
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
                “Scan now” looks for films and episodes without a color track.
              </Empty>
            )}
          </div>
        </Card>

        <Card className="lg:col-span-2">
          <CardHeader title="Failed" subtitle={q?.failed.length ? `${q.failed.length} file(s)` : "No failures"} />
          <div className="px-5 pt-3 pb-5">
            {q?.failed.length ? (
              <ul className="space-y-3">
                {q.failed.map((f, i) => (
                  <li key={i} className="rounded-lg border border-rose-400/10 bg-rose-400/[0.04] px-3 py-2.5">
                    <p className="truncate text-sm text-zinc-200">{f.name}</p>
                    <p className="mt-0.5 line-clamp-2 font-mono text-xs text-rose-300/80" title={f.error}>{f.error}</p>
                  </li>
                ))}
              </ul>
            ) : (
              <Empty icon={<CheckCircle2 className="size-4" />} title="All good" />
            )}
          </div>
        </Card>
      </div>
    </div>
  );
}
