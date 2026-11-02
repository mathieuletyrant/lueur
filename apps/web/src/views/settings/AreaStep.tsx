import { Loader2 } from "lucide-react";
import { useState } from "react";
import { api, useLoad } from "../../api";
import { AreaView } from "../../components/AreaView";
import { useApp } from "../../context";
import { Alert, Pill } from "../../ui";
import { Choice, Step } from "./Step";

export function AreaStep({ done, locked }: { done: boolean; locked: boolean }) {
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
