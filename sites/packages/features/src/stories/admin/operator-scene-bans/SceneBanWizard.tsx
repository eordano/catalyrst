import { useCallback, useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import SitesChrome from "@ui/web/frames/SitesChrome";
import OpSceneBansPage from "@ui/operator/pages/OpSceneBansPage";

import { useAuth } from "@data/lib/auth/index";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  commitSceneBan,
  type BanRow,
  type PlaceRef,
} from "@data/lib/catalyst/admin/scene-bans";
import {
  sceneBanMachine,
  resolveSceneBanSnapshot,
  slugToState,
  stateToSlug,
  type CommitFn,
  type SceneBanStateId,
  type TrackFn,
} from "./machine";

export type SceneBanWizardProps = {
  trackCtx: TrackContext;
  places: PlaceRef[];
  owner?: string | null;
  selectedPlaceId: string | null;
  banRows: BanRow[];
  banTotal: number;
  initialStep?: string;
  commit?: CommitFn;
  track?: TrackFn;
};

export default function SceneBanWizard(props: SceneBanWizardProps) {
  const { selectedPlaceId, initialStep } = props;
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const urlPlace = searchParams.get("place")?.trim() || selectedPlaceId || "";
  const stateId = slugToState(urlStep);

  return (
    <SitesChrome active="create">
      <SceneBanInner
        key={`${urlPlace}|${stateId}`}
        {...props}
        urlPlace={urlPlace}
        stateId={stateId}
      />
    </SitesChrome>
  );
}

type InnerProps = SceneBanWizardProps & {
  urlPlace: string;
  stateId: SceneBanStateId;
};

function SceneBanInner({
  trackCtx,
  places,
  owner,
  urlPlace,
  stateId,
  banRows,
  banTotal,
  commit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const auth = useAuth();

  const liveCommit = useCallback<CommitFn>(
    ({ placeId, action, address, signal }) => {
      if (!auth.identity) {
        return Promise.reject(
          new Error("Connect your wallet to commit this change."),
        );
      }
      return commitSceneBan({
        identity: auth.identity,
        placeId,
        action,
        address,
        signal,
      });
    },
    [auth.identity],
  );
  const resolvedCommit = commit ?? liveCommit;

  const snapshot = useRef(
    resolveSceneBanSnapshot({
      step: stateId,
      trackCtx,
      placeId: urlPlace,
      total: banTotal,
      commit: resolvedCommit,
      track,
    }),
  ).current;

  const [state, send] = useMachine(sceneBanMachine, {
    input: {
      trackCtx,
      placeId: urlPlace,
      total: banTotal,
      commit: resolvedCommit,
      track,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${urlPlace}|${step}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        let changed = false;
        if (params.get("step") !== step) {
          params.set("step", step);
          changed = true;
        }
        if (urlPlace && params.get("place") !== urlPlace) {
          params.set("place", urlPlace);
          changed = true;
        }
        return changed ? params : prev;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, urlPlace, setSearchParams]);

  function pickPlace(placeId: string) {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("place", placeId);
        params.set("step", "bans");
        return params;
      },
      { preventScrollReset: true },
    );
  }

  return (
    <OpSceneBansPage
      step={step}
      view={value}
      places={places}
      owner={owner}
      urlPlace={urlPlace}
      placeId={ctx.placeId ?? null}
      action={ctx.action ?? null}
      address={ctx.address ?? null}
      error={ctx.error ?? null}
      result={ctx.result ?? null}
      banRows={banRows.map((r) => ({ ...r, name: r.name ?? "" }))}
      banTotal={banTotal}
      onSelectPlace={(id) => {
        if (id === urlPlace && value === "pickPlace") {
          send({ type: "PICK_PLACE" });
        } else {
          pickPlace(id);
        }
      }}
      onChangePlace={() => pickPlace("")}
      onBan={(addr) => send({ type: "START_BAN", address: addr })}
      onUnban={(addr) => send({ type: "START_UNBAN", address: addr })}
      onBack={() => send({ type: "BACK" })}
      onReview={() => send({ type: "REVIEW" })}
      onSubmit={() => send({ type: "SUBMIT" })}
      onReset={() => send({ type: "RESET" })}
    />
  );
}
