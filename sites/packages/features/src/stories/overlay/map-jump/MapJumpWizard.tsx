import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import MapJumpView from "@ui/overlay/workflows/MapJumpView";

import { track as defaultTrack, type TrackContext } from "@core/lib/telemetry/track";
import {
  PIN_CATEGORIES,
  buildJumpUrl,
  filterPins,
  findPinByCoords,
  isWorldPin,
  normalizePinCategory,
  type MapJumpData,
  type MapPin,
  type PinCategory,
} from "@data/lib/catalyst/overlay/map-jump";
import { getBridge, sendBridge } from "../../../components/bevy-overlay/bridge";
import {
  mapJumpMachine,
  resolveMapJumpSnapshot,
  slugToState,
  stateToSlug,
  MAP_JUMP_EVENTS,
  type JumpFn,
  type TrackFn,
} from "./machine";

const PARCEL_SIZE = 16;

const bridgeJump: JumpFn = async ({ pin }) => {
  const jumpUrl = buildJumpUrl(pin);
  const bridge = getBridge();
  if (bridge && !isWorldPin(pin)) {
    sendBridge("Teleport", {
      x: pin.x * PARCEL_SIZE + PARCEL_SIZE / 2,
      z: pin.y * PARCEL_SIZE + PARCEL_SIZE / 2,
    });
  }
  return { jumpUrl };
};

export type MapJumpWizardProps = {
  trackCtx: TrackContext;
  data: MapJumpData;
  initialFilter?: string;
  initialSelect?: string;
  initialStep?: string;
  jump?: JumpFn;
  track?: TrackFn;
};

export default function MapJumpWizard(props: MapJumpWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const urlFilter = searchParams.get("filter")?.trim() || props.initialFilter || "";
  const urlSelect = searchParams.get("select")?.trim() || props.initialSelect || "";

  const stateId = slugToState(urlStep);
  const filter = normalizePinCategory(urlFilter);
  const selectedPin = findPinByCoords(props.data.pins, urlSelect);

  const effectiveStep = stateId !== "browsing" && !selectedPin ? "browsing" : stateId;

  return (
    <MapJumpWizardInner
      key={`${effectiveStep}|${selectedPin?.coords ?? ""}`}
      {...props}
      stateId={effectiveStep}
      filter={filter}
      selectedPin={selectedPin}
    />
  );
}

type InnerProps = MapJumpWizardProps & {
  stateId: ReturnType<typeof slugToState>;
  filter: PinCategory;
  selectedPin: MapPin | null;
};

function MapJumpWizardInner({
  trackCtx,
  data,
  stateId,
  filter,
  selectedPin,
  jump,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const emit: TrackFn = track ?? defaultTrack;

  const effectiveJump: JumpFn = jump ?? bridgeJump;

  const snapshot = useRef(
    resolveMapJumpSnapshot({ step: stateId, trackCtx, filter, pin: selectedPin, jump: effectiveJump, track }),
  ).current;

  const [state, send] = useMachine(mapJumpMachine, {
    input: { trackCtx, filter, pin: selectedPin, jump: effectiveJump, track },
    snapshot,
  });

  useEffect(() => {
    emit(MAP_JUMP_EVENTS.opened, { filter }, trackCtx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const value = state.value as string;
  const step = stateToSlug(value);
  const ctxPin = state.context.pin;
  const ctxFilter = state.context.filter;
  const visiblePins = filterPins(data.pins, ctxFilter);

  const lastUrl = useRef<string | null>(null);
  useEffect(() => {
    const want = `${step}|${ctxPin?.coords ?? ""}|${ctxFilter}`;
    if (lastUrl.current === want) return;
    lastUrl.current = want;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "map");
        params.set("step", step);
        if (ctxFilter && ctxFilter !== "all") params.set("filter", ctxFilter);
        else params.delete("filter");
        if (ctxPin?.coords) params.set("select", ctxPin.coords);
        else params.delete("select");
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, ctxPin?.coords, ctxFilter, setSearchParams]);

  return (
    <MapJumpView
      value={value}
      step={step}
      source={data.source}
      unavailableReason={data.source === "unavailable" ? data.reason ?? "" : null}
      pins={visiblePins}
      categories={PIN_CATEGORIES}
      filter={ctxFilter}
      pin={ctxPin}
      setHome={state.context.setHome}
      error={state.context.error}
      jumpUrl={state.context.result?.jumpUrl}
      onSelectPin={(pin) => send({ type: "SELECT_PIN", pin })}
      onFilter={(key) => send({ type: "FILTER", filter: normalizePinCategory(key) })}
      onClear={() => send({ type: "CLEAR" })}
      onConfirm={() => send({ type: "CONFIRM" })}
      onToggleHome={() => send({ type: "TOGGLE_HOME" })}
      onJump={() => send({ type: "JUMP" })}
      onBack={() => send({ type: "BACK" })}
      onRetry={() => send({ type: "RETRY" })}
    />
  );
}
