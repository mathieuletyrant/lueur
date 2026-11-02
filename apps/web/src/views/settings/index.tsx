import { setupSteps, useApp } from "../../context";
import { PageHeader } from "../../ui";
import { AnalysisSettings } from "./AnalysisSettings";
import { AreaStep } from "./AreaStep";
import { BridgeStep } from "./BridgeStep";
import { JellyfinStep } from "./JellyfinStep";
import { LibrariesStep } from "./LibrariesStep";

export function SettingsView() {
  const { cfg, status } = useApp();
  const s = setupSteps(cfg, status);
  return (
    <div>
      <PageHeader title="Settings" subtitle="Connect your Hue bridge and Jellyfin, tell lueur where your films are, and how to analyze them." />
      <ol className="space-y-4">
        <BridgeStep done={s.bridge} />
        <AreaStep done={s.area} locked={!s.bridge} />
        <JellyfinStep done={s.jellyfin} />
        <LibrariesStep done={s.libraries} locked={!s.jellyfin} />
      </ol>
      <div className="mt-8">
        <AnalysisSettings />
      </div>
    </div>
  );
}
