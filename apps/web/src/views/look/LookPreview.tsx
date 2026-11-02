import type { Look } from "../../api";
import { apply } from "../../lib/look";
import { Card, CardHeader, rgb } from "../../ui";

/** Typical screen colors, to see what the Look does to them. */
const palette: { name: string; c: [number, number, number] }[] = [
  { name: "Night", c: [22, 34, 66] },
  { name: "Teal", c: [38, 104, 112] },
  { name: "Skin", c: [214, 150, 112] },
  { name: "Sunset", c: [232, 96, 48] },
  { name: "Forest", c: [52, 92, 44] },
  { name: "Neon", c: [190, 40, 150] },
  { name: "Grey sky", c: [150, 156, 162] },
  { name: "Dark room", c: [14, 10, 8] },
];

/** Screen colors next to what the lights show with `look`. */
export function LookPreview({ look }: { look: Look }) {
  return (
  <Card className="self-start lg:col-span-2">
    <CardHeader title="Preview" subtitle="Screen colors, and what the lights will show." />
    <div className="space-y-2 px-5 pt-4 pb-5">
      {palette.map((p) => {
        const out = apply(look, p.c);
        return (
          <div key={p.name} className="flex items-center gap-3">
            <span className="w-20 shrink-0 text-xs text-zinc-500">{p.name}</span>
            <span className="h-8 flex-1 rounded-l-lg" style={{ background: rgb(p.c) }} />
            <span
              className="h-8 flex-1 rounded-r-lg transition-colors"
              style={{ background: rgb(out), boxShadow: `0 0 24px -4px ${rgb(out)}` }}
            />
          </div>
        );
      })}
      <div className="flex pt-1 pl-23 text-[11px] text-zinc-600">
        <span className="flex-1">Screen</span>
        <span className="flex-1 pl-3">Lights</span>
      </div>
    </div>
  </Card>
  );
}
