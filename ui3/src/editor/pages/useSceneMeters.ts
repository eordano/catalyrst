import type { RefObject } from "react";
import { useEffect, useState } from "react";
import type { EditorBus } from "../editor-bus";
import type { LiveSceneInfo } from "../../generated/editor-bus";
import type { RibbonMeter } from "../components/DeRibbon";

const POLL_MS = 5000;
const ENTITIES = /^entities:\s*(\d+)$/m;

function entityLimit(parcels: number): number {
  if (parcels <= 0) return 0;
  return Math.floor(Math.log2(parcels + 1) * 200);
}

interface Options {
  busRef: RefObject<EditorBus | null>;
  busLive: boolean;
  scene: LiveSceneInfo | null;
}

export function useSceneMeters({ busRef, busLive, scene }: Options): RibbonMeter[] {
  const [entities, setEntities] = useState<number | null>(null);
  const parcels = scene?.parcels?.length ?? 0;

  useEffect(() => {
    if (!busLive) {
      setEntities(null);
      return undefined;
    }
    let gone = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const schedule = () => {
      if (!gone) timer = setTimeout(read, POLL_MS);
    };
    const read = () => {
      if (gone) return;
      const bus = busRef.current;
      if (!bus) {
        setEntities(null);
        schedule();
        return;
      }
      void bus.rpc("sceneStats")
        .then((res) => {
          if (gone || busRef.current !== bus) return;
          const m = ENTITIES.exec(String(res ?? ""));
          setEntities(m ? Number(m[1]) : null);
        })
        .catch(() => {
          if (!gone && busRef.current === bus) setEntities(null);
        })
        .finally(schedule);
    };
    read();
    return () => {
      gone = true;
      if (timer !== null) clearTimeout(timer);
    };
  }, [busLive, busRef]);

  if (entities === null || parcels <= 0) return [];
  return [{ label: "entities", value: entities, limit: entityLimit(parcels) }];
}
