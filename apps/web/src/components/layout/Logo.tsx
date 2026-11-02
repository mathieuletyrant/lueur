export function Logo() {
  return (
    <span className="relative grid size-7 place-items-center rounded-lg border border-white/10 bg-zinc-950">
      <span className="absolute size-4 rounded-full bg-gradient-to-br from-glow-1 to-glow-2 blur-[5px] [animation:breathe_4s_ease-in-out_infinite]" />
      <span className="relative size-2 rounded-full bg-amber-50" />
    </span>
  );
}
