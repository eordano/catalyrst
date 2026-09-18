import { useQuery } from "@tanstack/react-query";

import {
  SCENE_OWNER_STALE_MS,
  fetchHomeRealm,
  fetchSceneDeployment,
  fetchWorldSceneDeployment,
  fetchLandOwner,
  sceneRecipients,
  type SceneRecipient,
  worldRealmBase,
  isHomeRealm,
  sceneOwnerKeys,
} from "../../data/catalyst/sceneOwner";
import { catalystBase } from "../../data/catalyst/client";
const COORDS_RE = /^\s*(-?\d+)\s*,\s*(-?\d+)\s*$/;

export function normalizeParcel(coords: string | null | undefined): string | null {
  if (typeof coords !== "string") return null;
  const m = coords.match(COORDS_RE);
  return m ? `${Number(m[1])},${Number(m[2])}` : null;
}

export type SceneOwner = {
  address: string | null;
  sceneTitle: string | null;
  loading: boolean;
  world: boolean;
  recipients: SceneRecipient[];
  tipAddress: string | null;
};

export function useSceneOwner(
  coords: string | null | undefined,
  realm: string | null | undefined,
  enabled = true,
): SceneOwner {
  const parcel = normalizeParcel(coords);
  const realmKnown = typeof realm === "string" && realm.trim() !== "";
  const wanted = enabled && parcel != null && realmKnown;
  const home = useQuery({
    queryKey: sceneOwnerKeys.homeRealm(),
    queryFn: ({ signal }) => fetchHomeRealm({ signal }),
    staleTime: Infinity,
    enabled: wanted,
  });
  const land = home.data != null && isHomeRealm(realm, home.data);
  const launchRealm = typeof window === "undefined" ? null : new URLSearchParams(window.location.search).get("realm");
  const worldName = [realm, launchRealm].find(value => value && /^[\w.-]+\.eth$/i.test(value));
  const worldBase = (realm ? worldRealmBase(realm, launchRealm) : null) ?? (worldName ? `${catalystBase()}/world/${encodeURIComponent(worldName)}` : null);
  const worldOwner = useQuery({
    queryKey: ["world-scene-recipients", realm, worldBase, parcel],
    queryFn: ({ signal }) => fetchWorldSceneDeployment(realm ?? "", worldBase ?? "", parcel ?? "", { signal }),
    staleTime: SCENE_OWNER_STALE_MS,
    enabled: wanted && home.data != null && !land && worldBase != null,
  });
  const deployment = useQuery({
    queryKey: sceneOwnerKeys.deployment(parcel),
    queryFn: ({ signal }) => fetchSceneDeployment(parcel ?? "", { signal }),
    staleTime: SCENE_OWNER_STALE_MS,
    enabled: wanted && land,
  });
  const landOwner = useQuery({
    queryKey: ["scene-land-owner", parcel],
    queryFn: ({ signal }) => fetchLandOwner(parcel ?? "", { signal }),
    staleTime: SCENE_OWNER_STALE_MS,
    enabled: wanted && land,
    retry: false,
  });
  const world = realmKnown && home.data != null && !land;
  const data = land ? deployment.data : worldOwner.data;
  const recipients = sceneRecipients(data, landOwner.data, world);
  const address = data?.sceneAuthor ?? data?.deployer ?? data?.tipAddress ?? (land ? landOwner.data : null) ?? null;
  return {
    address,
    tipAddress: data?.tipAddress ?? address,
    recipients,
    sceneTitle: data?.title ?? null,
    loading: wanted && (home.isFetching || (land ? deployment.isFetching || landOwner.isFetching : worldOwner.isFetching)),
    world,
  };
}

export { SceneFeedbackModal, SceneTipModal } from "./SceneMessageModal";
export { FEEDBACK_MAX, feedbackHeader, composeFeedback } from "./sceneMessage";
