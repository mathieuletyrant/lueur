import { Rainbow, Tv } from "lucide-react";
import { useState } from "react";
import { api } from "../../api";
import { AreaView } from "../../components/AreaView";
import { useApp } from "../../context";
import { Alert, Button, Card, CardHeader, Empty } from "../../ui";

export function AreaCard() {
  const { status, cfg } = useApp();
  const p = status?.player;
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<string>();

  const test = async () => {
    setTesting(true);
    setError(undefined);
    try {
      await api.test();
      await new Promise((r) => setTimeout(r, 6000));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setTesting(false);
    }
  };

  return (
    <Card className="lg:col-span-3">
      <CardHeader
        title={p?.area ?? "Entertainment area"}
        subtitle={p?.channels.length ? `${p.channels.length} channels` : "Not connected"}
        action={
          <Button onClick={test} loading={testing} disabled={!cfg.paired || p?.streaming} icon={<Rainbow className="size-4" />}>
            Test
          </Button>
        }
      />
      <div className="px-5 pt-2 pb-5">
        {p?.channels.length ? (
          <>
            <AreaView channels={p.channels} onChannel={p.streaming ? undefined : (id) => api.test(id).catch((e: Error) => setError(e.message))} />
            {!p.streaming && <p className="mt-2 text-center text-xs text-zinc-500">Click a light to make it blink.</p>}
          </>
        ) : (
          <Empty icon={<Tv className="size-4" />} title="No area yet">
            Pair a bridge and pick an entertainment area in Setup.
          </Empty>
        )}
        {error && <Alert>{error}</Alert>}
      </div>
    </Card>
  );
}
