import type { Status } from "../../api";
import { cx } from "../../ui";

type Health = { label: string; tone: "live" | "ok" | "warn" | "bad"; detail?: string };

/** One status for Bridge and Jellyfin: quiet when all is well, words only when something needs you. */
function health(setupDone: boolean, p?: Status["player"]): Health {
  if (!setupDone) return { label: "Setup needed", tone: "warn" };
  if (p?.error) return { label: "Bridge error", tone: "bad", detail: p.error };
  if (p?.jellyfin_error) return { label: "Jellyfin unreachable", tone: "bad", detail: p.jellyfin_error };
  return p?.streaming ? { label: "Live", tone: "live" } : { label: "Ready", tone: "ok" };
}

export function StatusPill({ setupDone, player }: { setupDone: boolean; player?: Status["player"] }) {
  const h = health(setupDone, player);
  const trouble = h.tone === "warn" || h.tone === "bad";
  return (
    <a
      href={trouble ? "#/setup" : "#/now"}
      title={h.detail ?? "Bridge and Jellyfin connected"}
      className={cx(
        "-ml-2 flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium transition",
        h.tone === "live" && "text-emerald-300",
        h.tone === "ok" && "text-zinc-500 hover:text-zinc-300",
        h.tone === "warn" && "bg-amber-400/10 text-amber-300 ring-1 ring-amber-400/20 ring-inset",
        h.tone === "bad" && "bg-rose-400/10 text-rose-300 ring-1 ring-rose-400/20 ring-inset",
      )}
    >
      <span className="relative flex size-1.5">
        {h.tone === "live" && <span className="absolute inline-flex size-full animate-ping rounded-full bg-emerald-400 opacity-75" />}
        <span
          className={cx(
            "relative inline-flex size-1.5 rounded-full",
            h.tone === "warn" ? "bg-amber-400" : h.tone === "bad" ? "bg-rose-400" : "bg-emerald-400",
          )}
        />
      </span>
      {h.label}
    </a>
  );
}
