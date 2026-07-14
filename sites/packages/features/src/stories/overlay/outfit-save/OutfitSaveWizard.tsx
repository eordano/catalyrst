import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import ExploreChrome from "@ui/explorer/frames/ExploreChrome";
import { AvatarStage } from "@ui/explorer/components/AvatarPreview";
import "@ui/explorer/pages/backpackoutfits.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  isExtraSlot,
  type EquippedSet,
} from "@data/lib/catalyst/overlay/outfit-save";
import {
  outfitSaveMachine,
  resolveOutfitSnapshot,
  slugToState,
  valueToSlug,
  type OutfitSaveSeed,
  type SaveFn,
  type TrackFn,
} from "./machine";

export type OutfitSaveWizardProps = {
  trackCtx: TrackContext;
  seed: OutfitSaveSeed;
  profile?: string;
  initialStep?: string;
  initialSlot?: number;
  save?: SaveFn;
  track?: TrackFn;
};

const noop = () => {};

export default function OutfitSaveWizard({
  trackCtx,
  seed,
  profile,
  initialStep,
  initialSlot,
  save,
  track,
}: OutfitSaveWizardProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  const rawSlot = searchParams.get("slot");
  const parsedSlot = rawSlot != null ? Number.parseInt(rawSlot, 10) : initialSlot;
  const slot =
    Number.isFinite(parsedSlot) && (parsedSlot as number) >= 0
      ? (parsedSlot as number)
      : 0;

  return (
    <OutfitSaveWizardInner
      key={`${stateId}:${slot}`}
      stateId={stateId}
      slot={slot}
      trackCtx={trackCtx}
      seed={seed}
      profile={profile}
      save={save}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  slot: number;
  trackCtx: TrackContext;
  seed: OutfitSaveSeed;
  profile?: string;
  save?: SaveFn;
  track?: TrackFn;
};

function OutfitSaveWizardInner({
  stateId,
  slot,
  trackCtx,
  seed,
  profile,
  save,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveOutfitSnapshot({ step: stateId, trackCtx, seed, save, track, slot }),
  ).current;

  const [state, send] = useMachine(outfitSaveMachine, {
    input: { trackCtx, seed, save, track },
    snapshot,
  });

  const value = state.value as string;
  const step = valueToSlug(value);
  const ctx = state.context;

  const lastKey = useRef<string | null>(null);
  useEffect(() => {
    const key = `${step}:${ctx.slot}`;
    if (lastKey.current === key) return;
    lastKey.current = key;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "backpack");
        params.set("tab", "outfits");
        if (params.get("step") === step && params.get("slot") === String(ctx.slot)) {
          return params;
        }
        params.set("step", step);
        params.set("slot", String(ctx.slot));
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, ctx.slot, setSearchParams]);

  const onClose = noop;
  const onTab = noop;

  return (
    <ExploreChrome active="backpack" onTab={onTab} onClose={onClose}>
      <div className="bpo" data-step={step} data-slot={ctx.slot}>
        <div className="bpo__bar">
          <h1 className="bpo__pagetitle">Backpack</h1>
        </div>

        <div className="bpo__main">
          <div className="bpo__preview">
            <div className="bpo__stage">
              <AvatarStage profile={profile} />
            </div>
          </div>

          <section className="bpo__panel">
            <div className="bpo__panelhead">
              <div className="bpo__subtabs" role="tablist" aria-label="Backpack view">
                <span className="bpo__subtab is-active" role="tab" aria-selected="true">
                  SAVED OUTFITS
                </span>
              </div>
            </div>

            {value === "browsing" && (
              <BrowsingStep
                seed={seed}
                onOpen={(s) => send({ type: "OPEN_SLOT", slot: s })}
              />
            )}

            {value === "naming" && (
              <NamingStep
                slot={ctx.slot}
                name={ctx.name}
                seed={seed}
                onName={(name) => send({ type: "SET_NAME", name })}
                onNext={() => send({ type: "NEXT" })}
                onBack={() => send({ type: "BACK" })}
              />
            )}

            {value === "capturing" && (
              <CapturingStep
                slot={ctx.slot}
                name={ctx.name}
                equipped={seed.equipped}
                onCapture={() => send({ type: "CAPTURE" })}
                onBack={() => send({ type: "BACK" })}
              />
            )}

            {(value === "saving" || value === "persisting") && (
              <SavingStep slot={ctx.slot} name={ctx.name} />
            )}

            {value === "gated" && (
              <GatedStep
                slot={ctx.slot}
                reason={ctx.gateReason}
                onBack={() => send({ type: "BACK" })}
              />
            )}

            {value === "done" && <DoneStep slot={ctx.slot} name={ctx.name} captured={ctx.captured} />}
          </section>
        </div>
      </div>
    </ExploreChrome>
  );
}

function SlotSilhouette() {
  return (
    <span className="bpo__silhouette" aria-hidden="true">
      <svg viewBox="0 0 60 130" width="52" height="112">
        <circle cx="30" cy="20" r="14" />
        <path d="M14 56c0-9 7-16 16-16s16 7 16 16v34c0 6-32 6-32 0V56Z" />
        <rect x="6" y="56" width="9" height="40" rx="4.5" />
        <rect x="45" y="56" width="9" height="40" rx="4.5" />
      </svg>
    </span>
  );
}

