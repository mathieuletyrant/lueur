import { useEffect, useState } from "react";
import { api } from "../api";
import { cx } from "../ui";

/** The film's colors from start to end, as a thin stripe; `at` marks a moment (0..1). */
export function Barcode({ path, duration, at, className }: { path: string; duration: number; at?: number; className?: string }) {
  const [colors, setColors] = useState<string[]>();
  useEffect(() => {
    let alive = true;
    api.barcode(path, duration).then((c) => alive && setColors(c), () => {});
    return () => {
      alive = false;
    };
  }, [path, duration]);
  const step = colors ? 100 / colors.length : 0;
  return (
    <div
      className={cx("relative h-1.5 overflow-hidden rounded-full bg-white/[0.04]", className)}
      style={
        colors && {
          background: `linear-gradient(to right, ${colors.map((c, i) => `${c} ${i * step}% ${(i + 1) * step}%`).join(", ")})`,
        }
      }
    >
      {at != null && (
        <span className="absolute inset-y-0 w-0.5 -translate-x-1/2 bg-white shadow-[0_0_4px_rgb(0_0_0/0.8)]" style={{ left: `${at * 100}%` }} />
      )}
    </div>
  );
}
