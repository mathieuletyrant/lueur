import { CheckCircle2, Film, Loader2, RotateCcw, Search } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, useLoad, type Video } from "../../api";
import { Barcode } from "../../components/Barcode";
import { useApp } from "../../context";
import { Alert, Button, Card, CardHeader, Input, Pill, Progress, Toggle } from "../../ui";

const SHOWN = 25;

/** Every film and episode, to analyze one right away. */
export function MediaList() {
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
              <li key={v.path} className="flex items-center gap-3 py-3 sm:gap-4">
                {/* A still from the film (15 % in); a plain slate until it is analyzed. */}
                <div className="grid aspect-video w-20 shrink-0 place-items-center overflow-hidden rounded-md bg-zinc-900 ring-1 ring-white/10 sm:w-32">
                  {v.duration != null ? (
                    <img
                      src={api.frameUrl(v.path, v.duration * 0.15, 320)}
                      alt=""
                      loading="lazy"
                      className="size-full object-cover [animation:fade-in_.6s_ease]"
                    />
                  ) : (
                    <Film className="size-4 text-zinc-600" />
                  )}
                </div>
                <div className="min-w-0 flex-1">
                  <p className="line-clamp-2 text-sm leading-snug break-words text-zinc-100 sm:truncate" title={v.name}>
                    {v.name}
                  </p>
                  <p className="truncate text-xs text-zinc-500">
                    {v.library}
                    {v.folder && ` · ${v.folder}`}
                  </p>
                  {v.duration != null && <Barcode path={v.path} duration={v.duration} className="mt-1.5 max-w-md" />}
                </div>
                {v.state === "running" ? (
                  <span className="flex w-16 shrink-0 items-center gap-2 sm:w-28">
                    <Progress value={running?.progress ?? null} className="flex-1" />
                    {running?.progress != null && <span className="text-xs text-zinc-500 tabular-nums">{Math.round(running.progress * 100)}%</span>}
                  </span>
                ) : v.state === "queued" ? (
                  <span className="shrink-0">
                    <Pill tone="blue">Queued</Pill>
                  </span>
                ) : v.analyzed ? (
                  <span className="flex shrink-0 items-center gap-1">
                    {/* Phones: a check instead of the pill, so the title gets the room. */}
                    <span className="hidden sm:inline-flex">
                      <Pill tone="green">Analyzed</Pill>
                    </span>
                    <CheckCircle2 className="size-4 text-emerald-400 sm:hidden" aria-label="Analyzed" />
                    <Button variant="ghost" className="h-7 px-2" onClick={() => analyze(v)} aria-label={`Analyze ${v.name} again`} title="Analyze again">
                      <RotateCcw className="size-3.5" />
                    </Button>
                  </span>
                ) : (
                  <Button className="h-7 shrink-0 px-2.5 text-xs" onClick={() => analyze(v)}>
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
