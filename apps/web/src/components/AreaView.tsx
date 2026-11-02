import { useApp } from "../context";
import { cx, rgb } from "../ui";

/** Same stretch as the player (Look → Fit to area): the area's extent maps to 0..1. */
function fitter(vals: number[], on: boolean) {
  const lo = Math.min(...vals);
  const hi = Math.max(...vals);
  return (v: number) => (on && hi - lo > 0.2 ? (v - lo) / (hi - lo) : (v + 1) / 2);
}

type Ch = { id: number; x: number; z: number; color?: [number, number, number] };

/** The TV inside the view, in % of its box. */
const TV = { left: 24, right: 76, top: 20, bottom: 80 };
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));

/** Where a light sits on the TV's edge, from where it reads the screen (u, v in 0..1). */
function onEdge(u: number, v: number) {
  const x = TV.left + u * (TV.right - TV.left);
  const y = TV.top + v * (TV.bottom - TV.top);
  // The closest edge; left and right win near the corners, like a strip's columns.
  const d = { left: u, right: 1 - u, top: v * 1.1, bottom: (1 - v) * 1.1 };
  const side = (Object.keys(d) as (keyof typeof d)[]).reduce((a, b) => (d[a] <= d[b] ? a : b));
  const inset = 5;
  if (side === "left") return { side, x: TV.left, y: clamp(y, TV.top + inset, TV.bottom - inset) };
  if (side === "right") return { side, x: TV.right, y: clamp(y, TV.top + inset, TV.bottom - inset) };
  return { side, x: clamp(x, TV.left + inset, TV.right - inset), y: side === "top" ? TV.top : TV.bottom };
}

const LABEL = {
  left: "right-full mr-2 top-1/2 -translate-y-1/2",
  right: "left-full ml-2 top-1/2 -translate-y-1/2",
  top: "bottom-full mb-1.5 left-1/2 -translate-x-1/2",
  bottom: "top-full mt-1.5 left-1/2 -translate-x-1/2",
};

/** The Area seen from the couch, as an ambilight: each Channel is a LED on
 *  the edge of the TV it reads, washing the wall behind with its color. */
export function AreaView({
  channels,
  compact,
  onChannel,
  screen,
}: {
  channels: Ch[];
  compact?: boolean;
  /** Image shown on the TV. */
  screen?: string;
  /** Makes each light clickable (to blink it). */
  onChannel?: (id: number) => void;
}) {
  const { cfg } = useApp();
  const fx = fitter(channels.map((c) => c.x), cfg.look.fit_area);
  const fz = fitter(channels.map((c) => c.z), cfg.look.fit_area);
  const lights = channels.map((c) => ({
    ...c,
    ...onEdge(fx(c.x), 1 - fz(c.z)),
    lit: !!c.color?.some((v) => v > 4),
  }));

  return (
    <div className={cx("relative isolate select-none", compact ? "aspect-[16/7]" : "aspect-[16/8]")}>
      {/* The wall: a soft wash per lit light, centred on the TV's edge. Gradients,
          not CSS blur, which Safari clips to a hard edge. */}
      {lights
        .filter((l) => l.lit)
        .map((l) => (
          <div
            key={`wash-${l.id}`}
            aria-hidden
            className="pointer-events-none absolute aspect-square w-[44%] -translate-x-1/2 -translate-y-1/2"
            style={{
              left: `${l.x}%`,
              top: `${l.y}%`,
              background: `radial-gradient(closest-side, ${rgb(l.color!)}, transparent)`,
              opacity: 0.5,
              mixBlendMode: "screen",
            }}
          />
        ))}

      {/* The TV, in front of the wall. */}
      <div
        className="absolute overflow-hidden rounded-md border border-white/15 bg-zinc-950 shadow-[0_24px_48px_-24px_rgb(0_0_0/0.9)]"
        style={{ left: `${TV.left}%`, right: `${100 - TV.right}%`, top: `${TV.top}%`, bottom: `${100 - TV.bottom}%` }}
      >
        {screen ? (
          <img src={screen} alt="" className="size-full object-cover" />
        ) : (
          <div className="size-full bg-gradient-to-b from-zinc-800/50 to-zinc-950" />
        )}
      </div>
      <div aria-hidden className="absolute left-[45%] h-[3%] w-[10%] rounded-b bg-zinc-800" style={{ top: `${TV.bottom}%` }} />

      {/* The LEDs on the TV's edge, their channel number outside. */}
      {lights.map((l) => {
        const style = {
          left: `${l.x}%`,
          top: `${l.y}%`,
          ...(l.lit
            ? { background: rgb(l.color!), borderColor: "rgb(255 255 255 / 0.6)", boxShadow: `0 0 10px 2px ${rgb(l.color!)}` }
            : { background: "#27272a", borderColor: "rgb(255 255 255 / 0.2)" }),
        };
        const className = cx("group absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border transition");
        const label = (
          <span className={cx("absolute text-[10px] font-medium tabular-nums text-zinc-400 group-hover:text-zinc-100", LABEL[l.side])}>
            {l.id}
          </span>
        );
        return onChannel ? (
          <button
            key={l.id}
            type="button"
            title={`Blink light ${l.id}`}
            // Inside a clickable area card in Settings: don't also select the card.
            onClick={(e) => (e.stopPropagation(), onChannel(l.id))}
            className={cx(className, "cursor-pointer hover:scale-150 focus-visible:outline-2 focus-visible:outline-glow-1")}
            style={style}
          >
            {label}
          </button>
        ) : (
          <span key={l.id} title={`Light ${l.id}`} className={className} style={style}>
            {label}
          </span>
        );
      })}
    </div>
  );
}
