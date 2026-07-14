import { useEffect, useRef } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import CommunityCreate from "@ui/explorer/components/CommunityCreate";
import "@ui/explorer/components/communitycreate.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  COMMUNITY_BOUNDS,
  validateStep,
  type CommunityDraft,
  type CommunityPrivacy,
  type CommunityVisibility,
  type OwnedPlace,
} from "@data/lib/catalyst/overlay/create-community";
import {
  communityMachine,
  emitStepCompleted,
  FORM_ORDER,
  resolveCommunitySnapshot,
  slugToState,
  stateToSlug,
  STATE_TO_SLUG,
  type CommunityStateId,
  type CreateFn,
  type TrackFn,
} from "./machine";

export type CreateCommunityProps = {
  trackCtx: TrackContext;
  ownedPlaces: OwnedPlace[];
  draft?: CommunityDraft;
  initialStep?: string;
  create?: CreateFn;
  track?: TrackFn;
};

export default function CreateCommunity({
  trackCtx,
  ownedPlaces,
  draft,
  initialStep,
  create,
  track,
}: CreateCommunityProps) {
  const [searchParams] = useSearchParams();

  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <CreateCommunityInner
      key={stateId}
      stateId={stateId}
      trackCtx={trackCtx}
      ownedPlaces={ownedPlaces}
      draft={draft}
      create={create}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: CommunityStateId;
  trackCtx: TrackContext;
  ownedPlaces: OwnedPlace[];
  draft?: CommunityDraft;
  create?: CreateFn;
  track?: TrackFn;
};

