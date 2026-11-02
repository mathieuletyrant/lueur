import { Clapperboard, Library, Settings2, SlidersHorizontal } from "lucide-react";
import { createContext, useContext } from "react";
import { type Config, type Status } from "./api";

export type Ctx = {
  cfg: Config;
  patch: (p: unknown) => Promise<Config>;
  reloadConfig: () => void;
  status?: Status;
  go: (v: View) => void;
};

export const AppCtx = createContext<Ctx>(null!);

export const useApp = () => useContext(AppCtx);

export const views = {
  now: { label: "Now", icon: Clapperboard },
  library: { label: "Library", icon: Library },
  look: { label: "Look", icon: SlidersHorizontal },
  setup: { label: "Settings", icon: Settings2 },
} as const;

export type View = keyof typeof views;

/** Setup steps done, in onboarding order. */
export function setupSteps(cfg: Config, status?: Status) {
  return {
    bridge: cfg.paired,
    area: cfg.paired && !!(cfg.hue.area || status?.player.area),
    jellyfin: !!cfg.jellyfin.url && cfg.has_api_key,
    libraries: cfg.libraries.length > 0,
  };
}
