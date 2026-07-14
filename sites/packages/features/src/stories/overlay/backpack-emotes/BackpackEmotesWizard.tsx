import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import BackpackEmotesView from "@ui/overlay/workflows/BackpackEmotesView";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  emotesMachine,
  resolveEmotesSnapshot,
  resolveStep,
  stateToSlug,
  type EmotesStateId,
  type SaveFn,
  type SlotBinding,
  type TrackFn,
} from "./machine";

export type EmoteOption = {
  urn: string;
  name: string;
  description: string | null;
  thumbnail: string | null;
  rarity: string | null;
  category: string;
  loop: boolean;
};

export type BackpackEmotesWizardProps = {
  trackCtx: TrackContext;
  profile?: string;
  catalog: EmoteOption[];
  loadout: SlotBinding[];
  slotOrder: number[];
  liveEmpty: boolean;
  initialStep?: string;
  initialSlot?: number | null;
  initialUrn?: string | null;
  save?: SaveFn;
  track?: TrackFn;
};

export default function BackpackEmotesWizard(props: BackpackEmotesWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const rawSlot = searchParams.get("slot") ?? (props.initialSlot ?? null)?.toString();
  const slot = rawSlot != null && rawSlot !== "" ? Number(rawSlot) : null;
  const urn = searchParams.get("urn") ?? props.initialUrn ?? null;

  const stateId = resolveStep(urlStep, Number.isNaN(slot) ? null : slot);

  const key = `${stateId}:${slot ?? ""}:${urn ?? ""}`;

  return (
    <BackpackEmotesInner
      key={key}
      stateId={stateId}
      slot={Number.isNaN(slot) ? null : slot}
      urn={urn}
      {...props}
    />
  );
}

type InnerProps = BackpackEmotesWizardProps & {
  stateId: EmotesStateId;
  slot: number | null;
  urn: string | null;
};

function BackpackEmotesInner({
  stateId,
  slot,
  urn,
  trackCtx,
  profile,
  catalog,
  loadout,
  slotOrder,
  liveEmpty,
  save,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const stagedName =
    urn != null ? catalog.find((e) => e.urn === urn)?.name : undefined;

  const snapshot = useRef(
    resolveEmotesSnapshot({
      step: stateId,
      trackCtx,
      loadout,
      save,
      track,
      slot: slot ?? undefined,
      urn: urn ?? undefined,
      name: stagedName,
    }),
  ).current;

  const [state, send] = useMachine(emotesMachine, {
    input: { trackCtx, loadout, save, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctx = state.context;
  const activeSlot = ctx.activeSlot;

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const slotParam = activeSlot != null ? String(activeSlot) : "";
    const urnParam = ctx.pendingUrn ?? "";
    const k = `${step}:${slotParam}:${urnParam}`;
    if (lastKey.current === k) return;
    lastKey.current = k;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "backpack");
        params.set("tab", "emotes");
        if (step === "open" || step === "pick") params.delete("step");
        else params.set("step", step);
        if (slotParam) params.set("slot", slotParam);
        else params.delete("slot");
        if (urnParam) params.set("urn", urnParam);
        else params.delete("urn");
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, activeSlot, ctx.pendingUrn, setSearchParams]);

  return (
    <BackpackEmotesView
      value={value}
      step={step}
      profile={profile}
      activeSlot={activeSlot}
      pendingUrn={ctx.pendingUrn}
      loadout={ctx.loadout}
      slotOrder={slotOrder}
      catalog={catalog}
      liveEmpty={liveEmpty}
      error={ctx.error}
      result={ctx.result}
      onOpen={() => send({ type: "OPEN" })}
      onPickSlot={(slot) => send({ type: "PICK_SLOT", slot })}
      onReview={() => send({ type: "REVIEW" })}
      onAssign={(urn, name) => send({ type: "ASSIGN", urn, name })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onBack={() => send({ type: "BACK" })}
      onSave={() => send({ type: "SAVE" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
