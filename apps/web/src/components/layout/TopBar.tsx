import { Lightbulb, LightbulbOff } from "lucide-react";
import type { Status } from "../../api";
import { type View, useApp, views } from "../../context";
import { cx } from "../../ui";
import { Logo } from "./Logo";
import { StatusPill } from "./StatusPill";

/** Logo and status; on desktop, the tabs too (on mobile they live in `BottomTabs`). */
export function TopBar({ view, setupDone, player }: { view: View; setupDone: boolean; player?: Status["player"] }) {
  return (
    <header className="sticky top-0 z-30 border-b border-white/[0.06] bg-[#09090b]/85 backdrop-blur-xl">
      <div className="mx-auto flex h-14 max-w-5xl items-center gap-6 px-4 md:h-16 md:px-6">
        <a href="#/now" className="flex items-center gap-2.5">
          <Logo />
          <span className="text-[17px] font-semibold tracking-tight">lueur</span>
        </a>
        <StatusPill setupDone={setupDone} player={player} />
        <LightsSwitch />
        <nav className="hidden gap-1 md:flex" aria-label="Main">
          {(Object.keys(views) as View[]).map((k) => {
            const { label, icon: Icon } = views[k];
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
                {k === "setup" && !setupDone && <SetupDot className="top-1.5 right-2" />}
              </a>
            );
          })}
        </nav>
      </div>
    </header>
  );
}

/** Pauses the ambilight: films leave the lights alone until switched back on. */
function LightsSwitch() {
  const { cfg, patch } = useApp();
  const on = cfg.lights_on;
  const Icon = on ? Lightbulb : LightbulbOff;
  return (
    <button
      type="button"
      onClick={() => patch({ lights_on: !on })}
      title={on ? "Pause the lights" : "Lights paused: switch back on"}
      aria-pressed={!on}
      className={cx(
        "ml-auto flex items-center gap-1.5 rounded-full px-3 py-1.5 text-xs font-medium transition",
        on ? "text-zinc-400 hover:bg-white/[0.06] hover:text-zinc-100" : "bg-amber-400/10 text-amber-300 ring-1 ring-amber-400/20 ring-inset",
      )}
    >
      <Icon className="size-4" />
      {!on && "Lights paused"}
    </button>
  );
}

export function SetupDot({ className }: { className: string }) {
  return <span className={cx("absolute size-1.5 rounded-full bg-glow-1", className)} aria-label="incomplete" />;
}