function CreateCommunityInner({
  stateId,
  trackCtx,
  ownedPlaces,
  draft,
  create,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();

  const snapshot = useRef(
    resolveCommunitySnapshot({ step: stateId, trackCtx, draft, create, track }),
  ).current;

  const [state, send] = useMachine(communityMachine, {
    input: { trackCtx, draft, create, track },
    snapshot,
  });

  const value = state.value as CommunityStateId;
  const step = stateToSlug(value);
  const d = state.context.draft;
  const issues = validateStep(value, d);
  const trackFn = track ?? state.context.track;

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

  return (
    <div className="create-community">
      <CommunityCreate />
    </div>
  );
}

function advance(
  send: (e: { type: "NEXT" }) => void,
  state: { value: unknown; context: { draft: CommunityDraft } },
  track: TrackFn,
  ctx: TrackContext,
): void {
  const from = state.value as CommunityStateId;
  const idx = FORM_ORDER.indexOf(from);
  const to = idx >= 0 && idx + 1 < FORM_ORDER.length ? FORM_ORDER[idx + 1] : undefined;
  const willAdvance =
    to !== undefined &&
    Object.keys(validateStep(from, state.context.draft)).length === 0;
  send({ type: "NEXT" });
  if (willAdvance && to) emitStepCompleted(track, ctx, from, to);
}

function Breadcrumbs({ active }: { active: CommunityStateId }) {
  const order: CommunityStateId[] = [
    "signinGate",
    "basics",
    "thumbnail",
    "privacy",
    "places",
    "review",
  ];
  return (
    <div className="create-community__crumbs" aria-hidden="true">
      {order.map((s) => (
        <span
          key={s}
          className={"create-community__crumb" + (s === active ? " is-active" : "")}
        >
          {STATE_TO_SLUG[s]}
        </span>
      ))}
    </div>
  );
}

function SigninGateControls({ onSignIn }: { onSignIn: () => void }) {
  return (
    <>
      <p className="create-community__hint">
        Community creation is gated on holding a Decentraland NAME
        (<code>require_owned_name</code>). Sign-in + the NAME check are{" "}
        <strong>simulated</strong> here.
      </p>
      <div className="create-community__row">
        <button
          type="button"
          className="create-community__btn create-community__btn--primary"
          onClick={onSignIn}
        >
          Sign in &amp; continue
        </button>
      </div>
    </>
  );
}

type StepControlsProps = {
  draft: CommunityDraft;
  issues?: Partial<Record<keyof CommunityDraft, string>>;
  onEdit: (patch: Partial<CommunityDraft>) => void;
  onBack: () => void;
  onNext: () => void;
};

function BasicsControls({ draft, issues = {}, onEdit, onBack, onNext }: StepControlsProps) {
  const canNext = !issues.name && !issues.description && draft.name.trim() !== "" && draft.description.trim() !== "";
  return (
    <>
      <div className="create-community__fields">
        <label className="create-community__field">
          <span className="create-community__label">Community name *</span>
          <input
            className="create-community__input"
            maxLength={COMMUNITY_BOUNDS.nameMax}
            placeholder="Write here"
            value={draft.name}
            onChange={(e) => onEdit({ name: e.target.value })}
          />
          {issues.name && <span className="create-community__error">{issues.name}</span>}
        </label>
        <label className="create-community__field">
          <span className="create-community__label">Description *</span>
          <textarea
            className="create-community__textarea"
            maxLength={COMMUNITY_BOUNDS.descriptionMax}
            placeholder="What is your community about?"
            value={draft.description}
            onChange={(e) => onEdit({ description: e.target.value })}
          />
          {issues.description && (
            <span className="create-community__error">{issues.description}</span>
          )}
        </label>
      </div>
      <NavRow onBack={onBack} onNext={onNext} canNext={canNext} />
    </>
  );
}

function ThumbnailControls({ draft, onEdit, onBack, onNext }: StepControlsProps) {
  return (
    <>
      <div className="create-community__fields">
        <span className="create-community__label">Profile picture</span>
        <span className="create-community__hint">PNG or JPG | 512x512 px | 1KB-500KB (optional)</span>
        <label className="create-community__toggle">
          <input
            type="checkbox"
            checked={draft.hasThumbnail}
            onChange={(e) => onEdit({ hasThumbnail: e.target.checked })}
          />
          I&apos;ve chosen a thumbnail (upload simulated)
        </label>
      </div>
      <NavRow onBack={onBack} onNext={onNext} canNext />
    </>
  );
}

function PrivacyControls({ draft, onEdit, onBack, onNext }: StepControlsProps) {
  return (
    <>
      <div className="create-community__fields">
        <label className="create-community__field">
          <span className="create-community__label">Membership</span>
          <select
            className="create-community__select"
            value={draft.privacy}
            onChange={(e) => onEdit({ privacy: e.target.value as CommunityPrivacy })}
          >
            <option value="public">Public &#x2014; anyone can join</option>
            <option value="private">Private &#x2014; members must be approved</option>
          </select>
        </label>
        <label className="create-community__toggle">
          <input
            type="checkbox"
            checked={draft.visibility === "all"}
            onChange={(e) =>
              onEdit({ visibility: (e.target.checked ? "all" : "unlisted") as CommunityVisibility })
            }
          />
          Discoverable in the directory (off = unlisted)
        </label>
      </div>
      <NavRow onBack={onBack} onNext={onNext} canNext />
    </>
  );
}

type PlacesControlsProps = StepControlsProps & { ownedPlaces: OwnedPlace[] };

function PlacesControls({ draft, ownedPlaces, onEdit, onBack, onNext }: PlacesControlsProps) {
  function toggle(id: string, on: boolean) {
    const set = new Set(draft.placeIds);
    if (on) set.add(id);
    else set.delete(id);
    onEdit({ placeIds: Array.from(set) });
  }
  return (
    <>
      <div className="create-community__fields">
        <span className="create-community__label">Places (optional)</span>
        <span className="create-community__hint">
          Attach places you own; the server validates ownership of every place id.
        </span>
        <div className="create-community__places">
          {ownedPlaces.length === 0 && (
            <span className="create-community__hint">You don&apos;t own any places yet.</span>
          )}
          {ownedPlaces.map((p) => (
            <label key={p.id} className="create-community__place">
              <input
                type="checkbox"
                checked={draft.placeIds.includes(p.id)}
                onChange={(e) => toggle(p.id, e.target.checked)}
              />
              {p.title} <span className="create-community__hint">({p.coords})</span>
            </label>
          ))}
        </div>
      </div>
      <NavRow onBack={onBack} onNext={onNext} canNext />
    </>
  );
}

function ReviewControls({
  draft,
  error,
  onBack,
  onSubmit,
}: {
  draft: CommunityDraft;
  error?: string;
  onBack: () => void;
  onSubmit: () => void;
}) {
  return (
    <>
      <div className="create-community__fields">
        <span className="create-community__label">Review</span>
        <span className="create-community__hint">
          <strong>{draft.name || "(unnamed)"}</strong> &#xB7; {draft.privacy} &#xB7;{" "}
          {draft.visibility === "all" ? "listed" : "unlisted"} &#xB7; {draft.placeIds.length} place
          {draft.placeIds.length === 1 ? "" : "s"}
          {draft.hasThumbnail ? " \u{B7} thumbnail" : ""}
        </span>
        {error && <span className="create-community__error">Create failed: {error}. Try again.</span>}
      </div>
      <p className="create-community__sim">
        The signed <code>POST /v1/communities</code> create is <strong>simulated</strong>
        {" "}(it needs a DCL auth-chain signature + a held NAME, and is not exposed on the live catalyst).
      </p>
      <div className="create-community__row">
        <button type="button" className="create-community__btn" onClick={onBack}>
          Back
        </button>
        <button
          type="button"
          className="create-community__btn create-community__btn--primary"
          onClick={onSubmit}
        >
          Create community
        </button>
      </div>
    </>
  );
}

function CreatedControls({ id }: { id?: string }) {
  return (
    <>
      <p className="create-community__hint" role="status">
        Community created (simulated). Id: <code>{id ? `${id.slice(0, 12)}\u{2026}` : "\u{2014}"}</code>
      </p>
      <div className="create-community__row">
        <a className="create-community__btn create-community__btn--primary" href="?step=signin-gate">
          Create another
        </a>
      </div>
    </>
  );
}

function NavRow({
  onBack,
  onNext,
  canNext,
}: {
  onBack: () => void;
  onNext: () => void;
  canNext: boolean;
}) {
  return (
    <div className="create-community__row">
      <button type="button" className="create-community__btn" onClick={onBack}>
        Back
      </button>
      <button
        type="button"
        className="create-community__btn create-community__btn--primary"
        aria-disabled={!canNext}
        disabled={!canNext}
        onClick={onNext}
      >
        Next
      </button>
    </div>
  );
}