function BrowsingStep({
  seed,
  onOpen,
}: {
  seed: OutfitSaveSeed;
  onOpen: (slot: number) => void;
}) {
  const slots = Array.from({ length: seed.totalSlots }, (_, i) => i);
  const hasName = seed.namesForExtraSlots.length > 0;
  return (
    <>
      <p className="bpo__promotext">
        Snapshot your current look into a slot. The first {seed.freeSlots} slots are
        free; the rest need a NAME.
      </p>
      <div className="bpo__slots">
        <button
          type="button"
          className="bpo__slot bpo__slot--save"
          onClick={() => onOpen(0)}
        >
          <span className="bpo__plus" aria-hidden="true">
            <svg viewBox="0 0 24 24" width="34" height="34" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round">
              <path d="M12 5v14M5 12h14" />
            </svg>
          </span>
          <span className="bpo__slotlabel">SAVE OUTFIT</span>
        </button>

        {slots.map((i) => {
          const locked = isExtraSlot(i, seed.freeSlots) && !hasName;
          return (
            <button
              key={i}
              type="button"
              className="bpo__slot bpo__slot--empty"
              onClick={() => onOpen(i)}
              aria-label={`Empty slot ${i + 1}${locked ? " (locked \u{2014} needs a NAME)" : ""}`}
              data-locked={locked || undefined}
            >
              <SlotSilhouette />
              <span className="bpo__emptylabel">
                Empty<br />Slot {i + 1}
                {locked ? <><br />&#x1F512; NAME</> : null}
              </span>
            </button>
          );
        })}
      </div>
    </>
  );
}

function NamingStep({
  slot,
  name,
  seed,
  onName,
  onNext,
  onBack,
}: {
  slot: number;
  name: string;
  seed: OutfitSaveSeed;
  onName: (name: string) => void;
  onNext: () => void;
  onBack: () => void;
}) {
  const extra = isExtraSlot(slot, seed.freeSlots);
  return (
    <div className="bpo__step">
      <h2 className="bpo__promotitle">Name your outfit (slot {slot + 1})</h2>
      <div className="bpo__search">
        <input
          type="text"
          value={name}
          placeholder="Outfit name"
          aria-label="Outfit name"
          onChange={(e) => onName(e.target.value)}
        />
      </div>
      {extra ? (
        <p className="bpo__promotext">
          Slot {slot + 1} is a NAME-gated slot &#x2014; a NAME is required to name and
          unlock it.
        </p>
      ) : null}
      <div className="bpo__controls">
        <button type="button" className="bpo__filter" onClick={onBack}>
          Back
        </button>
        <button type="button" className="bpo__getname" onClick={onNext}>
          Next: capture look
        </button>
      </div>
    </div>
  );
}

function CapturingStep({
  slot,
  name,
  equipped,
  onCapture,
  onBack,
}: {
  slot: number;
  name: string;
  equipped: EquippedSet;
  onCapture: () => void;
  onBack: () => void;
}) {
  return (
    <div className="bpo__step">
      <h2 className="bpo__promotitle">
        Capture current look{name ? ` \u{2014} "${name}"` : ""}
      </h2>
      <p className="bpo__promotext">
        {equipped.wearables.length} equipped wearables will be snapshotted into
        slot {slot + 1}.
      </p>
      <ul className="bpo__promotext">
        {equipped.wearables.slice(0, 6).map((w) => (
          <li key={w}>{w.split(":").pop()}</li>
        ))}
      </ul>
      <div className="bpo__controls">
        <button type="button" className="bpo__filter" onClick={onBack}>
          Back
        </button>
        <button type="button" className="bpo__getname" onClick={onCapture}>
          Capture &amp; save
        </button>
      </div>
    </div>
  );
}

function SavingStep({ slot, name }: { slot: number; name: string }) {
  return (
    <div className="bpo__step" aria-busy="true">
      <h2 className="bpo__promotitle">Saving outfit&#x2026;</h2>
      <p className="bpo__promotext">
        Saving {name ? `"${name}"` : "your look"} to slot {slot + 1} (simulated &#x2014;
        the content write is read-only at this gateway).
      </p>
    </div>
  );
}

function GatedStep({
  slot,
  reason,
  onBack,
}: {
  slot: number;
  reason?: string;
  onBack: () => void;
}) {
  const msg =
    reason === "needs-name"
      ? "This extra slot needs a NAME \u{2014} go back and name the outfit."
      : reason === "no-name-unlock"
        ? "This is a NAME-gated slot. Get a NAME to unlock 5 more Outfit slots."
        : reason === "out-of-range"
          ? "That slot does not exist."
          : "Save was blocked.";
  return (
    <div className="bpo__promo" role="alert">
      <div className="bpo__promobody">
        <div className="bpo__promotitle">Slot {slot + 1} is locked</div>
        <p className="bpo__promotext">{msg}</p>
        <div className="bpo__controls">
          <button type="button" className="bpo__filter" onClick={onBack}>
            Back
          </button>
          <button
            type="button"
            className="bpo__getname"
            onClick={() => {
              if (typeof window !== "undefined")
                window.open("https://catalyst.example.com/marketplace/claim-name", "_blank", "noopener");
            }}
          >
            GET A NAME
          </button>
        </div>
      </div>
    </div>
  );
}

function DoneStep({
  slot,
  name,
  captured,
}: {
  slot: number;
  name: string;
  captured?: EquippedSet;
}) {
  return (
    <div className="bpo__step" role="status">
      <h2 className="bpo__promotitle">Outfit saved &#x2713;</h2>
      <p className="bpo__promotext">
        {name ? `"${name}"` : "Your outfit"} is saved to slot {slot + 1}
        {captured ? ` (${captured.wearables.length} wearables)` : ""}. (Simulated
        save &#x2014; flow + metrics are real.)
      </p>
    </div>
  );
}
