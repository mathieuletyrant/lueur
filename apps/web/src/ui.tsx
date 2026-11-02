import type { ButtonHTMLAttributes, ReactNode } from "react";
import { AlertTriangle, Loader2 } from "lucide-react";

export const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

type Variant = "primary" | "secondary" | "ghost";

export function Button({
  variant = "secondary",
  loading,
  icon,
  className,
  children,
  disabled,
  ...p
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; loading?: boolean; icon?: ReactNode }) {
  return (
    <button
      {...p}
      disabled={disabled || loading}
      className={cx(
        "inline-flex h-9 items-center justify-center gap-2 rounded-lg px-3.5 text-sm font-medium whitespace-nowrap transition",
        "focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-glow-1 disabled:pointer-events-none disabled:opacity-50",
        variant === "primary" && "bg-zinc-50 text-zinc-950 shadow-sm hover:bg-white",
        variant === "secondary" && "border border-white/10 bg-white/[0.04] text-zinc-100 hover:bg-white/[0.08]",
        variant === "ghost" && "text-zinc-400 hover:bg-white/[0.06] hover:text-zinc-100",
        className,
      )}
    >
      {loading ? <Loader2 className="size-4 animate-spin" /> : icon}
      {children}
    </button>
  );
}

export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <section
      className={cx(
        "relative overflow-hidden rounded-2xl border border-white/[0.07] bg-zinc-900/40 shadow-[0_1px_0_0_rgb(255_255_255/0.04)_inset]",
        className,
      )}
    >
      {children}
    </section>
  );
}

export function CardHeader({ title, subtitle, action }: { title: ReactNode; subtitle?: ReactNode; action?: ReactNode }) {
  return (
    <div className="flex items-start justify-between gap-4 px-5 pt-5">
      <div className="min-w-0">
        <h2 className="text-sm font-semibold text-zinc-100">{title}</h2>
        {subtitle && <p className="mt-0.5 text-sm text-zinc-500">{subtitle}</p>}
      </div>
      {action}
    </div>
  );
}

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

export function Input(p: React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...p}
      className={cx(
        "h-9 w-full min-w-0 rounded-lg border border-white/10 bg-zinc-950/60 px-3 text-sm text-zinc-100 placeholder:text-zinc-600",
        "transition outline-none focus:border-white/25 focus:ring-4 focus:ring-white/[0.04]",
        p.className,
      )}
    />
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="block">
      <span className="mb-1.5 block text-xs font-medium text-zinc-400">{label}</span>
      {children}
      {hint && <span className="mt-1.5 block text-xs text-zinc-500">{hint}</span>}
    </label>
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

export function Toggle({ checked, onChange, label }: { checked: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={cx(
        "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-glow-1",
        checked ? "bg-zinc-100" : "bg-zinc-700",
      )}
    >
      <span
        className={cx(
          "inline-block size-4 rounded-full shadow transition",
          checked ? "translate-x-[18px] bg-zinc-950" : "translate-x-0.5 bg-zinc-300",
        )}
      />
    </button>
  );
}

export function Empty({ icon, title, children }: { icon: ReactNode; title: string; children?: ReactNode }) {
  return (
    <div className="flex flex-col items-center px-6 py-10 text-center">
      <div className="mb-3 grid size-10 place-items-center rounded-xl border border-white/10 bg-white/[0.03] text-zinc-400">
        {icon}
      </div>
      <p className="text-sm font-medium text-zinc-200">{title}</p>
      {children && <div className="mt-1 max-w-sm text-sm text-zinc-500">{children}</div>}
    </div>
  );
}

export function PageHeader({ title, subtitle, action }: { title: string; subtitle?: string; action?: ReactNode }) {
  return (
    <header className="mb-6 flex flex-wrap items-end justify-between gap-4">
      <div>
        <h1 className="text-2xl font-semibold tracking-tight text-zinc-50">{title}</h1>
        {subtitle && <p className="mt-1 text-sm text-zinc-500">{subtitle}</p>}
      </div>
      {action}
    </header>
  );
}

export const rgb = (c: [number, number, number]) => `rgb(${c[0]} ${c[1]} ${c[2]})`;

export function time(s: number) {
  s = Math.max(0, Math.floor(s));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}
