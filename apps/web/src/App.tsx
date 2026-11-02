import { Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { api, useLoad, usePoll } from "./api";
import { BottomTabs } from "./components/layout/BottomTabs";
import { TopBar } from "./components/layout/TopBar";
import { AppCtx, type Ctx, setupSteps, type View, views } from "./context";
import { Alert } from "./ui";
import { LibraryView } from "./views/library";
import { LookView } from "./views/look";
import { NowView } from "./views/now";
import { SettingsView } from "./views/settings";

const fromHash = (): View => {
  const h = location.hash.slice(2);
  return h in views ? (h as View) : "now";
};

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
  const setupDone = steps.every(Boolean);

  return (
    <AppCtx.Provider value={ctx}>
      <div className="pt-safe min-h-dvh">
        <TopBar view={view} setupDone={setupDone} player={status?.player} />
        <main className="mx-auto w-full max-w-5xl px-4 pt-6 pb-28 md:px-6 md:pt-10 md:pb-12">
          {view === "now" && <NowView />}
          {view === "library" && <LibraryView />}
          {view === "look" && <LookView />}
          {view === "setup" && <SettingsView />}
        </main>
      </div>

      <BottomTabs view={view} setupDone={setupDone} />
    </AppCtx.Provider>
  );
}
