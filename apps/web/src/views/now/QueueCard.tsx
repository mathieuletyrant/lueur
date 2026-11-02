import { ArrowRight, Clapperboard } from "lucide-react";
import { useApp } from "../../context";
import { Button, Card, CardHeader, Empty, RunningAnalysis } from "../../ui";

export function QueueCard() {
  const { status, go } = useApp();
  const q = status?.queue;
  const busy = !!q && (q.running.length > 0 || q.pending > 0);

  return (
    <Card className="flex flex-col lg:col-span-2">
      <CardHeader
        title="Analysis"
        subtitle={busy ? `${q!.pending} waiting` : "Queue is empty"}
        action={
          <Button variant="ghost" onClick={() => go("library")}>
            Library <ArrowRight className="size-4" />
          </Button>
        }
      />
      <div className="flex-1 px-5 pt-3 pb-5">
        {busy ? (
          <ul className="space-y-4">
            {q!.running.map((r) => (
              <li key={r.name}>
                <RunningAnalysis {...r} />
              </li>
            ))}
          </ul>
        ) : (
          <Empty icon={<Clapperboard className="size-4" />} title={q?.done ? `${q.done} analyzed this session` : "All caught up"}>
            Scan your libraries to analyze new films.
          </Empty>
        )}
      </div>
    </Card>
  );
}
