import { Clock, Settings2 } from "lucide-react";
import { useApp } from "../../context";
import { hh, toLocal } from "../../lib/hours";
import { Button, Card, CardHeader, RunningAnalysis } from "../../ui";
import { analysisSummary } from "../settings/AnalysisSettings";

/** What is being analyzed and what waits, with how lueur analyzes (links to Settings). */
export function AnalysisQueue({ onSettings }: { onSettings: () => void }) {
  const { cfg, status } = useApp();
  const q = status?.queue;
  const window = cfg.analysis.window;
  return (
    <Card className="mt-8">
      <CardHeader
        title="Queue"
        subtitle={
          q &&
          (q.waiting_for_window && !q.running.length && window
            ? `${q.pending} waiting for ${hh(toLocal(window.start))}`
            : `${q.running.length} running · ${q.pending} waiting · ${q.done} done`)
        }
        action={
          <Button
            variant="ghost"
            className="h-8 px-2.5 text-xs"
            onClick={onSettings}
            icon={<Settings2 className="size-3.5" />}
            title={analysisSummary(cfg)}
            aria-label={`Analysis settings: ${analysisSummary(cfg)}`}
          >
            {/* Phones: just the icon, the queue's own line needs the room. */}
            <span className="hidden sm:inline">{analysisSummary(cfg)}</span>
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
            {q.pending > q.next.length && <li className="py-3 text-xs text-zinc-500">and {q.pending - q.next.length} more</li>}
          </ul>
        ) : (
          <p className="py-2 text-sm text-zinc-500">
            Nothing in the queue. “Analyze missing” queues every film and episode without a color track.
          </p>
        )}
      </div>
    </Card>
  );
}

/** Analyses that failed this session, with ffmpeg's reason. */
export function FailedAnalyses() {
  const failed = useApp().status?.queue.failed;
  if (!failed?.length) return null;
  return (
    <Card className="mt-5">
      <CardHeader title="Failed" subtitle={`${failed.length} file(s)`} />
      <ul className="grid gap-3 px-5 pt-3 pb-5 sm:grid-cols-2">
        {failed.map((f, i) => (
          <li key={i} className="rounded-lg border border-rose-400/10 bg-rose-400/[0.04] px-3 py-2.5">
            <p className="truncate text-sm text-zinc-200">{f.name}</p>
            <p className="mt-0.5 line-clamp-2 font-mono text-xs text-rose-300/80" title={f.error}>
              {f.error}
            </p>
          </li>
        ))}
      </ul>
    </Card>
  );
}
