import type { Led } from "../lib/strip";
import { cx, rgb } from "../ui";

/** The TV in the drawing, in viewBox units (160 × 100). */
const TV = { left: 16, right: 144, top: 14, bottom: 86 };
/** How far outside the TV's edge the LEDs are drawn. */
const OUT = 4.5;

function place(led: Led) {
  const x = TV.left + ((led.x + 1) / 2) * (TV.right - TV.left);
  const y = TV.top + ((1 - led.z) / 2) * (TV.bottom - TV.top);
  // Pushed out of the TV, toward the side the LED sits on.
  const dx = led.x <= -0.999 ? -OUT : led.x >= 0.999 ? OUT : 0;
  const dy = led.z >= 0.999 ? -OUT : led.z <= -0.999 ? OUT : 0;
  return { x: x + dx, y: y + dy };
}

/** A WLED Strip seen from the couch: every LED around the TV, the first one
 *  marked, so the start corner and the direction can be checked at a glance. */
export function StripView({
  leds,
  colors,
  onLed,
  screen,
}: {
  leds: Led[];
  /** Image shown on the TV. */
  screen?: string;
  /** Live colors by LED id, sRGB. */
  colors?: Map<number, [number, number, number]>;
  /** Makes each LED clickable (to blink it). */
  onLed?: (id: number) => void;
}) {
  const first = leds[0];
  const second = leds[1];
  const last = leds[leds.length - 1];
  return (
    <svg viewBox="0 0 160 100" className="w-full select-none" role="img" aria-label={`Strip of ${leds.length} LEDs around the TV`}>
      <rect
        x={TV.left}
        y={TV.top}
        width={TV.right - TV.left}
        height={TV.bottom - TV.top}
        rx={1.5}
        className="fill-zinc-950 stroke-white/15"
        strokeWidth={0.4}
      />
      {screen && (
        <image
          href={screen}
          x={TV.left}
          y={TV.top}
          width={TV.right - TV.left}
          height={TV.bottom - TV.top}
          preserveAspectRatio="xMidYMid slice"
        />
      )}
      {leds.map((led) => {
        const p = place(led);
        const color = colors?.get(led.id);
        const lit = !!color?.some((v) => v > 4);
        const edge = led.id === first?.id || led.id === last?.id;
        return (
          <circle
            key={led.id}
            cx={p.x}
            cy={p.y}
            r={edge ? 1.6 : 1}
            onClick={onLed ? () => onLed(led.id) : undefined}
            className={cx(
              !lit && (led.id === first?.id ? "fill-glow-1" : "fill-zinc-600"),
              onLed && "cursor-pointer hover:opacity-70",
            )}
            style={lit ? { fill: rgb(color!) } : undefined}
          >
            <title>{onLed ? `Blink LED ${led.id}` : `LED ${led.id}`}</title>
          </circle>
        );
      })}
      {first && (
        <text {...label(place(first))} className="fill-glow-1 text-[4px] font-semibold">
          0
        </text>
      )}
      {last && last !== first && (
        <text {...label(place(last))} className="fill-zinc-400 text-[4px]">
          {last.id}
        </text>
      )}
      {first && second && <Arrow from={place(first)} to={place(second)} />}
    </svg>
  );
}

/** Where a LED's number goes: a little further out than the LED. */
function label(p: { x: number; y: number }) {
  const x = p.x < TV.left ? p.x - 3 : p.x > TV.right ? p.x + 3 : p.x;
  const y = p.y < TV.top ? p.y - 2.5 : p.y > TV.bottom ? p.y + 5 : p.y + 1.4;
  return { x, y, textAnchor: "middle" as const };
}

/** A short arrow inside the TV, from the first LED toward the second: the strip's direction. */
function Arrow({ from, to }: { from: { x: number; y: number }; to: { x: number; y: number } }) {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const len = Math.hypot(dx, dy) || 1;
  const [ux, uy] = [dx / len, dy / len];
  // Start just inside the TV, in front of LED 0.
  const sx = Math.min(TV.right - 6, Math.max(TV.left + 6, from.x));
  const sy = Math.min(TV.bottom - 6, Math.max(TV.top + 6, from.y));
  const ex = sx + ux * 12;
  const ey = sy + uy * 12;
  const head = `${ex},${ey} ${ex - ux * 2.5 - uy * 1.5},${ey - uy * 2.5 + ux * 1.5} ${ex - ux * 2.5 + uy * 1.5},${ey - uy * 2.5 - ux * 1.5}`;
  return (
    <g className="fill-glow-1 stroke-glow-1" strokeWidth={0.6}>
      <line x1={sx} y1={sy} x2={ex} y2={ey} />
      <polygon points={head} />
    </g>
  );
}
