import { useCallback, useEffect, useRef, useState } from "react";

export type Look = {
  brightness: number;
  saturation: number;
  min_brightness: number;
  smoothing_ms: number;
  offset_ms: number;
  spread: number;
  fit_area: boolean;
};

export type Library = { name: string; jellyfin: string; path: string };

export type Config = {
  store: string;
  hue: { bridge: string; area?: string | null };
  jellyfin: { url: string; device?: string | null };
  libraries: Library[];
  look: Look;
  auto_analyze: boolean;
  auto_scan_minutes: number;
  jobs: number;
  paired: boolean;
  has_api_key: boolean;
};

export type LiveChannel = { id: number; x: number; y: number; z: number; color: [number, number, number] };

export type Status = {
  player: {
    error: string | null;
    jellyfin_error: string | null;
    area: string | null;
    streaming: boolean;
    channels: LiveChannel[];
    playing: {
      title: string;
      device: string;
      position: number;
      duration: number | null;
      paused: boolean;
      has_track: boolean;
    } | null;
  };
  queue: {
    jobs: number;
    running: { name: string; progress: number | null; cpu: boolean }[];
    pending: number;
    next: string[];
    done: number;
    failed: { name: string; error: string }[];
  };
  last_scan: LastScan | null;
  hold: { path: string; t: number } | null;
};

export type LastScan = { at: number; auto: boolean; queued: number; removed: number };

export type Bridge = { ip: string; id: string | null; name: string | null; model: string | null };
export type Area = {
  id: string;
  metadata: { name: string };
  channels: { channel_id: number; position: { x: number; y: number; z: number } }[];
  status: string;
  configuration_type: string;
};
export type JellyfinLibrary = { name: string; kind: string; locations: string[] };
export type LibraryStats = Library & { exists: boolean; videos: number; analyzed: number };
export type Video = {
  library: string;
  name: string;
  folder: string;
  path: string;
  analyzed: boolean;
  duration: number | null;
  state: "running" | "queued" | null;
};
export type Dir = { path: string; parent: string | null; dirs: string[] };

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const r = await fetch(`/api/${path}`, {
    method,
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(data.error ?? `${r.status} ${r.statusText}`);
  return data as T;
}

export const api = {
  config: () => call<Config>("GET", "config"),
  patch: (p: unknown) => call<Config>("PATCH", "config", p),
  status: () => call<Status>("GET", "status"),
  bridges: () => call<Bridge[]>("GET", "bridges"),
  pair: (host: string) => call<{ paired: boolean }>("POST", "bridge/pair", { host }),
  areas: () => call<Area[]>("GET", "areas"),
  test: (channel?: number) => call<object>("POST", "test", { channel }),
  jellyfinInfo: () => call<{ name: string; version: string }>("GET", "jellyfin/info"),
  jellyfinDevices: () => call<string[]>("GET", "jellyfin/devices"),
  jellyfinLibraries: () => call<JellyfinLibrary[]>("GET", "jellyfin/libraries"),
  libraries: () => call<LibraryStats[]>("GET", "libraries"),
  scan: () => call<LastScan>("POST", "scan"),
  stop: () => call<object>("POST", "scan/stop"),
  videos: () => call<Video[]>("GET", "videos"),
  frameUrl: (path: string, t: number) => `/api/frame?path=${encodeURIComponent(path)}&t=${t.toFixed(1)}`,
  hold: (path: string, t: number) => call<object>("PUT", "hold", { path, t }),
  unhold: () => call<object>("DELETE", "hold"),
  analyze: (path: string) => call<{ queued: boolean }>("POST", "analyze", { path }),
  fs: (path: string) => call<Dir>("GET", `fs?path=${encodeURIComponent(path)}`),
};

/** Loads once (and on `reload()`), keeping the last good value on errors. */
export function useLoad<T>(fn: () => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string>();
  const [loading, setLoading] = useState(true);
  const reload = useCallback(() => {
    setLoading(true);
    fn()
      .then((d) => {
        setData(d);
        setError(undefined);
      })
      .catch((e: Error) => setError(e.message))
      .finally(() => setLoading(false));
  }, deps);
  useEffect(reload, [reload]);
  return { data, error, loading, reload, setData };
}

/** Polls `fn` every `ms` while the tab is visible. */
export function usePoll<T>(fn: () => Promise<T>, ms: number) {
  const [data, setData] = useState<T>();
  const [error, setError] = useState<string>();
  const fnRef = useRef(fn);
  fnRef.current = fn;
  useEffect(() => {
    let alive = true;
    let timer: number;
    const tick = async () => {
      if (!document.hidden) {
        try {
          const d = await fnRef.current();
          if (alive) {
            setData(d);
            setError(undefined);
          }
        } catch (e) {
          if (alive) setError((e as Error).message);
        }
      }
      if (alive) timer = window.setTimeout(tick, ms);
    };
    tick();
    return () => {
      alive = false;
      clearTimeout(timer);
    };
  }, [ms]);
  return { data, error };
}
