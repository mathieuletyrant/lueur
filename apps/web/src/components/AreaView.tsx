import { useApp } from "../App";
import { rgb } from "../ui";

/** Same stretch as the player (Look → Fit to area): the area's extent maps to 0..1. */
function fitter(vals: number[], on: boolean) {
  const lo = Math.min(...vals);
  const hi = Math.max(...vals);
  return (v: number) => (on && hi - lo > 0.2 ? (v - lo) / (hi - lo) : (v + 1) / 2);
}

type Ch = { id: number; x: number; z: number; color?: [number, number, number] };

/** The Area seen from the couch: the TV in the middle, each Channel where
 *  lueur reads the screen for it (x = left/right, z = floor/ceiling), glowing with its color. */
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
  return (
    <div className={compact ? "relative aspect-[16/7]" : "relative aspect-[16/8]"}>
      <div className="absolute inset-[22%_26%] rounded-md border border-white/15 bg-gradient-to-b from-zinc-800/60 to-zinc-900/80 shadow-[0_20px_40px_-20px_rgb(0_0_0/0.8)]">
        {screen && <img src={screen} alt="" className="absolute inset-0 size-full rounded-md object-cover" />}
        <div className="absolute inset-x-[42%] -bottom-2 h-2 rounded-b bg-zinc-700/60" />
      </div>
      {channels.map((c) => {
        const lit = c.color && c.color.some((v) => v > 4);
        const left = `${fx(c.x) * 76 + 12}%`;
        const top = `${(1 - fz(c.z)) * 72 + 14}%`;
        return (
          <div key={c.id} className="absolute -translate-x-1/2 -translate-y-1/2" style={{ left, top }}>
            {lit && (
              <div
                className="absolute top-1/2 left-1/2 size-16 -translate-x-1/2 -translate-y-1/2 rounded-full blur-xl transition-colors duration-300"
                style={{ background: rgb(c.color!), opacity: 0.7 }}
              />
            )}
            <Dot
              onClick={onChannel && ((e) => (e.stopPropagation(), onChannel(c.id)))}
              title={onChannel ? `Blink light ${c.id}` : `Light ${c.id}`}
              className="relative grid size-6 place-items-center rounded-full border text-[10px] font-semibold tabular-nums transition duration-300"
              style={
                lit
                  ? { background: rgb(c.color!), borderColor: "rgb(255 255 255 / 0.5)", color: "rgb(0 0 0 / 0.6)" }
                  : { background: "#18181b", borderColor: "rgb(255 255 255 / 0.15)", color: "#a1a1aa" }
              }
            >
              {c.id}
            </Dot>
          </div>
        );
      })}
    </div>
  );
}

function Dot({ onClick, ...p }: React.HTMLAttributes<HTMLElement> & { onClick?: (e: React.MouseEvent) => void }) {
  return onClick ? (
    <button
      type="button"
      {...p}
      onClick={onClick}
      className={p.className + " cursor-pointer hover:scale-125 hover:ring-4 hover:ring-white/15 focus-visible:outline-2 focus-visible:outline-glow-1"}
    />
  ) : (
    <div {...p} />
  );
}
