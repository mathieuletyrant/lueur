import { Loader2 } from "lucide-react";
import { type ButtonHTMLAttributes, type ReactNode } from "react";
import { cx } from "./format";

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
