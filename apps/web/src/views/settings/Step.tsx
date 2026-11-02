import { Check } from "lucide-react";
import { type ReactNode } from "react";
import { Button, Card, cx } from "../../ui";

export function Step({
  n,
  title,
  summary,
  done,
  locked,
  open,
  onToggle,
  children,
}: {
  n: number;
  title: string;
  summary: ReactNode;
  done: boolean;
  locked?: boolean;
  open: boolean;
  onToggle?: () => void;
  children: ReactNode;
}) {
  return (
    <li>
      <Card className={cx(locked && "opacity-50")}>
        <div className="flex items-center gap-4 px-5 py-4">
          <span
            className={cx(
              "grid size-7 shrink-0 place-items-center rounded-full text-xs font-semibold",
              done ? "bg-emerald-400/15 text-emerald-300 ring-1 ring-emerald-400/30" : "bg-white/[0.06] text-zinc-300 ring-1 ring-white/10",
            )}
          >
            {done ? <Check className="size-3.5" /> : n}
          </span>
          <div className="min-w-0 flex-1">
            <h2 className="text-sm font-semibold">{title}</h2>
            <div className="truncate text-sm text-zinc-500">{summary}</div>
          </div>
          {onToggle && !locked && (
            <Button variant="ghost" onClick={onToggle}>
              {open ? "Close" : done ? "Change" : "Set up"}
            </Button>
          )}
        </div>
        {open && !locked && <div className="border-t border-white/[0.06] px-5 py-5">{children}</div>}
      </Card>
    </li>
  );
}

/* ---------------------------------------------------------------- Bridge */

export function Choice({
  selected,
  onClick,
  className,
  children,
}: {
  selected: boolean;
  onClick: () => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && e.target === e.currentTarget && (e.preventDefault(), onClick())}
      aria-pressed={selected}
      className={cx(
        "flex cursor-pointer items-center gap-3 rounded-xl border p-3.5 text-left transition outline-none focus-visible:ring-2 focus-visible:ring-glow-1/60",
        selected
          ? "border-glow-1/50 bg-glow-1/[0.06] ring-4 ring-glow-1/[0.07]"
          : "border-white/[0.08] bg-white/[0.02] hover:border-white/15 hover:bg-white/[0.04]",
        className,
      )}
    >
      {children}
    </div>
  );
}
