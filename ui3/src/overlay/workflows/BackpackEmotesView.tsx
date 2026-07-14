import ExploreChrome from "../../explorer/frames/ExploreChrome";
import { AvatarStage } from "../../explorer/components/AvatarPreview";
import "../../explorer/pages/backpackemotes.css";
import "./backpackemotesview.css";

type BeSlotBinding = { slot: number; urn: string; name: string };

type BeEmoteOption = {
  urn: string;
  name: string;
  description: string | null;
  thumbnail: string | null;
  rarity: string | null;
  category: string;
  loop: boolean;
};

type BeSaveResult = { entityId: string; count: number };

type BackpackEmotesViewProps = {
  value?: string;
  step?: string;
  profile?: string;
  activeSlot?: number;
  pendingUrn?: string;
  loadout?: BeSlotBinding[];
  slotOrder?: number[];
  catalog?: BeEmoteOption[];
  liveEmpty?: boolean;
  error?: string;
  result?: BeSaveResult;
  onTab?: (id: string) => void;
  onClose?: () => void;
  onOpen?: () => void;
  onPickSlot?: (slot: number) => void;
  onReview?: () => void;
  onAssign?: (urn: string, name: string) => void;
  onConfirm?: () => void;
  onBack?: () => void;
  onSave?: () => void;
  onRetry?: () => void;
};

export default function BackpackEmotesView({
  value = "opening",
  step = "open",
  profile = undefined,
  activeSlot = undefined,
  pendingUrn = undefined,
  loadout = [],
  slotOrder = [],
  catalog = [],
  liveEmpty = false,
  error = undefined,
  result = undefined,
  onTab = undefined,
  onClose = undefined,
  onOpen = undefined,
  onPickSlot = undefined,
  onReview = undefined,
  onAssign = undefined,
  onConfirm = undefined,
  onBack = undefined,
  onSave = undefined,
  onRetry = undefined,
}: BackpackEmotesViewProps) {
  const stagedEmote =
    pendingUrn != null ? catalog.find((e) => e.urn === pendingUrn) : undefined;
  const slotBinding =
    activeSlot != null ? loadout.find((b) => b.slot === activeSlot) : undefined;

  return (
    <ExploreChrome active="backpack" onTab={onTab} onClose={onClose}>
      <div className="bpe" data-step={step} data-slot={activeSlot ?? ""}>
        <header className="bpe__head">
          <h1 className="bpe__title">Backpack</h1>
          <div className="bpe__pills" role="tablist" aria-label="Backpack sections">
            <span className="bpe__pill">
              <span className="bpe__pillicon u-mask-icon bpe__ic-wear" />
              Wearables
            </span>
            <span className="bpe__pill bpe__pill--emotes is-active" aria-current="page">
              <span className="bpe__pillicon u-mask-icon bpe__ic-emote" />
              Emotes
            </span>
          </div>
        </header>

        <div className="bpe__main">
          <div className="bpe__preview">
            <div className="bpe__stage">
              <AvatarStage profile={profile} />
            </div>
          </div>

          <nav className="bpe__slots" aria-label="Equipped emotes">
            {slotOrder.map((n) => {
              const bound = loadout.find((b) => b.slot === n);
              const isActive = activeSlot === n;
              return (
                <button
                  key={n}
                  type="button"
                  className={"bpe__slot" + (isActive ? " is-active" : "")}
                  aria-pressed={isActive}
                  title={bound?.name ?? `Slot ${n}`}
                  disabled={value === "saving" || value === "done"}
                  onClick={() => onPickSlot?.(n)}
                >
                  <span className="bpe__slotnum">{n}</span>
                  <span className="bpe__slotname u-truncate">{bound?.name ?? "Empty"}</span>
                  <span className="bpe__slotart" />
                </button>
              );
            })}
          </nav>

          <section className="bpe__panel">
            <StepPanel
              value={value}
              activeSlot={activeSlot}
              slotBinding={slotBinding}
              stagedEmote={stagedEmote}
              catalog={catalog}
              loadout={loadout}
              liveEmpty={liveEmpty}
              error={error}
              result={result}
              onOpen={onOpen}
              onReview={onReview}
              onAssign={onAssign}
              onConfirm={onConfirm}
              onBack={onBack}
              onSave={onSave}
              onRetry={onRetry}
            />
          </section>
        </div>
      </div>
    </ExploreChrome>
  );
}

