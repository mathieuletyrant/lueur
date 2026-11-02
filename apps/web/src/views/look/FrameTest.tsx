import { Lightbulb, Square } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, useLoad } from "../../api";
import { AreaView } from "../../components/AreaView";
import { Barcode } from "../../components/Barcode";
import { useApp } from "../../context";
import { Alert, Button, Card, CardHeader, Select, Slider, time } from "../../ui";

/** Hold a moment of a film on the lights, to tune the Look in front of them. */
export function FrameTest() {
  const { status, cfg } = useApp();
  const videos = useLoad(api.videos);
  const analyzed = useMemo(() => (videos.data ?? []).filter((v) => v.duration), [videos.data]);
  const [path, setPath] = useState("");
  const [t, setT] = useState(0);
  const [shown, setShown] = useState(0); // the time of the frame on screen (debounced)
  const [error, setError] = useState<string>();
  const holding = !!status?.hold;
  const video = analyzed.find((v) => v.path === path);
  const p = status?.player;

  // Default: the first analyzed video, a quarter in.
  useEffect(() => {
    if (!path && analyzed.length) {
      setPath(analyzed[0].path);
      const t0 = Math.round((analyzed[0].duration ?? 0) / 4);
      setT(t0);
      setShown(t0);
    }
  }, [analyzed]);

  // Debounce: one frame (and one move of the lights) per pause of the slider.
  useEffect(() => {
    const id = setTimeout(() => {
      setShown(t);
      if (holding && path) api.hold(path, t).catch((e: Error) => setError(e.message));
    }, 250);
    return () => clearTimeout(id);
  }, [t, path]);

  // Leaving the page gives the lights back.
  const holdingRef = useRef(holding);
  holdingRef.current = holding;
  useEffect(() => () => void (holdingRef.current && api.unhold()), []);

  const toggle = () => {
    setError(undefined);
    (holding ? api.unhold() : api.hold(path, t)).catch((e: Error) => setError(e.message));
  };

  if (videos.data && !analyzed.length) return null;

  return (
    <Card className="mb-5">
      <CardHeader
        title="Test on a frame"
        subtitle="Hold a moment of a film on your lights, then tune the settings below while looking at the room."
        action={
          <Button
            variant={holding ? "secondary" : "primary"}
            onClick={toggle}
            disabled={!video || !cfg.paired || (!!p?.playing?.has_track && !holding)}
            icon={holding ? <Square className="size-3.5" /> : <Lightbulb className="size-4" />}
          >
            {holding ? "Stop" : "Show on lights"}
          </Button>
        }
      />
      <div className="grid gap-5 px-5 pt-4 pb-5 lg:grid-cols-5">
        <div className="space-y-4 lg:col-span-2">
          <label className="block">
            <span className="mb-1.5 block text-xs font-medium text-zinc-400">Film or episode</span>
            <Select
              value={path}
              onChange={(e) => {
                const v = analyzed.find((x) => x.path === e.target.value);
                setPath(e.target.value);
                setT(Math.round((v?.duration ?? 0) / 4));
              }}
            >
              {[...new Set(analyzed.map((v) => v.library))].map((lib) => (
                <optgroup key={lib} label={lib}>
                  {analyzed
                    .filter((v) => v.library === lib)
                    .map((v) => (
                      <option key={v.path} value={v.path}>
                        {v.folder && v.folder !== v.name ? `${v.folder} · ${v.name}` : v.name}
                      </option>
                    ))}
                </optgroup>
              ))}
            </Select>
          </label>
          {video?.duration && <Barcode path={video.path} duration={video.duration} at={t / video.duration} className="h-3" />}
          <Slider
            id="frame-t"
            label="Moment"
            display={
              <>
                {time(t)} <span className="text-zinc-600">/ {time(video?.duration ?? 0)}</span>
              </>
            }
            value={t}
            min={0}
            max={Math.max(1, Math.floor(video?.duration ?? 1))}
            step={1}
            onChange={setT}
            hint={holding ? "The lights follow the slider." : "Pick a scene with strong colors, then show it on the lights."}
          />
          {error && <Alert>{error}</Alert>}
        </div>
        <div className="lg:col-span-3">
          <AreaView
            screen={video ? api.frameUrl(video.path, shown) : undefined}
            channels={holding ? (p?.channels ?? []) : (p?.channels ?? []).map((c) => ({ ...c, color: undefined }))}
          />
        </div>
      </div>
    </Card>
  );
}
