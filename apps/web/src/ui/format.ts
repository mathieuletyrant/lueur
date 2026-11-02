export const cx = (...c: (string | false | null | undefined)[]) => c.filter(Boolean).join(" ");

export const rgb = (c: [number, number, number]) => `rgb(${c[0]} ${c[1]} ${c[2]})`;

export function time(s: number) {
  s = Math.max(0, Math.floor(s));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`;
}
