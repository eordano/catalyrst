import { useCallback, useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import OpManageAdminsView from "@ui/operator/pages/OpManageAdminsView";

import { useAuth } from "@data/lib/auth/index";
import {
  commitSceneAdmin,
  type OperatedPlace,
  type SceneAdminRow,
} from "@data/lib/catalyst/admin/scene-admins";
import type { TrackContext } from "@core/lib/telemetry/track";
import {
  manageMachine,
  resolveManageSnapshot,
  slugToState,
  stateToSlug,
  type CommitFn,
  type TrackFn,
} from "./machine";

export type ManageAdminsWizardProps = {
  trackCtx: TrackContext;
  places: OperatedPlace[];
  grants: SceneAdminRow[];
  selectedPlaceId: string | null;
  initialStep?: string;
  commit?: CommitFn;
  track?: TrackFn;
};

export default function ManageAdminsWizard(props: ManageAdminsWizardProps) {
  const [searchParams] = useSearchParams();
  const urlStep =
    (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <ManageAdminsWizardInner
      key={stateId + "|" + (props.selectedPlaceId ?? "")}
      stateId={stateId}
      {...props}
    />
  );
}

type InnerProps = ManageAdminsWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function ManageAdminsWizardInner({
  stateId,
  trackCtx,
  places,
  grants,
  selectedPlaceId,
  commit,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const auth = useAuth();

  const liveCommit = useCallback<CommitFn>(
    ({ action, placeId, admin, signal }) => {
      if (!auth.identity) {
        return Promise.reject(
          new Error("Connect your wallet to commit this change."),
        );
      }
      return commitSceneAdmin({
        identity: auth.identity,
        placeId,
        action,
        admin,
        signal,
      });
    },
    [auth.identity],
  );
  const resolvedCommit = commit ?? liveCommit;

  const snapshot = useRef(
    resolveManageSnapshot({
      step: stateId,
      trackCtx,
      placeId: selectedPlaceId ?? undefined,
      commit: resolvedCommit,
      track,
    }),
  ).current;

  const [state, send] = useMachine(manageMachine, {
    input: {
      trackCtx,
      placeId: selectedPlaceId ?? undefined,
      commit: resolvedCommit,
      track,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;

  const activePlaceId = ctx.placeId ?? selectedPlaceId ?? null;

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = step + "|" + (activePlaceId ?? "");
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
        if (activePlaceId && params.get("place") !== activePlaceId) {
          params.set("place", activePlaceId);
          changed = true;
        }
        return changed ? params : prev;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, activePlaceId, setSearchParams]);

  return (
    <OpManageAdminsView
      step={step}
      view={value}
      places={places.map((p) => ({ ...p, title: p.title ?? "" }))}
      grants={grants.map((g) => ({ ...g, name: g.name ?? "" }))}
      activePlaceId={activePlaceId}
      action={ctx.action ?? null}
      grantAddress={ctx.grantAddress ?? null}
      revokeAddress={ctx.revokeAddress ?? null}
      error={ctx.error ?? null}
      onSelectPlace={(placeId) => send({ type: "SELECT_PLACE", placeId })}
      onStartGrant={(address) => send({ type: "START_GRANT", address })}
      onStartRevoke={(admin, canBeRemoved) =>
        send({ type: "START_REVOKE", address: admin, canBeRemoved })
      }
      onReview={() => send({ type: "REVIEW" })}
      onBack={() => send({ type: "BACK" })}
      onSubmit={() => send({ type: "SUBMIT" })}
      onDoneBack={() => send({ type: "DONE_BACK" })}
    />
  );
}
