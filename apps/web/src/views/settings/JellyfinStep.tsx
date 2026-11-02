import { Check, Loader2 } from "lucide-react";
import { useState } from "react";
import { api, useLoad } from "../../api";
import { useApp } from "../../context";
import { Alert, Button, Field, Input, Select } from "../../ui";
import { Step } from "./Step";

export function JellyfinStep({ done }: { done: boolean }) {
  const { cfg, patch } = useApp();
  const [open, setOpen] = useState(!done);
  const [url, setUrl] = useState(cfg.jellyfin.url);
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string>();
  const info = useLoad(() => (done ? api.jellyfinInfo() : Promise.resolve(undefined)), [done, cfg.jellyfin.url]);
  const devices = useLoad(() => (info.data ? api.jellyfinDevices() : Promise.resolve([])), [info.data]);

  const save = async () => {
    setSaving(true);
    setError(undefined);
    try {
      await patch({ jellyfin: { url: url.trim().replace(/\/$/, ""), api_key: key.trim() } });
      setKey("");
      info.reload();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  return (
    <Step
      n={3}
      title="Jellyfin"
      done={done}
      open={open}
      onToggle={() => setOpen(!open)}
      summary={info.data ? `${info.data.name} · Jellyfin ${info.data.version}` : done ? cfg.jellyfin.url : "Where lueur learns what's playing"}
    >
      <div className="grid gap-4 sm:grid-cols-2">
        <Field label="Server URL">
          <Input value={url} onChange={(e) => setUrl(e.target.value)} placeholder="http://192.168.1.10:8096" spellCheck={false} />
        </Field>
        <Field label="API key" hint="Dashboard → API keys. Must be an admin key, to read file paths.">
          <Input
            type="password"
            value={key}
            onChange={(e) => setKey(e.target.value)}
            placeholder={cfg.has_api_key ? "Saved, leave empty to keep" : "Paste the key"}
          />
        </Field>
      </div>
      <div className="mt-4 flex flex-wrap items-center gap-3">
        <Button variant="primary" onClick={save} loading={saving} disabled={!url.trim() || (!key.trim() && !cfg.has_api_key)}>
          Save and test
        </Button>
        {info.loading && done && <Loader2 className="size-4 animate-spin text-zinc-500" />}
        {info.data && (
          <span className="flex items-center gap-1.5 text-sm text-emerald-300">
            <Check className="size-4" /> Connected to {info.data.name}
          </span>
        )}
      </div>
      {(error || (done && info.error)) && <div className="mt-4"><Alert>{error ?? info.error}</Alert></div>}

      {info.data && (
        <div className="mt-6 border-t border-white/[0.06] pt-5">
          <Field label="Follow one device only" hint="Useful when several people watch Jellyfin at home.">
            <Select
              value={cfg.jellyfin.device ?? ""}
              onChange={(e) => patch({ jellyfin: { device: e.target.value || null } })}
              className="max-w-sm"
            >
              <option value="">Any device</option>
              {[...new Set([...(devices.data ?? []), ...(cfg.jellyfin.device ? [cfg.jellyfin.device] : [])])].map((d) => (
                <option key={d} value={d}>{d}</option>
              ))}
            </Select>
          </Field>
        </div>
      )}
    </Step>
  );
}

/* ------------------------------------------------------------- Libraries */
