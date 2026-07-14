import { useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MkTransferWizardView from "@ui/marketplace/workflows/MkTransferWizardView";
import type { MarketplaceNavId } from "@ui/marketplace/frames/MarketplaceChrome";

import type { TrackContext } from "@core/lib/telemetry/track";
import { looksLikeAddress, shortenHex, type TransferAsset } from "@data/lib/catalyst/marketplace/transfer";
import {
  transferMachine,
  resolveTransferSnapshot,
  slugToState,
  stateToSlug,
  type TransferFn,
  type TransferTarget,
  type TrackFn,
} from "./machine";

export type TransferWizardProps = {
  assets: TransferAsset[];
  source: "catalyst" | "fixture";
  trackCtx: TrackContext;
  initialStep?: string;
  transfer?: TransferFn;
  track?: TrackFn;
};

function toTarget(a: TransferAsset): TransferTarget {
  return { id: a.id, name: a.name, category: a.category, rarity: a.rarity, network: a.network, image: a.image };
}

export default function TransferWizard(props: TransferWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return <TransferWizardInner key={stateId} stateId={stateId} {...props} />;
}

type InnerProps = TransferWizardProps & {
  stateId: ReturnType<typeof slugToState>;
};

function TransferWizardInner({
  stateId,
  assets,
  source,
  trackCtx,
  transfer,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const firstAsset = assets[0];

  const snapshot = useRef(
    resolveTransferSnapshot({
      step: stateId,
      trackCtx,
      transfer,
      track,
      asset: firstAsset ? toTarget(firstAsset) : undefined,
    }),
  ).current;

  const [state, send] = useMachine(transferMachine, {
    input: { trackCtx, transfer, track },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const [recipient, setRecipient] = useState(state.context.recipient ?? "");
  const [tab, setTab] = useState<MarketplaceNavId>("collectibles");
  const recipientInvalid =
    recipient.trim().length > 0 && !looksLikeAddress(recipient);

  const activeAsset: TransferTarget | undefined =
    state.context.asset ?? (firstAsset ? toTarget(firstAsset) : undefined);

  return (
    <MkTransferWizardView
      value={value}
      step={step}
      source={source}
      assets={assets.map(toTarget)}
      activeAsset={activeAsset}
      tab={tab}
      onTab={setTab}
      recipient={recipient}
      recipientInvalid={recipientInvalid}
      recipientShort={shortenHex(state.context.recipient)}
      txHashShort={shortenHex(state.context.result?.txHash ?? "")}
      error={state.context.error}
      onRecipientChange={setRecipient}
      onSelectAsset={(asset) => send({ type: "SELECT_ASSET", asset })}
      onSubmitRecipient={() => send({ type: "SUBMIT_RECIPIENT", recipient })}
      onBack={() => send({ type: "BACK" })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onApprove={() => send({ type: "APPROVE" })}
      onExit={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
