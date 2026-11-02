import { Library, ScanLine, Square } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, useLoad } from "../../api";
import { useApp } from "../../context";
import { Alert, Button, Card, Empty, PageHeader } from "../../ui";
import { AnalysisQueue, FailedAnalyses } from "./AnalysisQueue";
import { LibraryCards } from "./LibraryCards";
import { MediaList } from "./MediaList";

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
        <LibraryCards stats={libs.data} loading={libs.loading} />
      )}

      <AnalysisQueue onSettings={openSettings} />

      <FailedAnalyses />

      {cfg.libraries.length > 0 && <MediaList />}
    </div>
  );
}
