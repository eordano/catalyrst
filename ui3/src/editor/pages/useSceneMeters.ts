import type { RefObject } from "react";
import { useEffect, useState } from "react";
import type { EditorBus } from "../editor-bus";
import type { LiveSceneInfo } from "../../generated/editor-bus";
import type { RibbonMeter } from "../components/DeRibbon";

const POLL_MS = 5000;
const ENTITIES = /^entities:\s*(\d+)$/m;

/** The deployment ceiling every explorer enforces, from the parcel count. */
function entityLimit(parcels: number): number {
  if (parcels <= 0) return 0;
  return Math.floor(Math.log2(parcels + 1) * 200);
}

interface Options {
  busRef: RefObject<EditorBus | null>;
  busLive: boolean;
  scene: LiveSceneInfo | null;
}

// /scene_stats is the only console command in the engine that reports a scene
// budget, and all it reports is the entity count. Triangles, materials and
// bytes have no source anywhere in the bevy crates, so they get no meter rather
// than a plausible-looking number.
export function useSceneMeters({ busRef, busLive, scene }: Options): RibbonMeter[] {
  const [entities, setEntities] = useState<number | null>(null);
  const parcels = scene?.parcels?.length ?? 0;

  useEffect(() => {
    if (!busLive) {
      setEntities(null);
      return undefined;
    }
    let gone = false;
    const read = () => {
      busRef.current
        ?.rpc("sceneStats")
        .then((res) => {
          if (gone) return;
          const m = ENTITIES.exec(String(res ?? ""));
          setEntities(m ? Number(m[1]) : null);
        })
        .catch(() => {
          if (!gone) setEntities(null);
        });
    };
    read();
    const id = setInterval(read, POLL_MS);
    return () => {
      gone = true;
      clearInterval(id);
    };
  }, [busLive, busRef]);

  if (entities === null || parcels <= 0) return [];
  return [{ label: "entities", value: entities, limit: entityLimit(parcels) }];
}
