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
  setup: { label: "Setup", icon: Settings2 },
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
  const health = [
    { label: "Bridge", ok: cfg.paired && !p?.error, warn: !cfg.paired },
    { label: "Jellyfin", ok: !!cfg.jellyfin.url && !p?.jellyfin_error, warn: !cfg.jellyfin.url },
  ];

  return (
    <AppCtx.Provider value={ctx}>
      <div className="min-h-dvh md:grid md:grid-cols-[232px_1fr]">
        <aside className="pt-safe sticky top-0 z-20 border-b border-white/[0.06] bg-[#09090b]/80 backdrop-blur-xl md:h-dvh md:border-r md:border-b-0">
          <div className="flex items-center gap-2.5 px-4 py-3.5 md:px-5 md:py-5">
            <Logo />
            <span className="text-[15px] font-semibold tracking-tight">lueur</span>
            {/* Mobile: the health dots sit in the top bar, the nav at the bottom. */}
            <span className="ml-auto flex gap-3 md:hidden">
              {health.map((h) => (
                <span key={h.label} className="flex items-center gap-1.5 text-xs text-zinc-500">
                  <span
                    className={cx("size-1.5 rounded-full", h.ok ? "bg-emerald-400" : h.warn ? "bg-zinc-600" : "bg-rose-400")}
                  />
                  {h.label}
                </span>
              ))}
            </span>
          </div>
          <nav className="hidden flex-col gap-1 px-3 md:flex" aria-label="Main">
            {(Object.keys(views) as View[]).map((k) => {
              const { label, icon: Icon } = views[k];
              const todo = k === "setup" && steps.some((s) => !s);
              return (
                <a
                  key={k}
                  href={`#/${k}`}
                  aria-current={view === k ? "page" : undefined}
                  className={cx(
                    "flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm font-medium whitespace-nowrap transition",
                    view === k ? "bg-white/[0.07] text-zinc-50" : "text-zinc-400 hover:bg-white/[0.04] hover:text-zinc-200",
                  )}
                >
                  <Icon className="size-4" />
                  {label}
                  {todo && <span className="ml-auto size-1.5 rounded-full bg-glow-1" aria-label="incomplete" />}
                </a>
              );
            })}
          </nav>
          <div className="absolute inset-x-0 bottom-0 hidden space-y-2 border-t border-white/[0.06] px-5 py-4 md:block">
            {health.map((h) => (
              <div key={h.label} className="flex items-center justify-between text-xs">
                <span className="text-zinc-500">{h.label}</span>
                <span
                  className={cx(
                    "flex items-center gap-1.5",
                    h.ok ? "text-emerald-300" : h.warn ? "text-zinc-500" : "text-rose-300",
                  )}
                >
                  <span
                    className={cx("size-1.5 rounded-full", h.ok ? "bg-emerald-400" : h.warn ? "bg-zinc-600" : "bg-rose-400")}
                  />
                  {h.ok ? "Connected" : h.warn ? "Not set up" : "Error"}
                </span>
              </div>
            ))}
          </div>
        </aside>

        <main className="mx-auto w-full max-w-5xl px-4 pt-6 pb-28 md:px-10 md:py-10">
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
