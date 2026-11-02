import { ChevronRight, CornerLeftUp, Folder, HardDrive, X } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { api, type Dir } from "../api";
import { Alert, Button } from "../ui";

/** Browses the folders lueur can see (not Jellyfin's) and returns one. */
export function FolderPicker({
  start,
  onPick,
  onClose,
}: {
  start: string;
  onPick: (path: string) => void;
  onClose: () => void;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const [dir, setDir] = useState<Dir>();
  const [error, setError] = useState<string>();

  const open = (p: string) =>
    api
      .fs(p)
      .then((d) => {
        setDir(d);
        setError(undefined);
      })
      .catch((e: Error) => setError(e.message));

  useEffect(() => {
    ref.current?.showModal();
    open(start);
  }, [start]);

  const parts = dir?.path.split("/").filter(Boolean) ?? [];

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      className="m-auto w-[min(560px,calc(100vw-32px))] rounded-2xl border border-white/10 bg-zinc-900 p-0 text-zinc-100 shadow-2xl"
    >
      <div className="flex items-center justify-between border-b border-white/[0.06] px-5 py-4">
        <div>
          <h2 className="text-sm font-semibold">Choose a folder</h2>
          <p className="text-xs text-zinc-500">As seen by lueur, on this machine.</p>
        </div>
        <Button variant="ghost" className="size-8 px-0" onClick={() => ref.current?.close()} aria-label="Close">
          <X className="size-4" />
        </Button>
      </div>

      <nav className="flex flex-wrap items-center gap-0.5 px-4 pt-3 text-sm" aria-label="Path">
        <button className="rounded-md px-1.5 py-0.5 text-zinc-400 hover:bg-white/[0.06] hover:text-zinc-100" onClick={() => open("/")}>
          <HardDrive className="size-3.5" />
        </button>
        {parts.map((p, i) => (
          <span key={i} className="flex items-center gap-0.5">
            <ChevronRight className="size-3.5 text-zinc-600" />
            <button
              className="rounded-md px-1.5 py-0.5 text-zinc-300 hover:bg-white/[0.06] hover:text-zinc-100"
              onClick={() => open("/" + parts.slice(0, i + 1).join("/"))}
            >
              {p}
            </button>
          </span>
        ))}
      </nav>

      <div className="mx-4 my-3 h-72 overflow-y-auto rounded-xl border border-white/[0.06] bg-zinc-950/50 p-1">
        {error && <Alert>{error}</Alert>}
        {dir?.parent && (
          <Row onClick={() => open(dir.parent!)}>
            <CornerLeftUp className="size-4 text-zinc-500" />
            <span className="text-zinc-400">..</span>
          </Row>
        )}
        {dir?.dirs.map((d) => (
          <Row key={d} onClick={() => open(`${dir.path.replace(/\/$/, "")}/${d}`)}>
            <Folder className="size-4 text-glow-1/80" />
            <span className="truncate">{d}</span>
          </Row>
        ))}
        {dir && dir.dirs.length === 0 && <p className="px-3 py-6 text-center text-sm text-zinc-500">No sub-folders</p>}
      </div>

      <div className="flex items-center justify-between gap-3 border-t border-white/[0.06] px-5 py-4">
        <code className="truncate font-mono text-xs text-zinc-400">{dir?.path}</code>
        <Button variant="primary" disabled={!dir} onClick={() => dir && (onPick(dir.path), ref.current?.close())}>
          Use this folder
        </Button>
      </div>
    </dialog>
  );
}

function Row({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      onClick={onClick}
      className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm text-zinc-200 hover:bg-white/[0.05]"
    >
      {children}
    </button>
  );
}
