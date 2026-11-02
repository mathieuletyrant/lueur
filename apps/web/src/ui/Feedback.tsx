import { AlertTriangle, Loader2 } from "lucide-react";
import { type ReactNode } from "react";
import { cx } from "./format";

type Tone = "neutral" | "green" | "amber" | "red" | "blue";

const tones: Record<Tone, string> = {
  neutral: "bg-white/[0.06] text-zinc-300 ring-white/10",
  green: "bg-emerald-400/10 text-emerald-300 ring-emerald-400/20",
  amber: "bg-amber-400/10 text-amber-300 ring-amber-400/20",
  red: "bg-rose-400/10 text-rose-300 ring-rose-400/20",
  blue: "bg-sky-400/10 text-sky-300 ring-sky-400/20",
};

const dots: Record<Tone, string> = {
  neutral: "bg-zinc-400",
  green: "bg-emerald-400",
  amber: "bg-amber-400",
  red: "bg-rose-400",
  blue: "bg-sky-400",
};

export function Pill({ tone = "neutral", pulse, children }: { tone?: Tone; pulse?: boolean; children: ReactNode }) {
  return (
    <span
      className={cx(
        "inline-flex items-center gap-1.5 rounded-full px-2.5 py-0.5 text-xs font-medium ring-1 ring-inset",
        tones[tone],
      )}
    >
      <span className="relative flex size-1.5">
        {pulse && <span className={cx("absolute inline-flex size-full animate-ping rounded-full opacity-75", dots[tone])} />}
        <span className={cx("relative inline-flex size-1.5 rounded-full", dots[tone])} />
      </span>
      {children}
    </span>
  );
}

export function Alert({ children, tone = "red" }: { children: ReactNode; tone?: "red" | "amber" }) {
  return (
    <div
      role="alert"
      className={cx(
        "flex items-start gap-3 rounded-xl border px-4 py-3 text-sm",
        tone === "red" ? "border-rose-400/20 bg-rose-400/[0.06] text-rose-200" : "border-amber-400/20 bg-amber-400/[0.06] text-amber-100",
      )}
    >
      <AlertTriangle className="mt-0.5 size-4 shrink-0 opacity-80" />
      <div className="min-w-0 break-words">{children}</div>
    </div>
  );
}

export function Progress({ value, className }: { value: number | null; className?: string }) {
  return (
    <div className={cx("h-1.5 overflow-hidden rounded-full bg-white/[0.06]", className)}>
      {value === null ? (
        <div className="h-full w-1/3 animate-pulse rounded-full bg-zinc-400/60" />
      ) : (
        <div
          className="h-full rounded-full bg-gradient-to-r from-glow-1 to-glow-2 transition-[width] duration-500"
          style={{ width: `${Math.max(0, Math.min(1, value)) * 100}%` }}
        />
      )}
    </div>
  );
}

/** Where an Analysis decodes: the GPU (fast, light) or the CPU (slow, heavier on the server). */
export function Decoder({ cpu }: { cpu: boolean }) {
  return (
    <span
      title={cpu ? "Decoding on the CPU: slower, uses more of the server" : "Decoding on the GPU"}
      className={cx(
        "shrink-0 rounded px-1.5 py-px text-[10px] font-semibold tracking-wide ring-1 ring-inset",
        cpu ? "bg-amber-400/10 text-amber-300 ring-amber-400/20" : "bg-emerald-400/10 text-emerald-300 ring-emerald-400/20",
      )}
    >
      {cpu ? "CPU" : "GPU"}
    </span>
  );
}

/** A running Analysis: name, GPU/CPU, progress. */
export function RunningAnalysis({ name, progress, cpu }: { name: string; progress: number | null; cpu: boolean }) {
  return (
    <>
      <div className="mb-2 flex items-center justify-between gap-3 text-sm">
        <span className="flex min-w-0 items-center gap-2">
          <Loader2 className="size-3.5 shrink-0 animate-spin text-glow-1" />
          <span className="truncate">{name}</span>
          <Decoder cpu={cpu} />
        </span>
        <span className="text-xs text-zinc-500 tabular-nums">{progress !== null ? `${Math.round(progress * 100)}%` : "…"}</span>
      </div>
      <Progress value={progress} />
    </>
  );
}
