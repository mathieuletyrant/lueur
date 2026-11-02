import { type View, views } from "../../context";
import { cx } from "../../ui";
import { SetupDot } from "./TopBar";

/** The tabs on mobile, at the bottom like an iOS app. */
export function BottomTabs({ view, setupDone }: { view: View; setupDone: boolean }) {
  return (
    <nav
      className="pb-safe fixed inset-x-0 bottom-0 z-30 border-t border-white/[0.06] bg-[#09090b]/90 backdrop-blur-xl md:hidden"
      aria-label="Main"
    >
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
              {k === "setup" && !setupDone && <SetupDot className="top-1.5 right-[calc(50%-18px)]" />}
            </a>
          );
        })}
      </div>
    </nav>
  );
}
