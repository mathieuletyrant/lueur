import { ArrowRight, Sparkles } from "lucide-react";
import { setupSteps, useApp } from "../../context";
import { Alert, Button, Card } from "../../ui";
import { AreaCard } from "./AreaCard";
import { NowPlaying } from "./NowPlaying";
import { QueueCard } from "./QueueCard";
import { SyncCard } from "./SyncCard";

export function NowView() {
  const { cfg, status, go } = useApp();
  const steps = setupSteps(cfg, status);
  const done = Object.values(steps).filter(Boolean).length;
  const p = status?.player;

  return (
    <div className="space-y-5">
      {done < 4 && (
        <Card className="flex flex-wrap items-center justify-between gap-4 px-5 py-4">
          <div className="flex items-center gap-3">
            <div className="grid size-9 place-items-center rounded-xl bg-gradient-to-br from-glow-1/20 to-glow-2/20 text-glow-1 ring-1 ring-glow-1/20">
              <Sparkles className="size-4" />
            </div>
            <div>
              <p className="text-sm font-medium">Finish setting up lueur</p>
              <p className="text-sm text-zinc-500">{done} of 4 steps done</p>
            </div>
          </div>
          <Button variant="primary" onClick={() => go("setup")}>
            Continue setup <ArrowRight className="size-4" />
          </Button>
        </Card>
      )}

      {done === 4 && p?.error && <Alert>{p.error}</Alert>}
      {p?.jellyfin_error && <Alert tone="amber">Jellyfin: {p.jellyfin_error}</Alert>}

      <NowPlaying />
      <SyncCard />

      <div className="grid gap-5 lg:grid-cols-5">
        <AreaCard />
        <QueueCard />
      </div>
    </div>
  );
}
