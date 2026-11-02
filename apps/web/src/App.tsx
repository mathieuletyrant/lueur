import { createContext, useContext, useEffect, useState } from "react";
import { Clapperboard, Library, Loader2, Settings2, SlidersHorizontal } from "lucide-react";
import { api, useLoad, usePoll, type Config, type Status } from "./api";
import { Alert, cx } from "./ui";
import { Now } from "./views/Now";
import { LibraryView } from "./views/Library";
import { LookView } from "./views/Look";
import { Setup } from "./views/Setup";

type Ctx = {
  cfg: Config;
  patch: (p: unknown) => Promise<Config>;
  reloadConfig: () => void;
  status?: Status;
  go: (v: View) => void;
};
const AppCtx = createContext<Ctx>(null!);
export const useApp = () => useContext(AppCtx);

const views = {
  now: { label: "Now", icon: Clapperboard },
  library: { label: "Library", icon: Library },
  look: { label: "Look", icon: SlidersHorizontal },
  setup: { label: "Settings", icon: Settings2 },
} as const;
export type View = keyof typeof views;

const fromHash = (): View => {
  const h = location.hash.slice(2);
  return h in views ? (h as View) : "now";
};

/** Setup steps done, in onboarding order. */
export function setupSteps(cfg: Config, status?: Status) {
  return {
    bridge: cfg.paired,
    area: cfg.paired && !!(cfg.hue.area || status?.player.area),
    jellyfin: !!cfg.jellyfin.url && cfg.has_api_key,
    libraries: cfg.libraries.length > 0,
  };
}

