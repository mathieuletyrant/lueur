import { useState } from "react";
import { setupSteps, useApp } from "../../context";
import { PageHeader, Segmented } from "../../ui";
import { AnalysisSettings } from "./AnalysisSettings";
import { AreaStep } from "./AreaStep";
import { BridgeStep } from "./BridgeStep";
import { JellyfinStep } from "./JellyfinStep";
import { LibrariesStep } from "./LibrariesStep";
import { WledStep } from "./WledStep";

type Lights = "hue" | "wled";

export function SettingsView() {
  const { cfg, status, patch } = useApp();
  const s = setupSteps(cfg, status);
  const [lights, setLights] = useState<Lights>(cfg.wled.host ? "wled" : "hue");

  // Back to Hue: the Strip's Layout stays saved, only its controller is forgotten.
  const choose = (next: Lights) => {
    setLights(next);
    if (next === "hue" && cfg.wled.host) patch({ wled: { host: "" } });
  };

  return (
    <div>
      <PageHeader
        title="Settings"
        subtitle="Connect your lights and Jellyfin, tell lueur where your films are, and how to analyze them."
      />
      <div className="mb-4 flex flex-wrap items-center gap-3">
        <span className="text-sm text-zinc-400">Lights</span>
        <div className="w-72">
          <Segmented
            label="Lights"
            value={lights}
            onChange={choose}
            options={[
              { value: "hue", label: "Philips Hue" },
              { value: "wled", label: "WLED strip" },
            ]}
          />
        </div>
      </div>
      <ol className="space-y-4">
        {lights === "wled" ? (
          <WledStep done={!!cfg.wled.host} />
        ) : (
          <>
            <BridgeStep done={s.bridge} />
            <AreaStep done={s.area} locked={!s.bridge} />
          </>
        )}
        <JellyfinStep n={lights === "wled" ? 2 : 3} done={s.jellyfin} />
        <LibrariesStep n={lights === "wled" ? 3 : 4} done={s.libraries} locked={!s.jellyfin} />
      </ol>
      <div className="mt-8">
        <AnalysisSettings />
      </div>
    </div>
  );
}
