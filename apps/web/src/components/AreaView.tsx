import { rgb } from "../ui";

type Ch = { id: number; x: number; z: number; color?: [number, number, number] };

/** The Area seen from the couch: the TV in the middle, each Channel at its
 *  Hue position (x = left/right, z = floor/ceiling), glowing with its color. */
export function AreaView({ channels, compact }: { channels: Ch[]; compact?: boolean }) {
  return (
    <div className={compact ? "relative aspect-[16/7]" : "relative aspect-[16/8]"}>
      <div className="absolute inset-[22%_26%] rounded-md border border-white/15 bg-gradient-to-b from-zinc-800/60 to-zinc-900/80 shadow-[0_20px_40px_-20px_rgb(0_0_0/0.8)]">
        <div className="absolute inset-x-[42%] -bottom-2 h-2 rounded-b bg-zinc-700/60" />
      </div>
      {channels.map((c) => {
        const lit = c.color && c.color.some((v) => v > 4);
        const left = `${((c.x + 1) / 2) * 84 + 8}%`;
        const top = `${((1 - c.z) / 2) * 76 + 12}%`;
        return (
          <div key={c.id} className="absolute -translate-x-1/2 -translate-y-1/2" style={{ left, top }}>
            {lit && (
              <div
                className="absolute top-1/2 left-1/2 size-16 -translate-x-1/2 -translate-y-1/2 rounded-full blur-xl transition-colors duration-300"
                style={{ background: rgb(c.color!), opacity: 0.7 }}
              />
            )}
            <div
              className="relative grid size-6 place-items-center rounded-full border text-[10px] font-semibold tabular-nums transition-colors duration-300"
              style={
                lit
                  ? { background: rgb(c.color!), borderColor: "rgb(255 255 255 / 0.5)", color: "rgb(0 0 0 / 0.6)" }
                  : { background: "#18181b", borderColor: "rgb(255 255 255 / 0.15)", color: "#a1a1aa" }
              }
            >
              {c.id}
            </div>
          </div>
        );
      })}
    </div>
  );
}