export default function App() {
  const [view, setView] = useState<View>(fromHash);
  const config = useLoad(api.config);
  const { data: status } = usePoll(api.status, view === "now" ? 400 : 1500);

  useEffect(() => {
    const on = () => setView(fromHash());
    addEventListener("hashchange", on);
    return () => removeEventListener("hashchange", on);
  }, []);

  const go = (v: View) => {
    location.hash = `/${v}`;
    scrollTo({ top: 0 });
  };

  if (!config.data) {
    return (
      <div className="grid min-h-dvh place-items-center p-4">
        {config.error ? (
          <Alert>lueur is unreachable: {config.error}</Alert>
        ) : (
          <Loader2 className="size-5 animate-spin text-zinc-500" />
        )}
      </div>
    );
  }

  const cfg = config.data;
  const steps = Object.values(setupSteps(cfg, status));
  const ctx: Ctx = {
    cfg,
    status,
    go,
    reloadConfig: config.reload,
    patch: async (p) => {
      const c = await api.patch(p);
      config.setData(c);
      return c;
    },
  };
  const p = status?.player;
  // One status for both services: quiet when all is well, words only when something needs you.
  const health: { label: string; tone: "live" | "ok" | "warn" | "bad"; detail?: string } = steps.some((s) => !s)
    ? { label: "Setup needed", tone: "warn" }
    : p?.error
      ? { label: "Bridge error", tone: "bad", detail: p.error }
      : p?.jellyfin_error
        ? { label: "Jellyfin unreachable", tone: "bad", detail: p.jellyfin_error }
        : p?.streaming
          ? { label: "Live", tone: "live" }
          : { label: "Ready", tone: "ok" };

  return (
    <AppCtx.Provider value={ctx}>
      <div className="pt-safe min-h-dvh">
        {/* One top bar: logo, then the tabs (desktop) and the health dots. On mobile the tabs move to the bottom. */}
        <header className="sticky top-0 z-30 border-b border-white/[0.06] bg-[#09090b]/85 backdrop-blur-xl">
          <div className="mx-auto flex h-14 max-w-5xl items-center gap-6 px-4 md:h-16 md:px-6">
            <a href="#/now" className="flex items-center gap-2.5">
              <Logo />
              <span className="text-[17px] font-semibold tracking-tight">lueur</span>
            </a>
            <a
              href={health.tone === "warn" || health.tone === "bad" ? "#/setup" : "#/now"}
              title={health.detail ?? "Bridge and Jellyfin connected"}
              className={cx(
                "-ml-2 flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium transition",
                health.tone === "live" && "text-emerald-300",
                health.tone === "ok" && "text-zinc-500 hover:text-zinc-300",
                health.tone === "warn" && "bg-amber-400/10 text-amber-300 ring-1 ring-amber-400/20 ring-inset",
                health.tone === "bad" && "bg-rose-400/10 text-rose-300 ring-1 ring-rose-400/20 ring-inset",
              )}
            >
              <span className="relative flex size-1.5">
                {health.tone === "live" && <span className="absolute inline-flex size-full animate-ping rounded-full bg-emerald-400 opacity-75" />}
                <span
                  className={cx(
                    "relative inline-flex size-1.5 rounded-full",
                    health.tone === "warn" ? "bg-amber-400" : health.tone === "bad" ? "bg-rose-400" : "bg-emerald-400",
                  )}
                />
              </span>
              {health.label}
            </a>
            <nav className="ml-auto hidden gap-1 md:flex" aria-label="Main">
              {(Object.keys(views) as View[]).map((k) => {
                const { label, icon: Icon } = views[k];
                const todo = k === "setup" && steps.some((s) => !s);
                return (
                  <a
                    key={k}
                    href={`#/${k}`}
                    aria-current={view === k ? "page" : undefined}
                    className={cx(
                      "relative flex items-center gap-2 rounded-full px-4 py-2 text-sm font-medium transition",
                      view === k ? "bg-white/[0.08] text-zinc-50" : "text-zinc-400 hover:text-zinc-100",
                    )}
                  >
                    <Icon className="size-4" />
                    {label}
                    {todo && <span className="absolute top-1.5 right-2 size-1.5 rounded-full bg-glow-1" aria-label="incomplete" />}
                  </a>
                );
              })}
            </nav>
          </div>
        </header>

        <main className="mx-auto w-full max-w-5xl px-4 pt-6 pb-28 md:px-6 md:pt-10 md:pb-12">
          {view === "now" && <Now />}
          {view === "library" && <LibraryView />}
          {view === "look" && <LookView />}
          {view === "setup" && <Setup />}
        </main>
      </div>

      <nav className="pb-safe fixed inset-x-0 bottom-0 z-30 border-t border-white/[0.06] bg-[#09090b]/90 backdrop-blur-xl md:hidden" aria-label="Main">
        <div className="mx-auto flex max-w-md">
          {(Object.keys(views) as View[]).map((k) => {
            const { label, icon: Icon } = views[k];
            const on = view === k;
            return (
              <a
                key={k}
                href={`#/${k}`}
                aria-current={on ? "page" : undefined}
                // Tapping the tab you're on scrolls back to the top, like any iOS app.
                onClick={() => on && scrollTo({ top: 0, behavior: "smooth" })}
                className={cx(
                  "group relative flex flex-1 flex-col items-center gap-0.5 pt-2 pb-1 text-[11px] font-semibold transition-colors",
                  on ? "text-zinc-50" : "text-zinc-500",
                )}
              >
                <Icon className="size-6 transition-transform group-active:scale-90" strokeWidth={on ? 2.2 : 1.7} />
                {label}
                {k === "setup" && steps.some((s) => !s) && (
                  <span className="absolute top-1.5 right-[calc(50%-18px)] size-1.5 rounded-full bg-glow-1" aria-label="incomplete" />
                )}
              </a>
            );
          })}
        </div>
      </nav>
    </AppCtx.Provider>
  );
}

function Logo() {
  return (
    <span className="relative grid size-7 place-items-center rounded-lg border border-white/10 bg-zinc-950">
      <span className="absolute size-4 rounded-full bg-gradient-to-br from-glow-1 to-glow-2 blur-[5px] [animation:breathe_4s_ease-in-out_infinite]" />
      <span className="relative size-2 rounded-full bg-amber-50" />
    </span>
  );
}
