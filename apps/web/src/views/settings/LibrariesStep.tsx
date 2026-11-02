import { Film, FolderOpen, Library, Loader2, Server, Tv } from "lucide-react";
import { useEffect, useState } from "react";
import { api, type Library as Lib, useLoad } from "../../api";
import { FolderPicker } from "../../components/FolderPicker";
import { useApp } from "../../context";
import { Alert, Button, cx, Input, Toggle } from "../../ui";
import { Step } from "./Step";

const kindIcon = { movies: Film, tvshows: Tv, mixed: Library } as Record<string, typeof Film>;

export function LibrariesStep({ done, locked }: { done: boolean; locked: boolean }) {
  const { cfg, patch } = useApp();
  const [open, setOpen] = useState(!done);
  const jf = useLoad(() => (locked ? Promise.resolve([]) : api.jellyfinLibraries()), [locked, cfg.jellyfin.url]);
  const stats = useLoad(api.libraries, [cfg.libraries]);
  const [rows, setRows] = useState<Record<string, { on: boolean; path: string; name: string }>>({});
  const [picking, setPicking] = useState<string>();
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();

  // One row per Jellyfin folder; followed ones come from the config.
  useEffect(() => {
    const next: typeof rows = {};
    for (const l of jf.data ?? []) {
      for (const loc of l.locations) {
        const saved = cfg.libraries.find((c) => c.jellyfin === loc);
        next[loc] = { on: !!saved, path: saved?.path ?? loc, name: l.name };
      }
    }
    setRows(next);
  }, [jf.data]);

  const save = async () => {
    setSaving(true);
    setError(undefined);
    try {
      const libraries: Lib[] = Object.entries(rows)
        .filter(([, r]) => r.on)
        .map(([jellyfin, r]) => ({ name: r.name, jellyfin, path: r.path.trim() }));
      await patch({ libraries });
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const followed = cfg.libraries.map((l) => l.name);
  const dirty = JSON.stringify(
    Object.entries(rows).filter(([, r]) => r.on).map(([j, r]) => [j, r.path.trim()]),
  ) !== JSON.stringify(cfg.libraries.map((l) => [l.jellyfin, l.path]));

  return (
    <Step
      n={4}
      title="Libraries"
      done={done}
      locked={locked}
      open={open}
      onToggle={() => setOpen(!open)}
      summary={done ? [...new Set(followed)].join(", ") : "Pick the films and shows to analyze"}
    >
      <p className="mb-4 text-sm text-zinc-500">
        Jellyfin and lueur may see the same files under different paths (different containers or machines). For each
        folder, choose where lueur finds it.
      </p>
      {jf.error && <Alert>{jf.error}</Alert>}
      {jf.loading && !jf.data && <Loader2 className="size-4 animate-spin text-zinc-500" />}
      {jf.data && !jf.data.length && <p className="text-sm text-zinc-500">No movie or show library on this Jellyfin server.</p>}

      <ul className="space-y-2">
        {jf.data?.flatMap((l) =>
          l.locations.map((loc) => {
            const r = rows[loc];
            if (!r) return null;
            const Icon = kindIcon[l.kind] ?? Library;
            const st = stats.data?.find((s) => s.jellyfin === loc && s.path === r.path.trim());
            return (
              <li
                key={loc}
                className={cx(
                  "rounded-xl border p-4 transition",
                  r.on ? "border-white/10 bg-white/[0.03]" : "border-white/[0.05]",
                )}
              >
                <div className="flex items-center gap-3">
                  <Icon className="size-4 shrink-0 text-zinc-400" />
                  <div className="min-w-0 flex-1">
                    <p className="text-sm font-medium">{l.name}</p>
                    <p className="truncate font-mono text-xs text-zinc-500" title={loc}>
                      <Server className="mr-1 inline size-3 align-[-1px]" />
                      Jellyfin: {loc}
                    </p>
                  </div>
                  <Toggle label={`Follow ${l.name}`} checked={r.on} onChange={(on) => setRows({ ...rows, [loc]: { ...r, on } })} />
                </div>
                {r.on && (
                  <div className="mt-3 flex items-center gap-2 pl-7">
                    <Input
                      value={r.path}
                      onChange={(e) => setRows({ ...rows, [loc]: { ...r, path: e.target.value } })}
                      className="font-mono text-xs"
                      spellCheck={false}
                      aria-label={`Folder of ${l.name} for lueur`}
                    />
                    <Button onClick={() => setPicking(loc)} icon={<FolderOpen className="size-4" />}>
                      Browse
                    </Button>
                  </div>
                )}
                {r.on && st && (
                  <p className={cx("mt-2 pl-7 text-xs", st.exists ? "text-zinc-500" : "text-rose-300")}>
                    {st.exists ? `${st.videos} videos found, ${st.analyzed} analyzed` : "lueur can't see this folder"}
                  </p>
                )}
              </li>
            );
          }),
        )}
      </ul>

      {jf.data && jf.data.length > 0 && (
        <div className="mt-5 flex flex-wrap items-center justify-between gap-4 border-t border-white/[0.06] pt-5">
          <p className="text-xs text-zinc-500">How and when they are analyzed: see Analysis below.</p>
          <Button variant="primary" onClick={save} loading={saving} disabled={!dirty}>
            Save libraries
          </Button>
        </div>
      )}
      {error && <div className="mt-4"><Alert>{error}</Alert></div>}

      {picking && rows[picking] && (
        <FolderPicker
          start={rows[picking].path}
          onClose={() => setPicking(undefined)}
          onPick={(path) => setRows({ ...rows, [picking]: { ...rows[picking], path } })}
        />
      )}
    </Step>
  );
}
