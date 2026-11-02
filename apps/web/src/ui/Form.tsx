import { type ReactNode } from "react";
import { cx } from "./format";

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

/** A labelled range input, its track filled up to the thumb. */
export function Slider({
  id,
  label,
  display,
  value,
  min,
  max,
  step,
  onChange,
  hint,
}: {
  id: string;
  label: ReactNode;
  display: ReactNode;
  value: number;
  min: number;
  max: number;
  step: number;
  onChange: (v: number) => void;
  hint?: ReactNode;
}) {
  return (
    <div>
      <div className="flex items-baseline justify-between gap-4">
        <label htmlFor={id} className="text-sm font-medium">{label}</label>
        <span className="text-sm text-zinc-300 tabular-nums">{display}</span>
      </div>
      <input
        id={id}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="mt-3 w-full"
        style={{ "--p": `${((value - min) / (max - min || 1)) * 100}%` } as React.CSSProperties}
      />
      {hint && <p className="mt-1 text-xs text-zinc-500">{hint}</p>}
    </div>
  );
}

/** A setting with a title, an explanation and a switch. */
export function SwitchRow({
  label,
  hint,
  checked,
  onChange,
  className,
}: {
  label: string;
  hint: ReactNode;
  checked: boolean;
  onChange: (v: boolean) => void;
  className?: string;
}) {
  return (
    <label className={cx("flex items-start justify-between gap-4", className)}>
      <span>
        <span className="block text-sm font-medium">{label}</span>
        <span className="mt-1 block text-xs text-zinc-500">{hint}</span>
      </span>
      <Toggle label={label} checked={checked} onChange={onChange} />
    </label>
  );
}

/** A native select styled like `Input`. */
export function Select({ className, ...p }: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      {...p}
      className={cx(
        "h-9 w-full rounded-lg border border-white/10 bg-zinc-950/60 px-3 text-sm text-zinc-100 outline-none focus:border-white/25",
        className,
      )}
    />
  );
}

/** A row of mutually exclusive choices. */
export function Segmented<T extends string | number>({
  label,
  options,
  value,
  onChange,
}: {
  label: string;
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
}) {
  return (
    <div className="flex flex-wrap gap-1 rounded-lg border border-white/10 p-0.5" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="radio"
          aria-checked={value === o.value}
          onClick={() => onChange(o.value)}
          className={cx(
            "h-7 flex-auto rounded-md px-2.5 text-xs font-medium whitespace-nowrap tabular-nums transition",
            value === o.value ? "bg-white/10 text-zinc-50" : "text-zinc-500 hover:text-zinc-200",
          )}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
