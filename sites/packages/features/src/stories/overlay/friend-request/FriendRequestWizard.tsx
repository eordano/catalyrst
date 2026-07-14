import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import FriendRequestView from "@ui/overlay/panels/FriendRequestView";

import { track, type TrackContext } from "@core/lib/telemetry/track";
import { sendBridge } from "../../../components/bevy-overlay/bridge";
import {
  friendMachine,
  resolveFriendSnapshot,
  parseAction,
  FRIEND_EVENTS,
  type FriendAction,
  type FriendStateId,
  type UpsertFn,
  type TrackFn,
} from "./machine";

export type Candidate = {
  address: string;
  name: string;
  mutualCount?: number;
  friendshipStatus?: string;
};

export type FriendRequestWizardProps = {
  trackCtx: TrackContext;
  candidate: Candidate;
  initialAction?: string;
  initialAddress?: string;
  tab?: string;
  upsert?: UpsertFn;
  trackFn?: TrackFn;
};

export default function FriendRequestWizard({
  trackCtx,
  candidate,
  initialAction,
  initialAddress,
  tab,
  upsert,
  trackFn,
}: FriendRequestWizardProps) {
  const [searchParams] = useSearchParams();

  const rawAction = searchParams.get("action") ?? initialAction ?? null;
  const { step, action } = parseAction(rawAction);
  const address = searchParams.get("address") ?? initialAddress ?? candidate.address;

  return (
    <FriendRequestInner
      key={`${step}:${action ?? ""}:${address}`}
      step={step}
      action={action}
      address={address}
      candidate={candidate}
      tab={tab}
      trackCtx={trackCtx}
      upsert={upsert}
      trackFn={trackFn}
    />
  );
}

type InnerProps = {
  step: FriendStateId;
  action?: FriendAction;
  address: string;
  candidate: Candidate;
  tab?: string;
  trackCtx: TrackContext;
  upsert?: UpsertFn;
  trackFn?: TrackFn;
};

function FriendRequestInner({
  step,
  action,
  address,
  candidate,
  tab,
  trackCtx,
  upsert,
  trackFn,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveFriendSnapshot({ step, trackCtx, upsert, track: trackFn, action, address }),
  ).current;

  const [state, send] = useMachine(friendMachine, {
    input: { trackCtx, upsert, track: trackFn, action, address },
    snapshot,
  });

  const value = state.value as string;
  const ctxAction = (state.context.action ?? action ?? "request") as FriendAction;
  const ctxAddress = state.context.address ?? address;

  const lastAction = useRef<string | null>(null);
  useEffect(() => {
    const next = stateToActionParam(value, ctxAction);
    if (lastAction.current === next) return;
    lastAction.current = next;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "friends");
        if (next) {
          params.set("action", next);
          if (ctxAddress) params.set("address", ctxAddress);
        } else {
          params.delete("action");
          params.delete("address");
        }
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [value, ctxAction, ctxAddress, setSearchParams]);

  function confirm() {
    if (ctxAction === "request" && ctxAddress) {
      sendBridge("friends.request", { address: ctxAddress });
    }
    send({ type: "CONFIRM" });
  }

  return (
    <FriendRequestView
      value={value}
      action={ctxAction}
      candidate={candidate}
      tab={tab}
      error={state.context.error}
      onStart={(a: FriendAction) =>
        send({ type: "START", action: a, address: candidate.address })
      }
      onCancel={() => send({ type: "CANCEL" })}
      onConfirm={confirm}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}

function stateToActionParam(value: string, action: FriendAction): string | null {
  switch (value) {
    case "confirming":
      return action === "request" ? "add" : action;
    case "blockPrompt":
      return "block";
    case "done":
      return "done";
    case "submitting":
      return action === "request" ? "add" : action;
    case "failed":
      return action === "request" ? "add" : action;
    default:
      return null;
  }
}

export { FRIEND_EVENTS };
export { track };
