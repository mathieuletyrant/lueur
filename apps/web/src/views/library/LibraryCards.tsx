import { AlertCircle, CheckCircle2, Loader2 } from "lucide-react";
import type { LibraryStats } from "../../api";
import { useApp } from "../../context";
import { Card, Progress } from "../../ui";

/** One card per Library: how many of its videos have a Track. */
export function LibraryCards({ stats, loading }: { stats?: LibraryStats[]; loading: boolean }) {
  const { cfg } = useApp();
  return (
    <div className="grid gap-4 sm:grid-cols-2">
      {(stats ?? cfg.libraries.map((l) => ({ ...l, exists: true, videos: 0, analyzed: 0 }))).map((l) => {
        const pct = l.videos ? l.analyzed / l.videos : 0;
        return (
          <Card key={l.jellyfin} className="p-5">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h3 className="truncate text-sm font-semibold">{l.name}</h3>
                <p className="mt-0.5 truncate font-mono text-xs text-zinc-500" title={l.path}>{l.path}</p>
              </div>
              {loading && !stats ? (
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
  );
}