function StepPanel(props: {
  value: string;
  activeSlot?: number;
  slotBinding?: BeSlotBinding;
  stagedEmote?: BeEmoteOption;
  catalog: BeEmoteOption[];
  loadout: BeSlotBinding[];
  liveEmpty: boolean;
  error?: string;
  result?: BeSaveResult;
  onOpen?: () => void;
  onReview?: () => void;
  onAssign?: (urn: string, name: string) => void;
  onConfirm?: () => void;
  onBack?: () => void;
  onSave?: () => void;
  onRetry?: () => void;
}) {
  const {
    value,
    activeSlot,
    slotBinding,
    stagedEmote,
    catalog,
    loadout,
    liveEmpty,
    error,
    result,
    onOpen,
    onReview,
    onAssign,
    onConfirm,
    onBack,
    onSave,
    onRetry,
  } = props;

  if (value === "opening") {
    return (
      <>
        <div className="bpe__grid">
          <div className="bpe__gridtop">
            <span className="bpe__emotechip">Emotes</span>
          </div>
          <p className="bpe__empty">
            Pick a numbered slot to assign an emote. Press a number key in-world to
            play the bound emote.
          </p>
        </div>
        <aside className="bpe__detail">
          <button
            type="button"
            className="bpe__cta bpe__cta--primary"
            onClick={onOpen}
          >
            Edit emotes
          </button>
        </aside>
      </>
    );
  }

  if (value === "picking") {
    return (
      <>
        <div className="bpe__grid">
          <div className="bpe__gridtop">
            <span className="bpe__emotechip">Choose a slot</span>
          </div>
          <p className="bpe__empty">
            Select one of the 10 numbered slots on the left to assign an emote to it.
          </p>
        </div>
        <aside className="bpe__detail">
          <button
            type="button"
            className="bpe__cta bpe__cta--primary"
            onClick={onReview}
          >
            Review loadout
          </button>
        </aside>
      </>
    );
  }

  if (value === "browsing") {
    return (
      <>
        <div className="bpe__grid">
          <div className="bpe__gridtop">
            <span className="bpe__emotechip">
              <span className="bpe__emotechipnum">{activeSlot}</span>
              EMOTE {activeSlot}
            </span>
          </div>
          {liveEmpty ? (
            <p className="bpe__note">
              No emotes owned on this realm &#x2014; showing the base/sample emote
              catalogue.
            </p>
          ) : null}
          <div className="bpe__cells">
            {catalog.map((e) => (
              <button
                key={e.urn}
                type="button"
                className={
                  "bpe__cell" + (slotBinding?.urn === e.urn ? " is-active" : "")
                }
                title={`${e.name} \u{B7} ${e.category}`}
                onClick={() => onAssign?.(e.urn, e.name)}
              >
                <span className="bpe__cellname u-truncate">{e.name}</span>
              </button>
            ))}
          </div>
        </div>
        <aside className="bpe__detail">
          <p className="bpe__empty">Select an emote to assign to slot {activeSlot}.</p>
          <button
            type="button"
            className="bpe__cta"
            onClick={onBack}
          >
            Back to slots
          </button>
        </aside>
      </>
    );
  }

  if (value === "assigning") {
    return (
      <>
        <div className="bpe__grid">
          <div className="bpe__gridtop">
            <span className="bpe__emotechip">
              <span className="bpe__emotechipnum">{activeSlot}</span>
              EMOTE {activeSlot}
            </span>
          </div>
          <div className="bpe__cells">
            {catalog.map((e) => (
              <span
                key={e.urn}
                className={
                  "bpe__cell" + (stagedEmote?.urn === e.urn ? " is-active" : "")
                }
                title={e.name}
              >
                <span className="bpe__cellname u-truncate">{e.name}</span>
              </span>
            ))}
          </div>
        </div>
        <aside className="bpe__detail">
          <h2 className="bpe__detailname">{stagedEmote?.name ?? "Emote"}</h2>
          <p className="bpe__detailmeta">
            {stagedEmote?.category}
            {stagedEmote?.loop ? " \u{B7} loops" : " \u{B7} one-shot"}
          </p>
          <p className="bpe__detaildesc">{stagedEmote?.description}</p>
          <div className="bpe__detailbtns">
            <button
              type="button"
              className="bpe__cta"
              onClick={onBack}
            >
              Back
            </button>
            <button
              type="button"
              className="bpe__cta bpe__cta--primary"
              onClick={onConfirm}
            >
              Assign to slot {activeSlot}
            </button>
          </div>
        </aside>
      </>
    );
  }

  if (value === "reviewing") {
    return (
      <>
        <div className="bpe__grid">
          <div className="bpe__gridtop">
            <span className="bpe__emotechip">Review loadout</span>
          </div>
          <ul className="bpe__loadout">
            {loadout.map((b) => (
              <li key={b.slot} className="bpe__loadoutrow">
                <span className="bpe__slotnum">{b.slot}</span>
                <span className="u-truncate">{b.name}</span>
              </li>
            ))}
          </ul>
        </div>
        <aside className="bpe__detail">
          <p className="bpe__detailmeta">{loadout.length} of 10 slots bound</p>
          <div className="bpe__detailbtns">
            <button
              type="button"
              className="bpe__cta"
              onClick={onBack}
            >
              Keep editing
            </button>
            <button
              type="button"
              className="bpe__cta bpe__cta--primary"
              onClick={onSave}
            >
              Save loadout
            </button>
          </div>
        </aside>
      </>
    );
  }

  if (value === "saving") {
    return (
      <div className="bpe__grid bpe__grid--center">
        <p className="bpe__progress">Saving emote loadout&#x2026;</p>
        <p className="bpe__note">Profile deploy is simulated (no on-chain write).</p>
      </div>
    );
  }

  if (value === "done") {
    return (
      <div className="bpe__grid bpe__grid--center">
        <p className="bpe__progress">Emote loadout saved (simulated).</p>
        <p className="bpe__note">
          {result?.count ?? loadout.length} slots written &#xB7; entity {result?.entityId}
        </p>
      </div>
    );
  }

  if (value === "error") {
    return (
      <div className="bpe__grid bpe__grid--center">
        <p className="bpe__progress">Save failed: {error}</p>
        <button
          type="button"
          className="bpe__cta bpe__cta--primary"
          onClick={onRetry}
        >
          Retry
        </button>
      </div>
    );
  }

  return null;
}
